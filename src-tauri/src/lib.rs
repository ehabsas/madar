use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::Serialize;
use std::{fs, path::PathBuf, sync::Mutex, time::{SystemTime, UNIX_EPOCH}};
use tauri::{Manager, State};

fn database_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    dir.push("Madar.db");
    Ok(dir)
}

fn connect(path: &PathBuf) -> Result<Connection, String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE)
        .map_err(|e| e.to_string())?;
    conn.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
    conn.pragma_update(None, "journal_mode", "WAL").map_err(|e| e.to_string())?;
    Ok(conn)
}

fn initialize_database(path: &PathBuf) -> Result<(), String> {
    let conn = connect(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY, value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            category_id INTEGER, unit TEXT NOT NULL DEFAULT 'قطعة',
            purchase_price_minor INTEGER NOT NULL DEFAULT 0 CHECK (purchase_price_minor >= 0),
            sale_price_minor INTEGER NOT NULL DEFAULT 0 CHECK (sale_price_minor >= 0),
            minimum_quantity_milli INTEGER NOT NULL DEFAULT 0 CHECK (minimum_quantity_milli >= 0),
            notes TEXT, is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS product_barcodes (
            id INTEGER PRIMARY KEY, product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
            unit TEXT NOT NULL DEFAULT 'قطعة', barcode TEXT NOT NULL UNIQUE,
            barcode_type TEXT NOT NULL CHECK (barcode_type IN ('legacy','internal','alternate','unit')),
            is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK (length(trim(barcode)) > 0)
        );
        CREATE INDEX IF NOT EXISTS idx_product_barcodes_product ON product_barcodes(product_id);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_product_one_primary_barcode ON product_barcodes(product_id) WHERE is_primary = 1;
        CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);
        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY, action TEXT NOT NULL, entity_type TEXT NOT NULL,
            entity_id TEXT, details TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')),
            actor_user_id INTEGER, actor_username TEXT
        );
        CREATE TABLE IF NOT EXISTS roles (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            is_system INTEGER NOT NULL DEFAULT 1 CHECK (is_system IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS permissions (
            code TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT ''
        );
        CREATE TABLE IF NOT EXISTS role_permissions (
            role_id INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
            permission_code TEXT NOT NULL REFERENCES permissions(code) ON DELETE CASCADE,
            PRIMARY KEY(role_id, permission_code)
        );
        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE,
            display_name TEXT NOT NULL, password_hash TEXT NOT NULL,
            role_id INTEGER NOT NULL REFERENCES roles(id),
            is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            last_login_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_users_role ON users(role_id);
        CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at DESC);
        CREATE TABLE IF NOT EXISTS warehouses (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS stock_movements (
            id INTEGER PRIMARY KEY,
            product_id INTEGER NOT NULL REFERENCES products(id),
            warehouse_id INTEGER NOT NULL REFERENCES warehouses(id),
            movement_type TEXT NOT NULL CHECK (movement_type IN ('opening','purchase','sale','purchase_return','sale_return','adjustment','transfer_in','transfer_out')),
            quantity_milli INTEGER NOT NULL CHECK (quantity_milli <> 0),
            reference_type TEXT,
            reference_id INTEGER,
            notes TEXT,
            unit TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_stock_movements_product_warehouse ON stock_movements(product_id, warehouse_id, created_at);
        INSERT OR IGNORE INTO warehouses(code,name) VALUES ('MAIN','المخزن الرئيسي');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('app_name','مدار');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('database_schema_version','4');
        UPDATE app_settings SET value='4', updated_at=datetime('now') WHERE key='database_schema_version' AND value IN ('2','3');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('barcode_prefix','MAD');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('barcode_auto_generate','true');"
    ).map_err(|e| e.to_string())?;

    // Additive migration for installations that created stock_movements from the early schema.
    let has_movement_unit = {
        let mut stmt = conn.prepare("PRAGMA table_info(stock_movements)").map_err(|e| e.to_string())?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1)).map_err(|e| e.to_string())?;
        let mut found = false;
        for column in columns {
            if column.map_err(|e| e.to_string())? == "unit" { found = true; break; }
        }
        found
    };
    if !has_movement_unit {
        conn.execute("ALTER TABLE stock_movements ADD COLUMN unit TEXT", []).map_err(|e| e.to_string())?;
    }
    conn.execute(
        "UPDATE stock_movements SET unit=(SELECT p.unit FROM products p WHERE p.id=stock_movements.product_id) WHERE unit IS NULL OR trim(unit)=''",
        [],
    ).map_err(|e| e.to_string())?;

    // Additive migration for databases created before user attribution was introduced.
    let has_actor_id = table_has_column(&conn, "audit_log", "actor_user_id")?;
    if !has_actor_id {
        conn.execute("ALTER TABLE audit_log ADD COLUMN actor_user_id INTEGER", []).map_err(|e| e.to_string())?;
    }
    let has_actor_name = table_has_column(&conn, "audit_log", "actor_username")?;
    if !has_actor_name {
        conn.execute("ALTER TABLE audit_log ADD COLUMN actor_username TEXT", []).map_err(|e| e.to_string())?;
    }
    seed_access_control(&conn)?;
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1)).map_err(|e| e.to_string())?;
    for row in rows {
        if row.map_err(|e| e.to_string())? == column { return Ok(true); }
    }
    Ok(false)
}

