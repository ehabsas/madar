#[tauri::command]
fn create_product(app: tauri::AppHandle, state: State<'_, AppState>, code: String, name: String, unit: Option<String>, barcode: Option<String>, purchase_price_minor: i64, sale_price_minor: i64, minimum_quantity_milli: i64) -> Result<ProductRow, String> {
    let actor = require_permission(state.inner(), "products.create")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let product_code = code.trim();
    let product_name = name.trim();
    if product_code.is_empty() || product_name.is_empty() { return Err("كود الصنف واسم الصنف مطلوبان".into()); }
    if purchase_price_minor < 0 || sale_price_minor < 0 { return Err("السعر لا يمكن أن يكون سالبًا".into()); }
    if minimum_quantity_milli < 0 { return Err("الحد الأدنى للمخزون لا يمكن أن يكون سالبًا".into()); }
    let unit_value = unit.unwrap_or_else(|| "قطعة".to_string());
    let supplied = barcode.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let (barcode_value, barcode_type) = match supplied {
        Some(value) => (validate_barcode(&value)?, "legacy"),
        None => {
            let generated = next_internal_barcode(&tx)?;
            (generated, "internal")
        }
    };
    let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM product_barcodes WHERE barcode=?1)", [&barcode_value], |r| r.get(0)).map_err(|e| e.to_string())?;
    if duplicate { return Err("الباركود مستخدم بالفعل لصنف آخر. أدخل باركود مختلفًا.".into()); }
    let purchase_price_minor = if actor.permissions.iter().any(|p| p == "products.purchase_price.edit") { purchase_price_minor } else { 0 };
    let sale_price_minor = if actor.permissions.iter().any(|p| p == "products.sale_price.edit") { sale_price_minor } else { 0 };
    tx.execute("INSERT INTO products(code,name,unit,purchase_price_minor,sale_price_minor,minimum_quantity_milli) VALUES (?1,?2,?3,?4,?5,?6)", params![product_code, product_name, unit_value, purchase_price_minor, sale_price_minor, minimum_quantity_milli]).map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,?4,1)", params![id, unit_value, barcode_value, barcode_type]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "create", "product", Some(&id.to_string()), &format!("code={}; barcode={}", product_code, barcode_value))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ProductRow { id, code: product_code.to_string(), name: product_name.to_string(), unit: unit_value, barcode: barcode_value, purchase_price_minor, sale_price_minor, minimum_quantity_milli })
}

#[tauri::command]
fn find_product_by_barcode(app: tauri::AppHandle, state: State<'_, AppState>, barcode: String) -> Result<Option<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    conn.query_row(
        "SELECT p.id,p.code,p.name,p.unit,b.barcode,p.purchase_price_minor,p.sale_price_minor,p.minimum_quantity_milli FROM product_barcodes b JOIN products p ON p.id=b.product_id WHERE b.barcode=?1 AND p.is_active=1 LIMIT 1",
        [barcode.trim()], |r| Ok(ProductRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, unit:r.get(3)?, barcode:r.get(4)?, purchase_price_minor:r.get(5)?, sale_price_minor:r.get(6)?, minimum_quantity_milli:r.get(7)? })
    ).optional().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_dashboard_inventory_counts(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // Return aggregate counts only for sections this signed-in user may view.
    let actor = {
        let guard = state.session.lock().map_err(|_| "تعذر الوصول إلى جلسة المستخدم")?;
        guard.as_ref().ok_or_else(|| "يلزم تسجيل الدخول أولًا".to_string())?.clone()
    };
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let product_count = if actor.permissions.iter().any(|p| p == "products.view") {
        Some(conn.query_row("SELECT COUNT(*) FROM products WHERE is_active=1", [], |r| r.get::<_,i64>(0)).map_err(|e| e.to_string())?)
    } else { None };
    let in_stock_product_count = if actor.permissions.iter().any(|p| p == "stock.view") {
        Some(conn.query_row("SELECT COUNT(*) FROM products p WHERE p.is_active=1 AND (SELECT COALESCE(SUM(sm.quantity_milli),0) FROM stock_movements sm WHERE sm.product_id=p.id)>0", [], |r| r.get::<_,i64>(0)).map_err(|e| e.to_string())?)
    } else { None };
    Ok(serde_json::json!({"productCount":product_count,"inStockProductCount":in_stock_product_count}))
}

#[tauri::command]
fn list_products(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, "", true)
}

#[tauri::command]
fn search_products(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, &query, true)
}

#[tauri::command]
fn list_inactive_products(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, "", false)
}

#[tauri::command]
fn search_inactive_products(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, &query, false)
}

#[tauri::command]
fn reactivate_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.reactivate")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let changed = tx.execute(
        "UPDATE products SET is_active=1,updated_at=datetime('now') WHERE id=?1 AND is_active=0",
        [id],
    ).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("الصنف غير موجود أو نشط بالفعل".into()); }
    write_audit(&tx, Some(&actor), "reactivate", "product", Some(&id.to_string()), "product reactivated; existing data and barcodes retained")?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok("تمت إعادة تفعيل الصنف مع الاحتفاظ ببياناته وباركوداته".into())
}

