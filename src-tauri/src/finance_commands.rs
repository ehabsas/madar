#[tauri::command]
fn get_company_branding(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<serde_json::Value,String> {
    // Brand identity is visible to every signed-in user, while contact/tax details remain protected.
    let _user = {
        let guard = state.session.lock().map_err(|_| "تعذر الوصول إلى جلسة المستخدم")?;
        guard.as_ref().ok_or_else(|| "يلزم تسجيل الدخول أولًا".to_string())?.clone()
    };
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let read = |key: &str| -> Result<String,String> {
        conn.query_row("SELECT value FROM app_settings WHERE key=?1", [key], |row| row.get(0))
            .optional().map(|value: Option<String>| value.unwrap_or_default()).map_err(|e| e.to_string())
    };
    Ok(serde_json::json!({"name":read("company_name")?,"logoDataUrl":read("company_logo_data_url")?,"currencyCode":read("currency_code")?}))
}

#[tauri::command]
fn get_company_profile(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<serde_json::Value,String> {
    {
        let guard = state.session.lock().map_err(|_| "تعذر الوصول إلى جلسة المستخدم")?;
        let user = guard.as_ref().ok_or_else(|| "يلزم تسجيل الدخول أولًا".to_string())?;
        if !user.permissions.iter().any(|p| ["company.settings.view","company.settings.manage"].contains(&p.as_str())) {
            return Err("ليس لديك صلاحية عرض بيانات الشركة".into());
        }
    }
    let path=database_path(&app)?; initialize_database(&path)?; let conn=connect(&path)?;
    read_company_profile_json(&conn)
}

#[tauri::command]
fn list_cashbook(app:tauri::AppHandle,state:State<'_,AppState>)->Result<serde_json::Value,String>{let _actor=require_permission(state.inner(),"cashbox.view")?;let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;let mut stmt=conn.prepare("SELECT id,direction,amount_minor,entry_date,source_type,description,notes,created_by_username,created_at FROM cashbook_entries ORDER BY id DESC LIMIT 500").map_err(|e|e.to_string())?;let rows=stmt.query_map([],|r|Ok(CashbookRow{id:r.get(0)?,direction:r.get(1)?,amount_minor:r.get(2)?,entry_date:r.get(3)?,source_type:r.get(4)?,description:r.get(5)?,notes:r.get(6)?,created_by_username:r.get(7)?,created_at:r.get(8)?})).map_err(|e|e.to_string())?;let entries=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let balance:i64=conn.query_row("SELECT COALESCE(SUM(CASE direction WHEN 'in' THEN amount_minor ELSE -amount_minor END),0) FROM cashbook_entries",[],|r|r.get(0)).map_err(|e|e.to_string())?;Ok(serde_json::json!({"balanceMinor":balance,"entries":entries}))}
