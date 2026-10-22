#[tauri::command]
fn create_backup(app: tauri::AppHandle, state: State<'_, AppState>, destination: String) -> Result<String, String> {
    let actor = require_permission(state.inner(), "backup.create")?;
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

#[tauri::command]
fn list_restore_points(app: tauri::AppHandle, state: State<'_, AppState>, ) -> Result<Vec<RestorePoint>, String> {
    let _actor = require_permission(state.inner(), "backup.view_points")?;
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
    let actor = require_permission(state.inner(), "backup.points.delete")?;
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
    let actor = require_permission(state.inner(), "backup.restore")?;
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
        let previous_has_overrides: bool = merge.query_row("SELECT EXISTS(SELECT 1 FROM previous.sqlite_master WHERE type='table' AND name='user_permission_overrides')", [], |r| r.get(0)).unwrap_or(false);
        let copy_overrides_sql = if previous_has_overrides {
            "INSERT INTO user_permission_overrides(user_id,permission_code,allowed) SELECT user_id,permission_code,allowed FROM previous.user_permission_overrides;"
        } else { "" };
        let merge_sql = format!(
            "BEGIN IMMEDIATE;
             DELETE FROM user_permission_overrides;
             DELETE FROM role_permissions;
             DELETE FROM users;
             DELETE FROM roles;
             INSERT INTO roles(id,code,name,is_system,created_at) SELECT id,code,name,is_system,created_at FROM previous.roles;
             INSERT OR IGNORE INTO permissions(code,name,description) SELECT code,name,description FROM previous.permissions;
             INSERT OR IGNORE INTO role_permissions(role_id,permission_code) SELECT role_id,permission_code FROM previous.role_permissions;
             INSERT INTO users(id,username,display_name,password_hash,role_id,is_active,created_at,updated_at,last_login_at,email,phone,address,avatar_data_url)
                SELECT id,username,display_name,password_hash,role_id,is_active,created_at,updated_at,last_login_at,email,phone,address,avatar_data_url FROM previous.users;
             {copy_overrides_sql}
             DELETE FROM audit_log;
             INSERT INTO audit_log(id,action,entity_type,entity_id,details,created_at,actor_user_id,actor_username)
                SELECT id,action,entity_type,entity_id,details,created_at,actor_user_id,actor_username FROM previous.audit_log;
             COMMIT;"
        );
        let merge_result = merge.execute_batch(&merge_sql);
        if let Err(error) = merge_result { let _ = merge.execute_batch("ROLLBACK;"); return Err(format!("تعذر الحفاظ على حسابات المستخدمين وسجل التدقيق أثناء الاستعادة: {error}")); }
        merge.execute_batch("DETACH DATABASE previous;").map_err(|e| e.to_string())?;
        seed_access_control(&merge)?;
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