#[tauri::command]
fn update_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64, code: String, name: String, unit: String, barcode: String, purchase_price_minor: i64, sale_price_minor: i64, minimum_quantity_milli: i64) -> Result<ProductRow, String> {
    let actor = require_permission(state.inner(), "products.edit")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| format!("تعذر بدء تعديل الصنف: {e}"))?;

    let code = code.trim();
    let name = name.trim();
    let unit = unit.trim();
    let barcode = validate_barcode(&barcode)?;
    if code.is_empty() || name.is_empty() || unit.is_empty() {
        return Err("الكود والاسم والوحدة حقول مطلوبة".into());
    }
    if barcode.is_empty() {
        return Err("الباركود الأساسي لا يمكن أن يكون فارغًا".into());
    }
    if purchase_price_minor < 0 || sale_price_minor < 0 {
        return Err("السعر لا يمكن أن يكون سالبًا".into());
    }
    if minimum_quantity_milli < 0 { return Err("الحد الأدنى للمخزون لا يمكن أن يكون سالبًا".into()); }

    let current: Option<(String, i64, i64)> = tx.query_row(
        "SELECT unit,purchase_price_minor,sale_price_minor FROM products WHERE id=?1 AND is_active=1",
        [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    ).optional().map_err(|e| format!("تعذر قراءة بيانات الصنف: {e}"))?;
    let Some((current_unit, current_purchase_price, current_sale_price)) = current else {
        return Err("الصنف غير موجود أو موقوف".into());
    };
    if purchase_price_minor != current_purchase_price && !actor.permissions.iter().any(|p| p == "products.purchase_price.edit") {
        return Err("ليس لديك صلاحية تعديل سعر الشراء".into());
    }
    if sale_price_minor != current_sale_price && !actor.permissions.iter().any(|p| p == "products.sale_price.edit") {
        return Err("ليس لديك صلاحية تعديل سعر البيع".into());
    }
    if current_unit != unit {
        let draft_refs: i64 = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM purchase_invoice_items li JOIN purchase_invoices h ON h.id=li.purchase_invoice_id WHERE li.product_id=?1 AND h.status='draft') + (SELECT COUNT(*) FROM sale_items li JOIN sales h ON h.id=li.sale_id WHERE li.product_id=?1 AND h.status='draft')",
            [id], |r| r.get(0)
        ).map_err(|e| format!("تعذر التحقق من فواتير الصنف: {e}"))?;
        if draft_refs > 0 {
            return Err("لا يمكن تغيير وحدة الصنف مع وجود فواتير مسودة تستخدمه؛ عدّل أو ألغِ المسودات أولًا.".into());
        }
        let on_hand: i64 = tx.query_row(
            "SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1",
            [id], |r| r.get(0)
        ).map_err(|e| format!("تعذر التحقق من رصيد الصنف: {e}"))?;
        if on_hand != 0 {
            return Err(format!("لا يمكن تغيير وحدة الصنف إلى «{unit}» لأن له رصيدًا حاليًا. صفّر الرصيد أو عالج تحويل الوحدة أولًا حتى لا تتغير دلالة الكميات المسجلة."));
        }
    }

    let current_primary: Option<(i64, String)> = tx.query_row(
        "SELECT id,barcode FROM product_barcodes WHERE product_id=?1 AND is_primary=1 LIMIT 1",
        [id], |r| Ok((r.get(0)?, r.get(1)?))
    ).optional().map_err(|e| format!("تعذر قراءة الباركود الأساسي الحالي: {e}"))?;
    let previous_barcode = current_primary.as_ref().map(|(_, value)| value.as_str().to_owned()).unwrap_or_default();
    let barcode_changed = previous_barcode.as_str() != barcode.as_str();

    // Validate ownership before modifying any primary flag. The transaction protects all changes as one unit.
    let mut target_barcode_row: Option<i64> = current_primary.as_ref().and_then(|(row_id, current)| {
        if current.as_str() == barcode.as_str() { Some(*row_id) } else { None }
    });
    if barcode_changed {
        if !actor.permissions.iter().any(|p| p == "products.barcode.manage") {
            return Err("لا تملك صلاحية تغيير الباركود الأساسي".into());
        }
        let existing: Option<(i64, i64)> = tx.query_row(
            "SELECT id,product_id FROM product_barcodes WHERE barcode=?1 LIMIT 1",
            [barcode.as_str()], |r| Ok((r.get(0)?, r.get(1)?))
        ).optional().map_err(|e| format!("تعذر التحقق من الباركود: {e}"))?;
        if let Some((row_id, owner_id)) = existing {
            if owner_id != id {
                return Err("الباركود مستخدم بالفعل لصنف آخر. أدخل باركود مختلفًا.".into());
            }
            target_barcode_row = Some(row_id);
        } else {
            // Create it as an alternate first. It becomes primary only after all validation is complete.
            tx.execute(
                "INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,'alternate',0)",
                params![id, unit, barcode]
            ).map_err(|e| format!("تعذر تسجيل الباركود الجديد: {e}"))?;
            target_barcode_row = Some(tx.last_insert_rowid());
        }
    }

    // Update product fields only after validation. A failure below rolls the whole transaction back.
    tx.execute(
        "UPDATE products SET code=?1,name=?2,unit=?3,purchase_price_minor=?4,sale_price_minor=?5,minimum_quantity_milli=?6,updated_at=datetime('now') WHERE id=?7",
        params![code, name, unit, purchase_price_minor, sale_price_minor, minimum_quantity_milli, id]
    ).map_err(|e| format!("تعذر تعديل الصنف (تأكد أن الكود غير مستخدم): {e}"))?;

    if barcode_changed {
        tx.execute(
            "UPDATE product_barcodes SET is_primary=0 WHERE product_id=?1 AND is_primary=1",
            [id]
        ).map_err(|e| format!("تعذر إلغاء تعيين الباركود الأساسي السابق: {e}"))?;
        let row_id = target_barcode_row.ok_or_else(|| "تعذر تحديد سجل الباركود الجديد".to_string())?;
        let changed = tx.execute(
            "UPDATE product_barcodes SET is_primary=1,unit=?1 WHERE id=?2 AND product_id=?3",
            params![unit, row_id, id]
        ).map_err(|e| format!("تعذر تفعيل الباركود الأساسي الجديد: {e}"))?;
        if changed != 1 {
            return Err("لم يتم العثور على سجل الباركود لتفعيله؛ لم يتم حفظ التعديل".into());
        }
    } else {
        tx.execute(
            "UPDATE product_barcodes SET unit=?1 WHERE product_id=?2 AND is_primary=1",
            params![unit, id]
        ).map_err(|e| format!("تعذر تحديث وحدة الباركود الأساسي: {e}"))?;
    }

    let audit_barcode_detail = if barcode_changed {
        format!("primary_barcode_changed_from={previous_barcode}; primary_barcode={barcode}; old_barcode_preserved_as_alternate=true")
    } else {
        format!("primary_barcode={barcode}; primary_barcode_changed=false")
    };
    write_audit(&tx, Some(&actor), "update", "product", Some(&id.to_string()), &format!("code={code}; name={name}; unit={unit}; {audit_barcode_detail}"))
        .map_err(|e| format!("تعذر تسجيل التعديل في سجل النشاط: {e}"))?;
    let row = tx.query_row(
        "SELECT p.id,p.code,p.name,p.unit,COALESCE((SELECT barcode FROM product_barcodes WHERE product_id=p.id AND is_primary=1 LIMIT 1),''),p.purchase_price_minor,p.sale_price_minor,p.minimum_quantity_milli FROM products p WHERE p.id=?1",
        [id], |r| Ok(ProductRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, unit:r.get(3)?, barcode:r.get(4)?, purchase_price_minor:r.get(5)?, sale_price_minor:r.get(6)?, minimum_quantity_milli:r.get(7)? })
    ).map_err(|e| format!("تم تحديث البيانات لكن تعذر إعادة قراءة الصنف قبل اعتماد التعديل: {e}"))?;
    tx.commit().map_err(|e| format!("تعذر اعتماد تعديل الصنف: {e}"))?;
    Ok(row)
}

