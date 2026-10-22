#[tauri::command]
fn auth_status(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let user_count: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let current = state.session.lock().map_err(|_| "تعذر قراءة جلسة المستخدم")?.clone();
    Ok(serde_json::json!({"setupRequired": user_count == 0, "currentUser": current}))
}

#[tauri::command]
fn create_initial_admin(app: tauri::AppHandle, state: State<'_, AppState>, username: String, display_name: String, password: String) -> Result<UserSession, String> {
    let username = validate_username(&username)?;
    let display_name = display_name.trim();
    if display_name.is_empty() { return Err("اكتب اسم مدير النظام".into()); }
    if password.chars().count() < 12 { return Err("كلمة المرور لازم تكون 12 حرفًا على الأقل".into()); }
    let path = database_path(&app)?; initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
    let count: i64 = tx.query_row("SELECT COUNT(*) FROM users", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    if count != 0 { return Err("تم إنشاء حساب بالفعل؛ استخدم شاشة تسجيل الدخول".into()); }
    let role_id: i64 = tx.query_row("SELECT id FROM roles WHERE code='admin'", [], |r| r.get(0)).map_err(|e| e.to_string())?;
    let password_hash = hash_password(&password)?;
    tx.execute("INSERT INTO users(username,display_name,password_hash,role_id) VALUES (?1,?2,?3,?4)", params![username,display_name,password_hash,role_id]).map_err(|e| format!("تعذر إنشاء الحساب؛ تأكد أن اسم المستخدم غير مستخدم: {e}"))?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO audit_log(action,entity_type,entity_id,details,actor_username) VALUES ('initial_admin_created','user',?1,?2,?3)", params![id.to_string(), format!("initial administrator username={username}"), username]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    let user = load_user_session(&conn, id)?;
    *state.session.lock().map_err(|_| "تعذر إنشاء جلسة المستخدم")? = Some(user.clone());
    Ok(user)
}

#[tauri::command]
fn login(app: tauri::AppHandle, state: State<'_, AppState>, username: String, password: String) -> Result<UserSession, String> {
    let normalized = username.trim().to_ascii_lowercase();
    let path = database_path(&app)?; initialize_database(&path)?;
    let conn = connect(&path)?;
    let maybe: Option<(i64,String)> = conn.query_row("SELECT id,password_hash FROM users WHERE username=?1 AND is_active=1", [&normalized], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e| e.to_string())?;
    let verified = match maybe.as_ref() {
        Some((_, hash)) => match PasswordHash::new(hash) {
            Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
            Err(_) => false,
        },
        None => false,
    };
    if !verified {
        conn.execute("INSERT INTO audit_log(action,entity_type,entity_id,details,actor_username) VALUES ('login_failed','session',NULL,'invalid username or password',?1)", [&normalized]).map_err(|e| e.to_string())?;
        return Err("اسم المستخدم أو كلمة المرور غير صحيحة، أو أن الحساب موقوف".into());
    }
    let id = maybe.ok_or("بيانات الدخول غير صحيحة")?.0;
    conn.execute("UPDATE users SET last_login_at=datetime('now') WHERE id=?1", [id]).map_err(|e| e.to_string())?;
    let user = load_user_session(&conn, id)?;
    write_audit(&conn, Some(&user), "login", "session", Some(&id.to_string()), "successful login")?;
    *state.session.lock().map_err(|_| "تعذر إنشاء جلسة المستخدم")? = Some(user.clone());
    Ok(user)
}

#[tauri::command]
fn logout(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let current = state.session.lock().map_err(|_| "تعذر قراءة جلسة المستخدم")?.take();
    if let Some(user) = current { audit_event(&app, Some(&user), "logout", "session", Some(&user.id.to_string()), "user logged out")?; }
    Ok(())
}

#[tauri::command]
fn list_users(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Vec<UserRow>, String> {
    let _actor = require_permission(state.inner(), "users.view")?;
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let mut stmt = conn.prepare("SELECT u.id,u.username,u.display_name,r.code,r.name,u.is_active,u.created_at,u.last_login_at,(SELECT COUNT(*) FROM user_permission_overrides po WHERE po.user_id=u.id),u.email,u.phone,u.address,u.avatar_data_url FROM users u JOIN roles r ON r.id=u.role_id ORDER BY u.id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(UserRow { id:r.get(0)?,username:r.get(1)?,display_name:r.get(2)?,role_code:r.get(3)?,role_name:r.get(4)?,is_active:r.get::<_,i64>(5)?!=0,created_at:r.get(6)?,last_login_at:r.get(7)?,permission_overrides:r.get(8)?,email:r.get(9)?,phone:r.get(10)?,address:r.get(11)?,avatar_data_url:r.get(12)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_user(app: tauri::AppHandle, state: State<'_, AppState>, username: String, display_name: String, role_code: String, password: String, email: Option<String>, phone: Option<String>, address: Option<String>, avatar_data_url: Option<String>) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.create")?;
    let username = validate_username(&username)?; let display_name = display_name.trim();
    if display_name.is_empty() { return Err("اكتب الاسم الظاهر للمستخدم".into()); }
    if display_name.chars().count() > 120 { return Err("الاسم الظاهر طويل جدًا (الحد 120 حرفًا)".into()); }
    if password.chars().count() < 12 { return Err("كلمة المرور لازم تكون 12 حرفًا على الأقل".into()); }
    let email = validate_profile_email(email)?;
    let phone = clean_optional_profile(phone, "رقم الهاتف", 40)?;
    let address = clean_optional_profile(address, "العنوان", 400)?;
    let avatar_data_url = validate_avatar_data_url(avatar_data_url)?;
    let allowed = ["admin","manager","warehouse","sales","viewer"];
    if !allowed.contains(&role_code.as_str()) { return Err("الدور المحدد غير صالح".into()); }
    if role_code == "admin" && actor.role_code != "admin" { return Err("إنشاء مدير نظام جديد متاح لمدير النظام فقط".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?; let tx=conn.transaction().map_err(|e|e.to_string())?;
    let role_id:i64=tx.query_row("SELECT id FROM roles WHERE code=?1",[&role_code],|r|r.get(0)).map_err(|e|e.to_string())?;
    let hashed=hash_password(&password)?;
    tx.execute("INSERT INTO users(username,display_name,password_hash,role_id,email,phone,address,avatar_data_url) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![username,display_name,hashed,role_id,email,phone,address,avatar_data_url]).map_err(|e|format!("تعذر إنشاء المستخدم؛ تأكد أن اسم المستخدم غير مستخدم: {e}"))?;
    let id=tx.last_insert_rowid(); write_audit(&tx,Some(&actor),"user_created","user",Some(&id.to_string()),&format!("username={username}; role={role_code}"))?; tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم إنشاء حساب المستخدم {username}"))
}

#[tauri::command]
fn update_user_profile(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, display_name: String, email: Option<String>, phone: Option<String>, address: Option<String>, avatar_data_url: Option<String>) -> Result<String, String> {
    let actor = {
        let guard = state.session.lock().map_err(|_| "تعذر الوصول إلى جلسة المستخدم")?;
        guard.as_ref().cloned().ok_or_else(|| "يلزم تسجيل الدخول أولًا".to_string())?
    };
    if user_id != actor.id && !actor.permissions.iter().any(|p| p == "users.profile.edit") {
        return Err("ليس لديك صلاحية تعديل بيانات مستخدم آخر".into());
    }
    let display_name = display_name.trim();
    if display_name.is_empty() { return Err("الاسم الظاهر مطلوب".into()); }
    if display_name.chars().count() > 120 { return Err("الاسم الظاهر طويل جدًا (الحد 120 حرفًا)".into()); }
    let email = validate_profile_email(email)?;
    let phone = clean_optional_profile(phone, "رقم الهاتف", 40)?;
    let address = clean_optional_profile(address, "العنوان", 400)?;
    let avatar_data_url = validate_avatar_data_url(avatar_data_url)?;
    let path = database_path(&app)?; initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
    let username: Option<String> = tx.query_row("SELECT username FROM users WHERE id=?1", [user_id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(username) = username else { return Err("المستخدم غير موجود".into()); };
    tx.execute("UPDATE users SET display_name=?1,email=?2,phone=?3,address=?4,avatar_data_url=?5,updated_at=datetime('now') WHERE id=?6", params![display_name,email,phone,address,avatar_data_url,user_id]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "user_profile_updated", "user", Some(&user_id.to_string()), &format!("username={username}; profile updated"))?;
    tx.commit().map_err(|e| e.to_string())?;
    if user_id == actor.id {
        if let Ok(mut guard) = state.session.lock() {
            if let Some(ref mut session) = *guard {
                session.display_name = display_name.to_string();
                session.avatar_data_url = avatar_data_url.clone();
                session.email = email.clone();
                session.phone = phone.clone();
                session.address = address.clone();
            }
        }
    }
    Ok(format!("تم حفظ بيانات المستخدم {username}"))
}

#[tauri::command]
fn set_user_active(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, active: bool) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.status.manage")?;
    if user_id == actor.id && !active { return Err("لا يمكنك إيقاف حسابك أثناء استخدامه".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?; let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let target: Option<(String,String,i64)> = tx.query_row("SELECT u.username,r.code,u.is_active FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1",[user_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,role,was_active))=target else{return Err("المستخدم غير موجود".into());};
    if role=="admin" && actor.role_code!="admin" { return Err("إدارة حسابات مدير النظام متاحة لمدير النظام فقط".into()); }
    if role=="admin" && was_active==1 && !active {
        let admins:i64=tx.query_row("SELECT COUNT(*) FROM users u JOIN roles r ON r.id=u.role_id WHERE r.code='admin' AND u.is_active=1",[],|r|r.get(0)).map_err(|e|e.to_string())?;
        if admins<=1{return Err("لا يمكن إيقاف آخر مدير نظام نشط".into());}
    }
    tx.execute("UPDATE users SET is_active=?1,updated_at=datetime('now') WHERE id=?2",params![if active{1}else{0},user_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if active{"user_activated"}else{"user_deactivated"},"user",Some(&user_id.to_string()),&format!("username={username}; active={active}"))?;
    tx.commit().map_err(|e|e.to_string())?; Ok(if active {format!("تم تفعيل المستخدم {username}")} else {format!("تم إيقاف المستخدم {username}")})
}

#[tauri::command]
fn reset_user_password(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, new_password: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.password.reset")?;
    if new_password.chars().count()<12{return Err("كلمة المرور الجديدة لازم تكون 12 حرفًا على الأقل".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction().map_err(|e|e.to_string())?;
    let target:Option<(String,String)>=tx.query_row("SELECT u.username,r.code FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1",[user_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,target_role))=target else{return Err("المستخدم غير موجود".into());};
    if target_role=="admin" && actor.role_code!="admin" { return Err("إعادة تعيين كلمة مرور مدير النظام متاحة لمدير النظام فقط".into()); }
    let hashed=hash_password(&new_password)?;tx.execute("UPDATE users SET password_hash=?1,updated_at=datetime('now') WHERE id=?2",params![hashed,user_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),"password_reset","user",Some(&user_id.to_string()),&format!("password reset for username={username}"))?;tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم تغيير كلمة مرور {username}"))
}

#[tauri::command]
fn change_own_password(app: tauri::AppHandle, state: State<'_, AppState>, current_password: String, new_password: String) -> Result<String, String> {
    let actor = state.session.lock().map_err(|_| "تعذر قراءة جلسة المستخدم")?.clone().ok_or("يلزم تسجيل الدخول أولًا")?;
    if new_password.chars().count() < 12 { return Err("كلمة المرور الجديدة لازم تكون 12 حرفًا على الأقل".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let conn=connect(&path)?;
    let current_hash:String=conn.query_row("SELECT password_hash FROM users WHERE id=?1 AND is_active=1",[actor.id],|r|r.get(0)).map_err(|_|"الحساب غير موجود أو موقوف".to_string())?;
    let parsed=PasswordHash::new(&current_hash).map_err(|_|"بيانات كلمة المرور المحفوظة غير صالحة".to_string())?;
    if Argon2::default().verify_password(current_password.as_bytes(),&parsed).is_err(){return Err("كلمة المرور الحالية غير صحيحة".into());}
    if current_password==new_password {return Err("اختر كلمة مرور جديدة مختلفة عن الحالية".into());}
    let hashed=hash_password(&new_password)?;
    conn.execute("UPDATE users SET password_hash=?1,updated_at=datetime('now') WHERE id=?2",params![hashed,actor.id]).map_err(|e|e.to_string())?;
    write_audit(&conn,Some(&actor),"password_changed","user",Some(&actor.id.to_string()),"user changed own password")?;
    Ok("تم تغيير كلمة المرور بنجاح".into())
}

#[tauri::command]
fn set_user_role(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, role_code: String) -> Result<String, String> {
    let actor=require_permission(state.inner(),"users.role.manage")?;
    if user_id==actor.id{return Err("لأمان النظام، لا يمكن تغيير دور الحساب المستخدم حاليًا. استخدم حساب مدير آخر عند الحاجة.".into());}
    let allowed=["admin","manager","warehouse","sales","viewer"];
    if !allowed.contains(&role_code.as_str()){return Err("الدور المحدد غير صالح".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let old:Option<(String,String,i64)>=tx.query_row("SELECT u.username,r.code,u.is_active FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1",[user_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,old_role,active))=old else{return Err("المستخدم غير موجود".into());};
    if old_role=="admin" && actor.role_code!="admin" {return Err("إدارة دور مدير النظام متاحة لمدير النظام فقط".into());}
    if old_role=="admin" && role_code!="admin" && active==1 {
      let admins:i64=tx.query_row("SELECT COUNT(*) FROM users u JOIN roles r ON r.id=u.role_id WHERE r.code='admin' AND u.is_active=1",[],|r|r.get(0)).map_err(|e|e.to_string())?;
      if admins<=1{return Err("لا يمكن تغيير دور آخر مدير نظام نشط".into());}
    }
    if role_code=="admin" && actor.role_code!="admin" {return Err("تعيين دور مدير النظام متاح لمدير النظام فقط".into());}
    let role_id:i64=tx.query_row("SELECT id FROM roles WHERE code=?1",[&role_code],|r|r.get(0)).map_err(|e|e.to_string())?;
    tx.execute("UPDATE users SET role_id=?1,updated_at=datetime('now') WHERE id=?2",params![role_id,user_id]).map_err(|e|e.to_string())?;
    if role_code=="admin" || old_role=="admin" {
        tx.execute("DELETE FROM user_permission_overrides WHERE user_id=?1",[user_id]).map_err(|e|e.to_string())?;
    }
    write_audit(&tx,Some(&actor),"user_role_changed","user",Some(&user_id.to_string()),&format!("username={username}; old_role={old_role}; new_role={role_code}"))?;
    tx.commit().map_err(|e|e.to_string())?;Ok(format!("تم تحديث دور المستخدم {username}"))
}

#[tauri::command]
fn list_user_permissions(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64) -> Result<Vec<PermissionDetail>, String> {
    let actor = require_permission(state.inner(), "users.permissions.manage")?;
    if actor.role_code != "admin" { return Err("محرر الصلاحيات متاح لمدير النظام فقط".into()); }
    if user_id == actor.id { return Err("لا يمكن تخصيص صلاحيات حسابك الحالي؛ لتجنب فقدان الوصول للنظام".into()); }
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let role: Option<String> = conn.query_row("SELECT r.code FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1", [user_id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(role_code) = role else { return Err("المستخدم غير موجود".into()); };
    if role_code == "admin" { return Err("لا يمكن تخصيص صلاحيات حساب مدير النظام؛ فهو يحتفظ بكل الصلاحيات".into()); }
    let mut stmt = conn.prepare(
        "SELECT p.code,p.name,p.description,
          COALESCE(up.allowed, CASE WHEN rp.permission_code IS NULL THEN 0 ELSE 1 END) AS effective_allowed,
          CASE WHEN rp.permission_code IS NULL THEN 0 ELSE 1 END AS role_allowed,
          CASE WHEN up.user_id IS NULL THEN 0 ELSE 1 END AS has_override
         FROM permissions p
         JOIN users u ON u.id=?1
         LEFT JOIN role_permissions rp ON rp.role_id=u.role_id AND rp.permission_code=p.code
         LEFT JOIN user_permission_overrides up ON up.user_id=u.id AND up.permission_code=p.code
         WHERE p.code NOT IN ('products.manage','stock.manage','backup.manage','users.manage')
         ORDER BY CASE
           WHEN p.code LIKE 'products.%' THEN 1 WHEN p.code LIKE 'stock.%' THEN 2
           WHEN p.code LIKE 'backup.%' THEN 3 WHEN p.code LIKE 'users.%' THEN 4
           WHEN p.code LIKE 'audit.%' THEN 5 ELSE 6 END, p.code"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([user_id], |r| Ok(PermissionDetail { code:r.get(0)?,name:r.get(1)?,description:r.get(2)?,allowed:r.get::<_,i64>(3)?!=0,role_allowed:r.get::<_,i64>(4)?!=0,is_override:r.get::<_,i64>(5)?!=0 })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_user_permissions(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, allowed_codes: Vec<String>) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.permissions.manage")?;
    if actor.role_code != "admin" { return Err("تخصيص الصلاحيات متاح لمدير النظام فقط".into()); }
    if user_id == actor.id { return Err("لا يمكن تخصيص صلاحيات حسابك الحالي؛ لتجنب فقدان الوصول للنظام".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let target: Option<(String,String)> = tx.query_row("SELECT u.username,r.code FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1", [user_id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,role_code))=target else { return Err("المستخدم غير موجود".into()); };
    if role_code=="admin" { return Err("لا يمكن تخصيص صلاحيات حساب مدير النظام؛ فهو يحتفظ بكل الصلاحيات".into()); }
    let unique: std::collections::HashSet<String> = allowed_codes.iter().cloned().collect();
    if unique.len()!=allowed_codes.len() { return Err("قائمة الصلاحيات تحتوي على تكرار غير متوقع".into()); }
    let mut stmt=tx.prepare("SELECT code FROM permissions WHERE code NOT IN ('products.manage','stock.manage','backup.manage','users.manage')").map_err(|e|e.to_string())?;
    let all_rows=stmt.query_map([],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?;
    let mut all_codes=Vec::new(); for row in all_rows { all_codes.push(row.map_err(|e|e.to_string())?); } drop(stmt);
    if let Some(unknown)=allowed_codes.iter().find(|code|!all_codes.contains(code)) { return Err(format!("الصلاحية غير معروفة: {unknown}")); }
    if unique.contains("users.permissions.manage") { return Err("صلاحية تخصيص الصلاحيات لا يمكن منحها لمستخدم آخر؛ وهي محجوزة لمدير النظام".into()); }
    let dependencies = [
        ("products.create","products.view"), ("products.edit","products.view"),
        ("products.deactivate","products.view"), ("products.reactivate","products.view"),
        ("products.barcode.manage","products.view"), ("products.purchase_price.edit","products.view"),
        ("products.sale_price.edit","products.view"), ("stock.movement.record","stock.view"),
        ("backup.points.delete","backup.view_points"), ("users.create","users.view"), ("users.profile.edit","users.view"),
        ("users.status.manage","users.view"), ("users.role.manage","users.view"), ("users.password.reset","users.view"),
        ("customers.create","customers.view"), ("customers.edit","customers.view"), ("customers.status.manage","customers.view"),
        ("suppliers.create","suppliers.view"), ("suppliers.edit","suppliers.view"), ("suppliers.status.manage","suppliers.view"),
        ("purchases.create","purchases.view"), ("purchases.post","purchases.view"), ("purchases.cancel","purchases.view"),
        ("sales.create","sales.view"), ("sales.post","sales.view"), ("sales.cancel","sales.view"),
        ("purchases.create","products.view"), ("purchases.create","suppliers.view"),
        ("purchases.post","purchases.create"), ("sales.create","products.view"),
        ("sales.create","customers.view"), ("sales.post","sales.create"),
        ("purchases.edit_draft","purchases.view"), ("purchases.edit_draft","purchases.create"),
        ("sales.edit_draft","sales.view"), ("sales.edit_draft","sales.create"),
        ("sales.price.override","sales.create")
    ];
    for (permission, required) in dependencies {
        if unique.contains(permission) && !unique.contains(required) {
            return Err(format!("صلاحية {permission} تتطلب أيضًا صلاحية {required}"));
        }
    }
    if (unique.contains("products.purchase_price.edit") || unique.contains("products.sale_price.edit"))
        && !(unique.contains("products.create") || unique.contains("products.edit")) {
        return Err("صلاحيات تعديل الأسعار تتطلب السماح بإضافة الأصناف أو تعديل بياناتها".into());
    }
    let mut changed=Vec::new();
    for code in &all_codes {
        let desired=unique.contains(code);
        let role_allowed: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id=(SELECT role_id FROM users WHERE id=?1) AND permission_code=?2)",params![user_id,code],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if desired==role_allowed {
            let removed=tx.execute("DELETE FROM user_permission_overrides WHERE user_id=?1 AND permission_code=?2",params![user_id,code]).map_err(|e|e.to_string())?;
            if removed>0 { changed.push(format!("{code}=role_default")); }
        } else {
            let wanted=if desired {1_i64} else {0_i64};
            let previous:Option<i64>=tx.query_row("SELECT allowed FROM user_permission_overrides WHERE user_id=?1 AND permission_code=?2",params![user_id,code],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            if previous!=Some(wanted) {
                tx.execute("INSERT INTO user_permission_overrides(user_id,permission_code,allowed) VALUES (?1,?2,?3) ON CONFLICT(user_id,permission_code) DO UPDATE SET allowed=excluded.allowed",params![user_id,code,wanted]).map_err(|e|e.to_string())?;
                changed.push(format!("{code}={}",if desired{"allowed"}else{"denied"}));
            }
        }
    }
    write_audit(&tx,Some(&actor),"user_permissions_changed","user",Some(&user_id.to_string()),&format!("username={username}; changed={}",if changed.is_empty(){"none".into()}else{changed.join(",")}))?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم حفظ الصلاحيات التفصيلية للمستخدم {username}. تسري التغييرات عند تسجيل دخوله التالي."))
}
