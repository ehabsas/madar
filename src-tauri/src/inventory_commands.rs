#[tauri::command]
fn list_stock(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<StockRow>, String> {
    let _actor = require_permission(state.inner(), "stock.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let q = query.trim();
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        "SELECT p.id,p.code,p.name,p.unit,
                COALESCE((SELECT b.barcode FROM product_barcodes b WHERE b.product_id=p.id AND b.is_primary=1 LIMIT 1),''),
                COALESCE(SUM(sm.quantity_milli),0),p.minimum_quantity_milli,p.is_active
         FROM products p
         LEFT JOIN stock_movements sm ON sm.product_id=p.id
         WHERE (?1='' OR p.name LIKE ?2 OR p.code LIKE ?2 OR EXISTS(
             SELECT 1 FROM product_barcodes bx WHERE bx.product_id=p.id AND bx.barcode LIKE ?2
         ))
         GROUP BY p.id,p.code,p.name,p.unit,p.minimum_quantity_milli,p.is_active
         ORDER BY p.is_active DESC,p.name COLLATE NOCASE LIMIT 1000"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![q, like], |r| Ok(StockRow {
        product_id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        unit: r.get(3)?,
        barcode: r.get(4)?,
        quantity_milli: r.get(5)?,
        minimum_quantity_milli: r.get(6)?,
        is_active: r.get::<_, i64>(7)? != 0,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_stock_movements(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<StockMovementRow>, String> {
    let _actor = require_permission(state.inner(), "stock.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let mut stmt = conn.prepare(
        "SELECT sm.id,p.code,p.name,COALESCE(sm.unit,p.unit,''),sm.quantity_milli,COALESCE(sm.notes,''),sm.created_at
         FROM stock_movements sm JOIN products p ON p.id=sm.product_id
         ORDER BY sm.id DESC LIMIT 100"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(StockMovementRow {
        id: r.get(0)?,
        product_code: r.get(1)?,
        product_name: r.get(2)?,
        unit: r.get(3)?,
        quantity_milli: r.get(4)?,
        notes: r.get(5)?,
        created_at: r.get(6)?,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn record_stock_movement(
    app: tauri::AppHandle, state: State<'_, AppState>, product_id: i64,
    direction: String,
    quantity_milli: i64,
    notes: Option<String>,
) -> Result<String, String> {
    let actor = require_permission(state.inner(), "stock.movement.record")?;
    if quantity_milli <= 0 {
        return Err("الكمية لازم تكون أكبر من صفر".into());
    }
    let signed_quantity = match direction.as_str() {
        "in" => quantity_milli,
        "out" => -quantity_milli,
        _ => return Err("نوع الحركة غير صالح؛ اختر وارد أو منصرف".into()),
    };

    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let product: Option<(String, String, String)> = tx.query_row(
        "SELECT code,name,unit FROM products WHERE id=?1 AND is_active=1",
        [product_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).optional().map_err(|e| e.to_string())?;
    let Some((product_code, product_name, unit)) = product else {
        return Err("الصنف غير موجود أو موقوف؛ أعد تفعيله قبل تسجيل حركة جديدة".into());
    };

    let warehouse_id: i64 = tx.query_row(
        "SELECT id FROM warehouses WHERE code='MAIN' AND is_active=1 LIMIT 1",
        [], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    let current_balance: i64 = tx.query_row(
        "SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1 AND warehouse_id=?2",
        params![product_id, warehouse_id], |r| r.get(0),
    ).map_err(|e| e.to_string())?;

    if direction == "out" && quantity_milli > current_balance {
        return Err(format!(
            "الرصيد المتاح للصنف «{}» هو {} {} فقط؛ لا يمكن صرف {} {}.",
            product_name,
            current_balance as f64 / 1000.0,
            unit,
            quantity_milli as f64 / 1000.0,
            unit,
        ));
    }

    let note = notes.unwrap_or_default().trim().to_string();
    let reference_type = if direction == "in" { "manual_in" } else { "manual_out" };
    tx.execute(
        "INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,notes,unit)
         VALUES (?1,?2,'adjustment',?3,?4,?5,?6)",
        params![product_id, warehouse_id, signed_quantity, reference_type, if note.is_empty() { None } else { Some(note.as_str()) }, unit],
    ).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), if direction == "in" { "stock_in" } else { "stock_out" }, "stock", Some(&product_id.to_string()), &format!("code={product_code}; quantity_milli={quantity_milli}; unit={unit}; note={note}"))?;
    tx.commit().map_err(|e| e.to_string())?;

    Ok(format!(
        "تم تسجيل {} للصنف «{}» بكمية {} {}.",
        if direction == "in" { "الوارد" } else { "المنصرف" },
        product_name,
        quantity_milli as f64 / 1000.0,
        unit,
    ))
}