#[tauri::command]
fn add_alternate_barcode(app: tauri::AppHandle, state: State<'_, AppState>, product_id: i64, barcode: String, unit: Option<String>) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.barcode.manage")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let barcode = validate_barcode(&barcode)?;
    let product_unit: Option<String> = tx.query_row("SELECT unit FROM products WHERE id=?1 AND is_active=1", [product_id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(product_unit) = product_unit else { return Err("الصنف غير موجود أو موقوف".into()); };
    let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM product_barcodes WHERE barcode=?1)", [barcode.as_str()], |r| r.get(0)).map_err(|e| e.to_string())?;
    if duplicate { return Err("هذا الباركود مستخدم بالفعل، ولن يتم ربطه بصنف آخر".into()); }
    let unit = unit.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or(product_unit);
    tx.execute("INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,'alternate',0)", params![product_id,unit,barcode]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "add_alternate_barcode", "product", Some(&product_id.to_string()), &format!("barcode={barcode}; unit={unit}"))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(format!("تمت إضافة الباركود البديل {barcode}"))
}

#[tauri::command]
fn deactivate_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.deactivate")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let changed = tx.execute("UPDATE products SET is_active=0,updated_at=datetime('now') WHERE id=?1 AND is_active=1", [id]).map_err(|e| e.to_string())?;
    if changed == 0 { return Err("الصنف غير موجود أو موقوف بالفعل".into()); }
    write_audit(&tx, Some(&actor), "deactivate", "product", Some(&id.to_string()), "soft delete; data and barcodes retained")?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok("تم إيقاف الصنف مع الاحتفاظ ببياناته وباركوداته".into())
}