fn seed_access_control(conn: &Connection) -> Result<(), String> {
    let permissions = [
        ("products.view", "عرض المنتجات", "عرض وبحث الأصناف والباركودات"),
        ("products.manage", "إدارة المنتجات", "إضافة وتعديل وإيقاف وإعادة تفعيل الأصناف"),
        ("stock.view", "عرض المخزون", "عرض الأرصدة وسجل الحركات"),
        ("stock.manage", "إدارة المخزون", "تسجيل حركات الوارد والمنصرف"),
        ("backup.manage", "النسخ والاستعادة", "إنشاء النسخ الاحتياطية واستعادة وحذف نقاط الاستعادة"),
        ("users.manage", "إدارة المستخدمين", "إنشاء وتعطيل المستخدمين وتحديد أدوارهم"),
        ("audit.view", "عرض سجل النشاط", "عرض عمليات المستخدمين وسجل التدقيق"),
    ];
    for (code, name, description) in permissions {
        conn.execute("INSERT OR IGNORE INTO permissions(code,name,description) VALUES (?1,?2,?3)", params![code,name,description]).map_err(|e| e.to_string())?;
    }
    for (code, name) in [("admin","مدير النظام"),("manager","مدير"),("warehouse","مسؤول مخزن"),("sales","مبيعات"),("viewer","عرض فقط")] {
        conn.execute("INSERT OR IGNORE INTO roles(code,name,is_system) VALUES (?1,?2,1)", params![code,name]).map_err(|e| e.to_string())?;
    }
    let all = ["products.view","products.manage","stock.view","stock.manage","backup.manage","users.manage","audit.view"];
    let manager = ["products.view","products.manage","stock.view","stock.manage","backup.manage","audit.view"];
    let warehouse = ["products.view","stock.view","stock.manage"];
    let sales = ["products.view","stock.view"];
    let viewer = ["products.view","stock.view"];
    let matrices: [(&str, &[&str]); 5] = [("admin", &all), ("manager", &manager), ("warehouse", &warehouse), ("sales", &sales), ("viewer", &viewer)];
    for (role, items) in matrices {
        let role_id: i64 = conn.query_row("SELECT id FROM roles WHERE code=?1", [role], |r| r.get(0)).map_err(|e| e.to_string())?;
        for item in items {
            conn.execute("INSERT OR IGNORE INTO role_permissions(role_id,permission_code) VALUES (?1,?2)", params![role_id,item]).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UserSession {
    id: i64,
    username: String,
    display_name: String,
    role_code: String,
    role_name: String,
    permissions: Vec<String>,
}

#[derive(Default)]
struct AppState { session: Mutex<Option<UserSession>> }

fn require_permission(state: &AppState, permission: &str) -> Result<UserSession, String> {
    let guard = state.session.lock().map_err(|_| "تعذر الوصول إلى جلسة المستخدم")?;
    let user = guard.as_ref().ok_or_else(|| "يلزم تسجيل الدخول أولًا".to_string())?;
    if !user.permissions.iter().any(|p| p == permission) {
        return Err(format!("ليس لديك صلاحية لتنفيذ هذه العملية ({permission})"));
    }
    Ok(user.clone())
}

fn write_audit(conn: &Connection, actor: Option<&UserSession>, action: &str, entity_type: &str, entity_id: Option<&str>, details: &str) -> Result<(), String> {
    conn.execute(
        "INSERT INTO audit_log(action,entity_type,entity_id,details,actor_user_id,actor_username) VALUES (?1,?2,?3,?4,?5,?6)",
        params![action,entity_type,entity_id,details,actor.map(|u| u.id),actor.map(|u| u.username.as_str())],
    ).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_user_session(conn: &Connection, user_id: i64) -> Result<UserSession, String> {
    let (username, display_name, role_code, role_name): (String,String,String,String) = conn.query_row(
        "SELECT u.username,u.display_name,r.code,r.name FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1 AND u.is_active=1",
        [user_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)),
    ).map_err(|_| "المستخدم غير موجود أو موقوف".to_string())?;
    let mut stmt = conn.prepare("SELECT rp.permission_code FROM role_permissions rp JOIN users u ON u.role_id=rp.role_id WHERE u.id=?1 ORDER BY rp.permission_code").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([user_id], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
    let mut permissions = Vec::new();
    for row in rows { permissions.push(row.map_err(|e| e.to_string())?); }
    Ok(UserSession { id:user_id, username, display_name, role_code, role_name, permissions })
}

fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| format!("تعذر تأمين كلمة المرور: {e}"))
}

fn validate_username(username: &str) -> Result<String, String> {
    let normalized = username.trim().to_ascii_lowercase();
    if normalized.len() < 3 || normalized.len() > 40 || !normalized.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-') {
        return Err("اسم المستخدم يجب أن يكون من 3 إلى 40 حرفًا إنجليزيًا أو رقمًا، ويمكن استخدام النقطة والشرطة والشرطة السفلية".into());
    }
    Ok(normalized)
}

fn audit_event(app: &tauri::AppHandle, actor: Option<&UserSession>, action: &str, entity_type: &str, entity_id: Option<&str>, details: &str) -> Result<(), String> {
    let path = database_path(app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    write_audit(&conn, actor, action, entity_type, entity_id, details)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserRow { id: i64, username: String, display_name: String, role_code: String, role_name: String, is_active: bool, created_at: String, last_login_at: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuditRow { id: i64, action: String, entity_type: String, entity_id: Option<String>, details: Option<String>, actor_username: Option<String>, created_at: String }

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
    let mut conn = connect(&path)?;
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
    let _actor = require_permission(state.inner(), "users.manage")?;
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let mut stmt = conn.prepare("SELECT u.id,u.username,u.display_name,r.code,r.name,u.is_active,u.created_at,u.last_login_at FROM users u JOIN roles r ON r.id=u.role_id ORDER BY u.id").map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], |r| Ok(UserRow { id:r.get(0)?,username:r.get(1)?,display_name:r.get(2)?,role_code:r.get(3)?,role_name:r.get(4)?,is_active:r.get::<_,i64>(5)?!=0,created_at:r.get(6)?,last_login_at:r.get(7)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn create_user(app: tauri::AppHandle, state: State<'_, AppState>, username: String, display_name: String, role_code: String, password: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.manage")?;
    let username = validate_username(&username)?; let display_name = display_name.trim();
    if display_name.is_empty() { return Err("اكتب الاسم الظاهر للمستخدم".into()); }
    if password.chars().count() < 12 { return Err("كلمة المرور لازم تكون 12 حرفًا على الأقل".into()); }
    let allowed = ["admin","manager","warehouse","sales","viewer"];
    if !allowed.contains(&role_code.as_str()) { return Err("الدور المحدد غير صالح".into()); }
    if role_code == "admin" && actor.role_code != "admin" { return Err("إنشاء مدير نظام جديد متاح لمدير النظام فقط".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?; let tx=conn.transaction().map_err(|e|e.to_string())?;
    let role_id:i64=tx.query_row("SELECT id FROM roles WHERE code=?1",[&role_code],|r|r.get(0)).map_err(|e|e.to_string())?;
    let hashed=hash_password(&password)?;
    tx.execute("INSERT INTO users(username,display_name,password_hash,role_id) VALUES (?1,?2,?3,?4)",params![username,display_name,hashed,role_id]).map_err(|e|format!("تعذر إنشاء المستخدم؛ تأكد أن اسم المستخدم غير مستخدم: {e}"))?;
    let id=tx.last_insert_rowid(); write_audit(&tx,Some(&actor),"user_created","user",Some(&id.to_string()),&format!("username={username}; role={role_code}"))?; tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم إنشاء حساب المستخدم {username}"))
}

#[tauri::command]
fn set_user_active(app: tauri::AppHandle, state: State<'_, AppState>, user_id: i64, active: bool) -> Result<String, String> {
    let actor = require_permission(state.inner(), "users.manage")?;
    if user_id == actor.id && !active { return Err("لا يمكنك إيقاف حسابك أثناء استخدامه".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?; let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let target: Option<(String,String,i64)> = tx.query_row("SELECT u.username,r.code,u.is_active FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1",[user_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,role,was_active))=target else{return Err("المستخدم غير موجود".into());};
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
    let actor = require_permission(state.inner(), "users.manage")?;
    if new_password.chars().count()<12{return Err("كلمة المرور الجديدة لازم تكون 12 حرفًا على الأقل".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction().map_err(|e|e.to_string())?;
    let username:Option<String>=tx.query_row("SELECT username FROM users WHERE id=?1",[user_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let Some(username)=username else{return Err("المستخدم غير موجود".into());};
    let hashed=hash_password(&new_password)?;tx.execute("UPDATE users SET password_hash=?1,updated_at=datetime('now') WHERE id=?2",params![hashed,user_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),"password_reset","user",Some(&user_id.to_string()),&format!("password reset for username={username}"))?;tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم تغيير كلمة مرور {username}"))
}

#[tauri::command]
fn change_own_password(app: tauri::AppHandle, state: State<'_, AppState>, current_password: String, new_password: String) -> Result<String, String> {
    let actor = state.session.lock().map_err(|_| "تعذر قراءة جلسة المستخدم")?.clone().ok_or("يلزم تسجيل الدخول أولًا")?;
    if new_password.chars().count() < 12 { return Err("كلمة المرور الجديدة لازم تكون 12 حرفًا على الأقل".into()); }
    let path=database_path(&app)?; initialize_database(&path)?; let mut conn=connect(&path)?;
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
    let actor=require_permission(state.inner(),"users.manage")?;
    if user_id==actor.id{return Err("لأمان النظام، لا يمكن تغيير دور الحساب المستخدم حاليًا. استخدم حساب مدير آخر عند الحاجة.".into());}
    let allowed=["admin","manager","warehouse","sales","viewer"];
    if !allowed.contains(&role_code.as_str()){return Err("الدور المحدد غير صالح".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let old:Option<(String,String,i64)>=tx.query_row("SELECT u.username,r.code,u.is_active FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1",[user_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((username,old_role,active))=old else{return Err("المستخدم غير موجود".into());};
    if old_role=="admin" && role_code!="admin" && active==1 {
      let admins:i64=tx.query_row("SELECT COUNT(*) FROM users u JOIN roles r ON r.id=u.role_id WHERE r.code='admin' AND u.is_active=1",[],|r|r.get(0)).map_err(|e|e.to_string())?;
      if admins<=1{return Err("لا يمكن تغيير دور آخر مدير نظام نشط".into());}
    }
    if role_code=="admin" && actor.role_code!="admin" {return Err("تعيين دور مدير النظام متاح لمدير النظام فقط".into());}
    let role_id:i64=tx.query_row("SELECT id FROM roles WHERE code=?1",[&role_code],|r|r.get(0)).map_err(|e|e.to_string())?;
    tx.execute("UPDATE users SET role_id=?1,updated_at=datetime('now') WHERE id=?2",params![role_id,user_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),"user_role_changed","user",Some(&user_id.to_string()),&format!("username={username}; old_role={old_role}; new_role={role_code}"))?;
    tx.commit().map_err(|e|e.to_string())?;Ok(format!("تم تحديث دور المستخدم {username}"))
}

#[tauri::command]
fn list_audit_log(app: tauri::AppHandle, state: State<'_, AppState>, query: Option<String>) -> Result<Vec<AuditRow>, String> {
    let _actor=require_permission(state.inner(),"audit.view")?;
    let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;let q=query.unwrap_or_default();let like=format!("%{}%",q.trim());
    let mut stmt=conn.prepare("SELECT id,action,entity_type,entity_id,details,actor_username,created_at FROM audit_log WHERE (?1='' OR action LIKE ?2 OR entity_type LIKE ?2 OR COALESCE(entity_id,'') LIKE ?2 OR COALESCE(details,'') LIKE ?2 OR COALESCE(actor_username,'') LIKE ?2) ORDER BY id DESC LIMIT 500").map_err(|e|e.to_string())?;
    let rows=stmt.query_map(params![q.trim(),like],|r|Ok(AuditRow{id:r.get(0)?,action:r.get(1)?,entity_type:r.get(2)?,entity_id:r.get(3)?,details:r.get(4)?,actor_username:r.get(5)?,created_at:r.get(6)?})).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[derive(Serialize)]
struct ProductRow {
    id: i64, code: String, name: String, unit: String, barcode: String,
    purchase_price_minor: i64, sale_price_minor: i64,
}

fn next_internal_barcode(conn: &Connection) -> Result<String, String> {
    // Internal Code 128-friendly barcode. It is unique within Madar, not a GS1 retail GTIN.
    let mut n: i64 = conn.query_row(
        "SELECT COALESCE(MAX(id), 0) + 1 FROM product_barcodes WHERE barcode_type='internal'",
        [], |r| r.get(0)).map_err(|e| e.to_string())?;
    loop {
        let candidate = format!("MAD{:010}", n);
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM product_barcodes WHERE barcode=?1)",
            [&candidate], |r| r.get(0)).map_err(|e| e.to_string())?;
        if !exists { return Ok(candidate); }
        n += 1;
    }
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({"name":"مدار", "englishName":"Madar", "version":env!("CARGO_PKG_VERSION"), "database":"SQLite", "mode":"local", "barcodePolicy":"one primary barcode per product; preserve supplied barcode or generate internal barcode"})
}

#[tauri::command]
fn create_product(app: tauri::AppHandle, state: State<'_, AppState>, code: String, name: String, unit: Option<String>, barcode: Option<String>, purchase_price_minor: i64, sale_price_minor: i64) -> Result<ProductRow, String> {
    let actor = require_permission(state.inner(), "products.manage")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let product_code = code.trim();
    let product_name = name.trim();
    if product_code.is_empty() || product_name.is_empty() { return Err("كود الصنف واسم الصنف مطلوبان".into()); }
    if purchase_price_minor < 0 || sale_price_minor < 0 { return Err("السعر لا يمكن أن يكون سالبًا".into()); }
    let unit_value = unit.unwrap_or_else(|| "قطعة".to_string());
    let supplied = barcode.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let (barcode_value, barcode_type) = match supplied {
        Some(value) => (value, "legacy"),
        None => {
            let generated = next_internal_barcode(&tx)?;
            (generated, "internal")
        }
    };
    let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM product_barcodes WHERE barcode=?1)", [&barcode_value], |r| r.get(0)).map_err(|e| e.to_string())?;
    if duplicate { return Err("الباركود مستخدم بالفعل لصنف آخر. أدخل باركود مختلفًا.".into()); }
    tx.execute("INSERT INTO products(code,name,unit,purchase_price_minor,sale_price_minor) VALUES (?1,?2,?3,?4,?5)", params![product_code, product_name, unit_value, purchase_price_minor, sale_price_minor]).map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,?4,1)", params![id, unit_value, barcode_value, barcode_type]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "create", "product", Some(&id.to_string()), &format!("code={}; barcode={}", product_code, barcode_value))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ProductRow { id, code: product_code.to_string(), name: product_name.to_string(), unit: unit_value, barcode: barcode_value, purchase_price_minor, sale_price_minor })
}

#[tauri::command]
fn find_product_by_barcode(app: tauri::AppHandle, state: State<'_, AppState>, barcode: String) -> Result<Option<ProductRow>, String> {
    let actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    conn.query_row(
        "SELECT p.id,p.code,p.name,p.unit,b.barcode,p.purchase_price_minor,p.sale_price_minor FROM product_barcodes b JOIN products p ON p.id=b.product_id WHERE b.barcode=?1 AND p.is_active=1 LIMIT 1",
        [barcode.trim()], |r| Ok(ProductRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, unit:r.get(3)?, barcode:r.get(4)?, purchase_price_minor:r.get(5)?, sale_price_minor:r.get(6)? })
    ).optional().map_err(|e| e.to_string())
}

fn search_products_db(conn: &Connection, query: &str, is_active: bool) -> Result<Vec<ProductRow>, String> {
    let q = query.trim();
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        "SELECT p.id,p.code,p.name,p.unit,COALESCE((SELECT b.barcode FROM product_barcodes b WHERE b.product_id=p.id AND b.is_primary=1 LIMIT 1),''),p.purchase_price_minor,p.sale_price_minor
         FROM products p WHERE p.is_active=?1 AND (?2='' OR p.name LIKE ?3 OR p.code LIKE ?3 OR EXISTS(SELECT 1 FROM product_barcodes bx WHERE bx.product_id=p.id AND bx.barcode LIKE ?3))
         ORDER BY p.name COLLATE NOCASE LIMIT 500"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![if is_active { 1 } else { 0 }, q, like], |r| Ok(ProductRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, unit:r.get(3)?, barcode:r.get(4)?, purchase_price_minor:r.get(5)?, sale_price_minor:r.get(6)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_products(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<ProductRow>, String> {
    let actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, "", true)
}

#[tauri::command]
fn search_products(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<ProductRow>, String> {
    let actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, &query, true)
}

#[tauri::command]
fn list_inactive_products(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<ProductRow>, String> {
    let actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, "", false)
}

#[tauri::command]
fn search_inactive_products(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<ProductRow>, String> {
    let actor = require_permission(state.inner(), "products.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    search_products_db(&conn, &query, false)
}

#[tauri::command]
fn reactivate_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.manage")?;
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
fn update_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64, code: String, name: String, unit: String, purchase_price_minor: i64, sale_price_minor: i64) -> Result<ProductRow, String> {
    let actor = require_permission(state.inner(), "products.manage")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let code = code.trim();
    let name = name.trim();
    let unit = unit.trim();
    if code.is_empty() || name.is_empty() || unit.is_empty() { return Err("الكود والاسم والوحدة حقول مطلوبة".into()); }
    if purchase_price_minor < 0 || sale_price_minor < 0 { return Err("السعر لا يمكن أن يكون سالبًا".into()); }
    let current_unit: Option<String> = tx.query_row("SELECT unit FROM products WHERE id=?1 AND is_active=1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(current_unit) = current_unit else { return Err("الصنف غير موجود أو موقوف".into()); };
    if current_unit != unit {
        let on_hand: i64 = tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1", [id], |r| r.get(0)).map_err(|e| e.to_string())?;
        if on_hand != 0 {
            return Err(format!("لا يمكن تغيير وحدة الصنف إلى «{unit}» لأن له رصيدًا حاليًا. صفّر الرصيد أو عالج تحويل الوحدة أولًا حتى لا تتغير دلالة الكميات المسجلة."));
        }
    }
    tx.execute("UPDATE products SET code=?1,name=?2,unit=?3,purchase_price_minor=?4,sale_price_minor=?5,updated_at=datetime('now') WHERE id=?6", params![code,name,unit,purchase_price_minor,sale_price_minor,id]).map_err(|e| format!("تعذر تعديل الصنف (تأكد أن الكود غير مستخدم): {e}"))?;
    tx.execute("UPDATE product_barcodes SET unit=?1 WHERE product_id=?2 AND is_primary=1", params![unit,id]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "update", "product", Some(&id.to_string()), &format!("code={code}; name={name}; unit={unit}; barcode_preserved=true"))?;
    let row = tx.query_row("SELECT p.id,p.code,p.name,p.unit,COALESCE((SELECT barcode FROM product_barcodes WHERE product_id=p.id AND is_primary=1 LIMIT 1),''),p.purchase_price_minor,p.sale_price_minor FROM products p WHERE p.id=?1", [id], |r| Ok(ProductRow { id:r.get(0)?,code:r.get(1)?,name:r.get(2)?,unit:r.get(3)?,barcode:r.get(4)?,purchase_price_minor:r.get(5)?,sale_price_minor:r.get(6)? })).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(row)
}

#[tauri::command]
fn add_alternate_barcode(app: tauri::AppHandle, state: State<'_, AppState>, product_id: i64, barcode: String, unit: Option<String>) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.manage")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let barcode = barcode.trim();
    if barcode.is_empty() { return Err("أدخل الباركود البديل أولًا".into()); }
    let product_unit: Option<String> = tx.query_row("SELECT unit FROM products WHERE id=?1 AND is_active=1", [product_id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    let Some(product_unit) = product_unit else { return Err("الصنف غير موجود أو موقوف".into()); };
    let duplicate: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM product_barcodes WHERE barcode=?1)", [barcode], |r| r.get(0)).map_err(|e| e.to_string())?;
    if duplicate { return Err("هذا الباركود مستخدم بالفعل، ولن يتم ربطه بصنف آخر".into()); }
    let unit = unit.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).unwrap_or(product_unit);
    tx.execute("INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,'alternate',0)", params![product_id,unit,barcode]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "add_alternate_barcode", "product", Some(&product_id.to_string()), &format!("barcode={barcode}; unit={unit}"))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(format!("تمت إضافة الباركود البديل {barcode}"))
}

#[tauri::command]
fn deactivate_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64) -> Result<String, String> {
    let actor = require_permission(state.inner(), "products.manage")?;
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

#[derive(Serialize)]
struct StockRow {
    product_id: i64,
    code: String,
    name: String,
    unit: String,
    barcode: String,
    quantity_milli: i64,
    is_active: bool,
}

#[derive(Serialize)]
struct StockMovementRow {
    id: i64,
    product_code: String,
    product_name: String,
    unit: String,
    quantity_milli: i64,
    notes: String,
    created_at: String,
}

#[tauri::command]
fn list_stock(app: tauri::AppHandle, state: State<'_, AppState>, query: String) -> Result<Vec<StockRow>, String> {
    let actor = require_permission(state.inner(), "stock.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let q = query.trim();
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        "SELECT p.id,p.code,p.name,p.unit,
                COALESCE((SELECT b.barcode FROM product_barcodes b WHERE b.product_id=p.id AND b.is_primary=1 LIMIT 1),''),
                COALESCE(SUM(sm.quantity_milli),0),p.is_active
         FROM products p
         LEFT JOIN stock_movements sm ON sm.product_id=p.id
         WHERE (?1='' OR p.name LIKE ?2 OR p.code LIKE ?2 OR EXISTS(
             SELECT 1 FROM product_barcodes bx WHERE bx.product_id=p.id AND bx.barcode LIKE ?2
         ))
         GROUP BY p.id,p.code,p.name,p.unit,p.is_active
         ORDER BY p.is_active DESC,p.name COLLATE NOCASE LIMIT 1000"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![q, like], |r| Ok(StockRow {
        product_id: r.get(0)?,
        code: r.get(1)?,
        name: r.get(2)?,
        unit: r.get(3)?,
        barcode: r.get(4)?,
        quantity_milli: r.get(5)?,
        is_active: r.get::<_, i64>(6)? != 0,
    })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[tauri::command]
fn list_stock_movements(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<StockMovementRow>, String> {
    let actor = require_permission(state.inner(), "stock.view")?;
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
    let actor = require_permission(state.inner(), "stock.manage")?;
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
    write_audit(&tx, Some(&actor), if direction == "in" { "stock_in" } else { "stock_out" }, "product", Some(&product_id.to_string()), &format!("code={product_code}; quantity_milli={quantity_milli}; unit={unit}; note={note}"))?;
    tx.commit().map_err(|e| e.to_string())?;

    Ok(format!(
        "تم تسجيل {} للصنف «{}» بكمية {} {}.",
        if direction == "in" { "الوارد" } else { "المنصرف" },
        product_name,
        quantity_milli as f64 / 1000.0,
        unit,
    ))
}

#[tauri::command]
fn create_backup(app: tauri::AppHandle, state: State<'_, AppState>, destination: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "backup.manage")?;
    let source = database_path(&app)?;
    initialize_database(&source)?;
    let destination_path = PathBuf::from(destination.trim());
    if destination_path.as_os_str().is_empty() {
        return Err("لم يتم تحديد مكان حفظ النسخة الاحتياطية".into());
    }
    if destination_path == source {
        return Err("لا يمكن حفظ النسخة فوق قاعدة البيانات الأصلية".into());
    }
    if let Some(parent) = destination_path.parent() {
        if !parent.is_dir() {
            return Err("مجلد الحفظ غير موجود".into());
        }
    }
    let src = Connection::open(&source).map_err(|e| e.to_string())?;
    let mut dst = Connection::open(&destination_path).map_err(|e| e.to_string())?;
    {
        let backup = rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| e.to_string())?;
        backup.run_to_completion(128, std::time::Duration::from_millis(10), None).map_err(|e| e.to_string())?;
    } // Drop Backup before querying dst.
    dst.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())
        .and_then(|result| if result == "ok" { Ok(()) } else { Err(format!("فشل التحقق من النسخة: {result}")) })?;
    audit_event(&app, Some(&actor), "backup_created", "backup", None, &format!("destination={}", destination_path.display()))?;
    Ok(destination_path.to_string_lossy().to_string())
}

#[derive(Serialize)]
struct RestorePoint {
    path: String,
    file_name: String,
    modified_at_secs: u64,
    size_bytes: u64,
}

#[tauri::command]
fn list_restore_points(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<RestorePoint>, String> {
    let actor = require_permission(state.inner(), "backup.manage")?;
    let current = database_path(&app)?;
    let dir = current.parent().ok_or("تعذر تحديد مجلد قاعدة البيانات")?;
    let mut points = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else { continue; };
        // These are the automatic safety copies created immediately before a restore.
        if !file_name.starts_with("Madar.before-restore.") || !file_name.ends_with(".db") || !path.is_file() {
            continue;
        }
        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        // Ignore invalid/corrupt files so the restore list only offers usable databases.
        let valid = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .and_then(|conn| {
                let check: String = conn.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
                let has_products: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='products')",
                    [], |row| row.get(0))?;
                if check == "ok" && has_products { Ok(()) } else { Err(rusqlite::Error::InvalidQuery) }
            }).is_ok();
        if !valid { continue; }
        let modified_at_secs = metadata.modified().ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()).unwrap_or(0);
        points.push(RestorePoint {
            path: path.to_string_lossy().to_string(),
            file_name: file_name.to_string(),
            modified_at_secs,
            size_bytes: metadata.len(),
        });
    }
    points.sort_by(|a, b| b.modified_at_secs.cmp(&a.modified_at_secs).then_with(|| b.file_name.cmp(&a.file_name)));
    Ok(points)
}

#[tauri::command]
fn delete_restore_point(app: tauri::AppHandle, state: State<'_, AppState>, point_path: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "backup.manage")?;
    let current = database_path(&app)?;
    let dir = current.parent().ok_or("تعذر تحديد مجلد قاعدة البيانات")?;
    let path = PathBuf::from(point_path.trim());
    let parent = path.parent().ok_or("مسار نقطة الاستعادة غير صالح")?;
    if parent != dir {
        return Err("يمكن حذف نقاط الاستعادة المحفوظة داخل مجلد مدار فقط".into());
    }
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("اسم ملف غير صالح")?;
    if !name.starts_with("Madar.before-restore.") || !name.ends_with(".db") {
        return Err("هذا الملف ليس نقطة استعادة معتمدة للحذف".into());
    }
    if !path.is_file() {
        return Err("نقطة الاستعادة غير موجودة بالفعل".into());
    }
    fs::remove_file(&path).map_err(|e| format!("تعذر حذف نقطة الاستعادة: {e}"))?;
    audit_event(&app, Some(&actor), "restore_point_deleted", "backup", None, &format!("file={name}"))?;
    Ok(format!("تم حذف نقطة الاستعادة: {name}"))
}

#[tauri::command]
fn restore_backup(app: tauri::AppHandle, state: State<'_, AppState>, backup_path: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "backup.manage")?;
    let current = database_path(&app)?;
    let backup_path = PathBuf::from(backup_path.trim());
    if !backup_path.is_file() {
        return Err("ملف النسخة الاحتياطية غير موجود".into());
    }
    if backup_path == current {
        return Err("اختر ملف نسخة احتياطية مختلفًا عن قاعدة البيانات الحالية".into());
    }

    // Validate the selected file before changing anything.
    let backup_conn = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("تعذر فتح النسخة الاحتياطية: {e}"))?;
    let check: String = backup_conn.query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|e| format!("ملف النسخة غير صالح: {e}"))?;
    if check != "ok" {
        return Err(format!("النسخة الاحتياطية غير سليمة: {check}"));
    }
    let has_products: bool = backup_conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='products')",
        [], |row| row.get(0)).map_err(|e| e.to_string())?;
    if !has_products {
        return Err("الملف المحدد لا يبدو نسخة من قاعدة بيانات مدار".into());
    }
    drop(backup_conn);

    let parent = current.parent().ok_or("تعذر تحديد مجلد قاعدة البيانات")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let temp = parent.join(format!("Madar.restore.{stamp}.tmp.db"));
    let safety = parent.join(format!("Madar.before-restore.{stamp}.db"));
    if temp.exists() { fs::remove_file(&temp).map_err(|e| e.to_string())?; }
    let src = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    let mut dst = Connection::open(&temp).map_err(|e| e.to_string())?;
    {
        let copier = rusqlite::backup::Backup::new(&src, &mut dst).map_err(|e| e.to_string())?;
        copier.run_to_completion(128, std::time::Duration::from_millis(10), None).map_err(|e| e.to_string())?;
    } // Drop Backup before querying or closing either connection.
    let copied_check: String = dst.query_row("PRAGMA quick_check", [], |row| row.get(0)).map_err(|e| e.to_string())?;
    if copied_check != "ok" {
        let _ = fs::remove_file(&temp);
        return Err(format!("فشل التحقق من النسخة المستعادة: {copied_check}"));
    }
    drop(dst);
    drop(src);

    // Ensure the restored database has the newest security schema, then retain current local accounts,
    // role assignments, and audit history rather than rolling access control back with a data restore.
    initialize_database(&temp)?;
    if current.exists() {
        let merge = Connection::open(&temp).map_err(|e| e.to_string())?;
        merge.pragma_update(None, "foreign_keys", "ON").map_err(|e| e.to_string())?;
        merge.execute("ATTACH DATABASE ?1 AS previous", [current.to_string_lossy().to_string()]).map_err(|e| format!("تعذر إرفاق قاعدة البيانات الحالية للحفاظ على المستخدمين والسجل: {e}"))?;
        let merge_result = merge.execute_batch(
            "BEGIN IMMEDIATE;
             DELETE FROM role_permissions;
             DELETE FROM users;
             DELETE FROM permissions;
             DELETE FROM roles;
             INSERT INTO roles(id,code,name,is_system,created_at) SELECT id,code,name,is_system,created_at FROM previous.roles;
             INSERT INTO permissions(code,name,description) SELECT code,name,description FROM previous.permissions;
             INSERT INTO role_permissions(role_id,permission_code) SELECT role_id,permission_code FROM previous.role_permissions;
             INSERT INTO users(id,username,display_name,password_hash,role_id,is_active,created_at,updated_at,last_login_at)
                SELECT id,username,display_name,password_hash,role_id,is_active,created_at,updated_at,last_login_at FROM previous.users;
             DELETE FROM audit_log;
             INSERT INTO audit_log(id,action,entity_type,entity_id,details,created_at,actor_user_id,actor_username)
                SELECT id,action,entity_type,entity_id,details,created_at,actor_user_id,actor_username FROM previous.audit_log;
             COMMIT;"
        );
        if let Err(error) = merge_result { let _ = merge.execute_batch("ROLLBACK;"); return Err(format!("تعذر الحفاظ على حسابات المستخدمين وسجل التدقيق أثناء الاستعادة: {error}")); }
        merge.execute_batch("DETACH DATABASE previous;").map_err(|e| e.to_string())?;
        drop(merge);
        // Flush the restored database's WAL before renaming its main file.
        if let Ok(checkpoint) = Connection::open(&temp) {
            let _ = checkpoint.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;");
            drop(checkpoint);
        }
        for suffix in ["-wal", "-shm"] {
            let sidecar = PathBuf::from(format!("{}{}", temp.to_string_lossy(), suffix));
            if sidecar.exists() { let _ = fs::remove_file(sidecar); }
        }
    }

    // Checkpoint and close the current database before moving it aside.
    if current.exists() {
        let active = Connection::open(&current).map_err(|e| e.to_string())?;
        active.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").map_err(|e| e.to_string())?;
        drop(active);
        fs::rename(&current, &safety).map_err(|e| format!("تعذر حفظ قاعدة البيانات الحالية قبل الاستعادة: {e}"))?;
    }
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{}", current.to_string_lossy(), suffix));
        if sidecar.exists() { let _ = fs::remove_file(sidecar); }
    }
    if let Err(error) = fs::rename(&temp, &current) {
        if safety.exists() { let _ = fs::rename(&safety, &current); }
        return Err(format!("تعذر تفعيل النسخة المستعادة، وتمت محاولة إعادة قاعدة البيانات السابقة: {error}"));
    }
    let restore_actor = actor.clone();
    let restore_text = format!("restored_from={}; safety_copy={}", backup_path.display(), safety.display());
    let _ = audit_event(&app, Some(&restore_actor), "backup_restored", "database", None, &restore_text);
    *state.session.lock().map_err(|_| "تعذر إنهاء الجلسة بعد الاستعادة")? = None;
    Ok(format!("تمت الاستعادة. احتفظنا بقاعدة البيانات السابقة في: {}. سجّل الدخول مرة أخرى.", safety.display()))
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = database_path(&app.handle()).map_err(std::io::Error::other)?;
            initialize_database(&path).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, auth_status, create_initial_admin, login, logout, list_users, create_user, set_user_active, set_user_role, reset_user_password, change_own_password, list_audit_log, create_product, find_product_by_barcode, list_products, search_products, list_inactive_products, search_inactive_products, reactivate_product, update_product, add_alternate_barcode, deactivate_product, list_stock, list_stock_movements, record_stock_movement, create_backup, list_restore_points, delete_restore_point, restore_backup])
        .run(tauri::generate_context!())
        .expect("خطأ أثناء تشغيل مدار");
}
