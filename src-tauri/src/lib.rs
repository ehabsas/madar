use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
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
            last_login_at TEXT, email TEXT, phone TEXT, address TEXT, avatar_data_url TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_users_role ON users(role_id);
        CREATE TABLE IF NOT EXISTS user_permission_overrides (
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            permission_code TEXT NOT NULL REFERENCES permissions(code) ON DELETE CASCADE,
            allowed INTEGER NOT NULL CHECK (allowed IN (0,1)),
            PRIMARY KEY(user_id, permission_code)
        );
        CREATE TABLE IF NOT EXISTS customers (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            email TEXT, phone TEXT, address TEXT, notes TEXT,
            is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_customers_name ON customers(name);
        CREATE TABLE IF NOT EXISTS suppliers (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
            email TEXT, phone TEXT, address TEXT, notes TEXT,
            is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0,1)),
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_suppliers_name ON suppliers(name);
        CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_audit_log_filters ON audit_log(entity_type, action, id DESC);
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
        CREATE TABLE IF NOT EXISTS purchase_invoices (
            id INTEGER PRIMARY KEY, invoice_no TEXT NOT NULL UNIQUE,
            supplier_id INTEGER NOT NULL REFERENCES suppliers(id), invoice_date TEXT NOT NULL,
            subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK (subtotal_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            total_minor INTEGER NOT NULL DEFAULT 0 CHECK (total_minor >= 0),
            tax_rate_bps INTEGER NOT NULL DEFAULT 0, tax_minor INTEGER NOT NULL DEFAULT 0, prices_include_tax INTEGER NOT NULL DEFAULT 0, notes TEXT,
            status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','posted','cancelled')),
            created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')), posted_at TEXT, cancelled_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_purchase_invoices_date ON purchase_invoices(invoice_date, id DESC);
        CREATE TABLE IF NOT EXISTS purchase_invoice_items (
            id INTEGER PRIMARY KEY,
            purchase_invoice_id INTEGER NOT NULL REFERENCES purchase_invoices(id) ON DELETE CASCADE,
            product_id INTEGER NOT NULL REFERENCES products(id), unit TEXT NOT NULL DEFAULT 'قطعة', quantity_milli INTEGER NOT NULL CHECK (quantity_milli > 0),
            unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            line_total_minor INTEGER NOT NULL CHECK (line_total_minor >= 0)
        );
        CREATE INDEX IF NOT EXISTS idx_purchase_invoice_items_invoice ON purchase_invoice_items(purchase_invoice_id);
        CREATE TABLE IF NOT EXISTS sales (
            id INTEGER PRIMARY KEY, invoice_no TEXT NOT NULL UNIQUE,
            customer_id INTEGER REFERENCES customers(id), invoice_date TEXT NOT NULL,
            subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK (subtotal_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            total_minor INTEGER NOT NULL DEFAULT 0 CHECK (total_minor >= 0),
            tax_rate_bps INTEGER NOT NULL DEFAULT 0, tax_minor INTEGER NOT NULL DEFAULT 0, prices_include_tax INTEGER NOT NULL DEFAULT 0,
            paid_minor INTEGER NOT NULL DEFAULT 0 CHECK (paid_minor >= 0),
            notes TEXT, status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','posted','cancelled')),
            created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')), posted_at TEXT, cancelled_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_sales_date ON sales(invoice_date, id DESC);
        CREATE TABLE IF NOT EXISTS sale_items (
            id INTEGER PRIMARY KEY, sale_id INTEGER NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
            product_id INTEGER NOT NULL REFERENCES products(id), unit TEXT NOT NULL DEFAULT 'قطعة', quantity_milli INTEGER NOT NULL CHECK (quantity_milli > 0),
            unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            line_total_minor INTEGER NOT NULL CHECK (line_total_minor >= 0)
        );
        CREATE INDEX IF NOT EXISTS idx_sale_items_invoice ON sale_items(sale_id);
        INSERT OR IGNORE INTO warehouses(code,name) VALUES ('MAIN','المخزن الرئيسي');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('app_name','مدار');
        INSERT OR IGNORE INTO app_settings(key,value) VALUES ('database_schema_version','10');
        UPDATE app_settings SET value='10', updated_at=datetime('now') WHERE key='database_schema_version' AND value IN ('1','2','3','4','5','6','7','8','9');
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

    // Additive user profile migration: safe for existing local databases.
    for (column, declaration) in [("email", "TEXT"), ("phone", "TEXT"), ("address", "TEXT"), ("avatar_data_url", "TEXT")] {
        if !table_has_column(&conn, "users", column)? {
            conn.execute(&format!("ALTER TABLE users ADD COLUMN {column} {declaration}"), []).map_err(|e| e.to_string())?;
        }
    }

    // Additive migration for customer/supplier contact fields and invoice lifecycle metadata.
    for table in ["customers", "suppliers"] {
        if !table_has_column(&conn, table, "email")? {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN email TEXT"), []).map_err(|e| e.to_string())?;
        }
        if !table_has_column(&conn, table, "notes")? {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN notes TEXT"), []).map_err(|e| e.to_string())?;
        }
    }
    for table in ["purchase_invoices", "sales"] {
        for (column, declaration) in [("created_by", "INTEGER"), ("created_by_username", "TEXT"), ("updated_at", "TEXT"), ("posted_at", "TEXT"), ("cancelled_at", "TEXT")] {
            if !table_has_column(&conn, table, column)? {
                conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {declaration}"), []).map_err(|e| e.to_string())?;
            }
        }
        conn.execute(&format!("UPDATE {table} SET updated_at=COALESCE(updated_at,created_at,datetime('now')) WHERE updated_at IS NULL OR trim(updated_at)=''"), []).map_err(|e| e.to_string())?;
    }
    for table in ["purchase_invoice_items", "sale_items"] {
        if !table_has_column(&conn, table, "unit")? {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN unit TEXT"), []).map_err(|e| e.to_string())?;
        }
        conn.execute(&format!("UPDATE {table} SET unit=(SELECT p.unit FROM products p WHERE p.id={table}.product_id) WHERE unit IS NULL OR trim(unit)=''"), []).map_err(|e| e.to_string())?;
    }

    // Additive migration for databases created before user attribution was introduced.
    let has_actor_id = table_has_column(&conn, "audit_log", "actor_user_id")?;
    if !has_actor_id {
        conn.execute("ALTER TABLE audit_log ADD COLUMN actor_user_id INTEGER", []).map_err(|e| e.to_string())?;
    }
    let has_actor_name = table_has_column(&conn, "audit_log", "actor_username")?;
    if !has_actor_name {
        conn.execute("ALTER TABLE audit_log ADD COLUMN actor_username TEXT", []).map_err(|e| e.to_string())?;
    }
    // Commercial operations: quotations, payments, cashbook, company profile and reporting.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS quotations (
            id INTEGER PRIMARY KEY,
            quote_no TEXT NOT NULL UNIQUE,
            customer_id INTEGER NOT NULL REFERENCES customers(id),
            quote_date TEXT NOT NULL,
            valid_until TEXT,
            subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK (subtotal_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            total_minor INTEGER NOT NULL DEFAULT 0 CHECK (total_minor >= 0),
            tax_rate_bps INTEGER NOT NULL DEFAULT 0, tax_minor INTEGER NOT NULL DEFAULT 0, prices_include_tax INTEGER NOT NULL DEFAULT 0,
            notes TEXT,
            status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','sent','accepted','rejected','expired','converted','cancelled')),
            converted_sale_id INTEGER,
            created_by INTEGER,
            created_by_username TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_quotations_date ON quotations(quote_date,id DESC);
        CREATE TABLE IF NOT EXISTS quotation_items (
            id INTEGER PRIMARY KEY,
            quotation_id INTEGER NOT NULL REFERENCES quotations(id) ON DELETE CASCADE,
            product_id INTEGER NOT NULL REFERENCES products(id),
            unit TEXT NOT NULL DEFAULT 'قطعة',
            quantity_milli INTEGER NOT NULL CHECK (quantity_milli > 0),
            unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
            discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
            line_total_minor INTEGER NOT NULL CHECK (line_total_minor >= 0)
        );
        CREATE INDEX IF NOT EXISTS idx_quotation_items_quote ON quotation_items(quotation_id);
        CREATE TABLE IF NOT EXISTS payments (
            id INTEGER PRIMARY KEY,
            invoice_type TEXT NOT NULL CHECK (invoice_type IN ('sale','purchase')),
            invoice_id INTEGER NOT NULL,
            direction TEXT NOT NULL CHECK (direction IN ('in','out')),
            amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
            payment_date TEXT NOT NULL,
            method TEXT NOT NULL DEFAULT 'cash' CHECK (method IN ('cash','bank','card','transfer','other')),
            reference_no TEXT,
            notes TEXT,
            created_by INTEGER,
            created_by_username TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_payments_invoice ON payments(invoice_type,invoice_id,id DESC);
        CREATE INDEX IF NOT EXISTS idx_payments_date ON payments(payment_date,id DESC);
        CREATE TABLE IF NOT EXISTS cashbook_entries (
            id INTEGER PRIMARY KEY,
            direction TEXT NOT NULL CHECK (direction IN ('in','out')),
            amount_minor INTEGER NOT NULL CHECK (amount_minor > 0),
            entry_date TEXT NOT NULL,
            source_type TEXT NOT NULL,
            source_id INTEGER,
            description TEXT NOT NULL DEFAULT '',
            notes TEXT,
            created_by INTEGER,
            created_by_username TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_cashbook_date ON cashbook_entries(entry_date,id DESC);
        INSERT OR IGNORE INTO app_settings(key,value) VALUES
        ('company_name',''),('company_phone',''),('company_email',''),('company_address',''),
        ('company_tax_number',''),('company_commercial_register',''),('company_website',''),
        ('company_logo_data_url',''),('company_invoice_footer',''),('currency_code','EGP'),
        ('company_tax_rate_bps','0'),('company_prices_include_tax','false'),
        ('company_schema_version','2');"
    ).map_err(|e| e.to_string())?;
    // Additive tax snapshot migration: existing documents retain their original totals and get a zero-tax snapshot.
    for table in ["purchase_invoices", "sales", "quotations"] {
        for (column, declaration) in [("tax_rate_bps", "INTEGER NOT NULL DEFAULT 0"), ("tax_minor", "INTEGER NOT NULL DEFAULT 0"), ("prices_include_tax", "INTEGER NOT NULL DEFAULT 0")] {
            if !table_has_column(&conn, table, column)? {
                conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} {declaration}"), []).map_err(|e| e.to_string())?;
            }
        }
    }
    if !table_has_column(&conn, "purchase_invoices", "paid_minor")? {
        conn.execute("ALTER TABLE purchase_invoices ADD COLUMN paid_minor INTEGER NOT NULL DEFAULT 0 CHECK (paid_minor >= 0)", []).map_err(|e| e.to_string())?;
    }
    if !table_has_column(&conn, "sale_items", "cost_price_minor")? {
        conn.execute("ALTER TABLE sale_items ADD COLUMN cost_price_minor INTEGER", []).map_err(|e| e.to_string())?;
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
        // Legacy broad permissions retained for compatibility with existing databases.
        ("products.view", "عرض المنتجات", "عرض وبحث الأصناف والباركودات"),
        ("products.manage", "إدارة المنتجات (قديم)", "صلاحية شاملة قديمة لإدارة الأصناف"),
        ("stock.view", "عرض المخزون", "عرض الأرصدة وسجل الحركات"),
        ("stock.manage", "إدارة المخزون (قديم)", "صلاحية شاملة قديمة لحركات المخزون"),
        ("backup.manage", "النسخ والاستعادة (قديم)", "صلاحية شاملة قديمة للنسخ والاستعادة"),
        ("users.manage", "إدارة المستخدمين (قديم)", "صلاحية شاملة قديمة لإدارة المستخدمين"),
        ("audit.view", "عرض سجل النشاط", "عرض عمليات المستخدمين وسجل التدقيق"),
        // Product-level permissions.
        ("products.create", "إضافة صنف", "إنشاء أصناف جديدة"),
        ("products.edit", "تعديل بيانات الصنف", "تعديل الكود والاسم والوحدة والبيانات العامة"),
        ("products.deactivate", "إيقاف صنف", "إيقاف الصنف دون حذفه نهائيًا"),
        ("products.reactivate", "إعادة تفعيل صنف", "إرجاع الصنف الموقوف إلى الأصناف النشطة"),
        ("products.barcode.manage", "إدارة الباركودات", "إضافة باركودات بديلة للصنف"),
        ("products.purchase_price.edit", "تعديل سعر الشراء", "تغيير سعر الشراء عند إنشاء أو تعديل صنف"),
        ("products.sale_price.edit", "تعديل سعر البيع", "تغيير سعر البيع عند إنشاء أو تعديل صنف"),
        // Stock permissions.
        ("stock.movement.record", "تسجيل حركات المخزون", "تسجيل الوارد والمنصرف يدويًا"),
        // Backup permissions.
        ("backup.view_points", "عرض نقاط الاستعادة", "عرض نقاط الاستعادة المحفوظة داخل مدار"),
        ("backup.create", "إنشاء نسخة احتياطية", "حفظ نسخة احتياطية إلى ملف"),
        ("backup.restore", "استعادة نسخة احتياطية", "استعادة قاعدة البيانات من نسخة"),
        ("backup.points.delete", "حذف نقطة استعادة", "حذف نقاط الاستعادة التي أنشأها مدار"),
        // User administration permissions.
        ("users.view", "عرض المستخدمين", "عرض قائمة حسابات المستخدمين"),
        ("users.profile.edit", "تعديل بيانات المستخدم", "تعديل البريد والهاتف والعنوان والصورة والاسم الظاهر"),
        ("users.create", "إنشاء مستخدم", "إنشاء حساب مستخدم جديد"),
        ("users.status.manage", "تفعيل وإيقاف الحسابات", "تفعيل أو إيقاف حساب المستخدم"),
        ("users.role.manage", "تغيير أدوار المستخدمين", "تغيير دور المستخدم"),
        ("users.permissions.manage", "تخصيص صلاحيات المستخدم", "ضبط الصلاحيات الفردية لكل مستخدم؛ لمدير النظام فقط"),
        ("users.password.reset", "إعادة تعيين كلمة المرور", "تعيين كلمة مرور جديدة لمستخدم آخر"),
        ("customers.view", "عرض العملاء", "عرض والبحث في سجل العملاء"),
        ("customers.create", "إضافة عميل", "إنشاء سجل عميل جديد"),
        ("customers.edit", "تعديل بيانات العميل", "تعديل بيانات الاتصال والعنوان والملاحظات"),
        ("customers.status.manage", "إيقاف وتفعيل العملاء", "إيقاف العميل من الاستخدام الجديد أو إعادة تفعيله"),
        ("suppliers.view", "عرض الموردين", "عرض والبحث في سجل الموردين"),
        ("suppliers.create", "إضافة مورد", "إنشاء سجل مورد جديد"),
        ("suppliers.edit", "تعديل بيانات المورد", "تعديل بيانات الاتصال والعنوان والملاحظات"),
        ("suppliers.status.manage", "إيقاف وتفعيل الموردين", "إيقاف المورد من الاستخدام الجديد أو إعادة تفعيله"),
        ("purchases.view", "عرض فواتير المشتريات", "عرض تفاصيل فواتير الشراء وحالاتها"),
        ("purchases.create", "إنشاء فاتورة مشتريات", "حفظ فاتورة مشتريات كمسودة"),
        ("purchases.edit_draft", "تعديل مسودة مشتريات", "تعديل فواتير المشتريات قبل اعتمادها فقط"),
        ("purchases.post", "اعتماد فاتورة مشتريات", "اعتماد فاتورة مشتريات وإضافة الكميات للمخزون"),
        ("purchases.cancel", "إلغاء فاتورة مشتريات", "إلغاء المسودة أو عكس أثر فاتورة معتمدة بأمان"),
        ("sales.view", "عرض فواتير المبيعات", "عرض تفاصيل فواتير البيع وحالاتها"),
        ("sales.create", "إنشاء فاتورة مبيعات", "حفظ فاتورة مبيعات كمسودة"),
        ("sales.edit_draft", "تعديل مسودة مبيعات", "تعديل فواتير المبيعات قبل اعتمادها فقط"),
        ("sales.post", "اعتماد فاتورة مبيعات", "اعتماد فاتورة بيع وخصم الكميات من المخزون"),
        ("sales.cancel", "إلغاء فاتورة مبيعات", "إلغاء المسودة أو عكس أثر فاتورة معتمدة بأمان"),
        ("sales.price.override", "تجاوز سعر البيع", "تغيير سعر البيع داخل الفاتورة عن السعر المسجل للصنف"),
        ("company.settings.view", "عرض بيانات الشركة", "عرض بيانات الشركة المستخدمة في المطبوعات"),
        ("company.settings.manage", "إدارة بيانات الشركة", "تعديل بيانات الشركة والضريبة وطريقة شمول الأسعار للضريبة وبيانات الفواتير المطبوعة"),
        ("quotations.view", "عرض عروض الأسعار", "عرض وبحث عروض الأسعار وتفاصيلها"),
        ("quotations.create", "إنشاء عرض سعر", "إنشاء عرض سعر جديد كمسودة"),
        ("quotations.edit", "تعديل عرض سعر", "تعديل عرض السعر طالما لم يتم تحويله إلى فاتورة"),
        ("quotations.status.manage", "تحديث حالة عرض السعر", "إرسال العرض أو قبوله أو رفضه أو إلغاؤه"),
        ("quotations.convert", "تحويل عرض السعر إلى فاتورة", "إنشاء مسودة مبيعات من عرض سعر مقبول"),
        ("finance.view", "عرض المدفوعات والحسابات", "عرض المدفوعات والمتبقي على الفواتير"),
        ("finance.payment.record", "تسجيل دفعة", "تسجيل تحصيل من عميل أو سداد لمورد"),
        ("cashbox.view", "عرض الخزنة", "عرض سجل حركة النقدية ورصيد الخزنة"),
        ("cashbox.adjust", "تسوية الخزنة", "تسجيل إضافة أو صرف نقدي غير مرتبط بفاتورة"),
        ("reports.view", "عرض التقارير", "عرض الملخصات المالية والأصناف منخفضة المخزون"),
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
    // Detailed permission defaults; insert-only so existing user/custom overrides are never overwritten.
    let admin_detailed = ["products.view","products.create","products.edit","products.deactivate","products.reactivate","products.barcode.manage","products.purchase_price.edit","products.sale_price.edit","stock.view","stock.movement.record","backup.view_points","backup.create","backup.restore","backup.points.delete","users.view","users.profile.edit","users.create","users.status.manage","users.role.manage","users.permissions.manage","users.password.reset","audit.view","customers.view","customers.create","customers.edit","customers.status.manage","suppliers.view","suppliers.create","suppliers.edit","suppliers.status.manage","purchases.view","purchases.create","purchases.edit_draft","purchases.post","purchases.cancel","sales.view","sales.create","sales.edit_draft","sales.post","sales.cancel","sales.price.override","company.settings.view","company.settings.manage","quotations.view","quotations.create","quotations.edit","quotations.status.manage","quotations.convert","finance.view","finance.payment.record","cashbox.view","cashbox.adjust","reports.view"];
    let manager_detailed = ["products.view","products.create","products.edit","products.deactivate","products.reactivate","products.barcode.manage","products.purchase_price.edit","products.sale_price.edit","stock.view","stock.movement.record","backup.view_points","backup.create","backup.restore","backup.points.delete","audit.view","customers.view","customers.create","customers.edit","customers.status.manage","suppliers.view","suppliers.create","suppliers.edit","suppliers.status.manage","purchases.view","purchases.create","purchases.edit_draft","purchases.post","purchases.cancel","sales.view","sales.create","sales.edit_draft","sales.post","sales.cancel","sales.price.override","company.settings.view","quotations.view","quotations.create","quotations.edit","quotations.status.manage","quotations.convert","finance.view","finance.payment.record","cashbox.view","cashbox.adjust","reports.view"];
    let warehouse_detailed = ["products.view","stock.view","stock.movement.record","suppliers.view","purchases.view","company.settings.view"];
    let sales_detailed = ["products.view","stock.view","customers.view","customers.create","sales.view","sales.create","sales.edit_draft","sales.post","company.settings.view","quotations.view","quotations.create","quotations.edit","quotations.status.manage","quotations.convert","finance.view","finance.payment.record","cashbox.view","reports.view"];
    let viewer_detailed = ["products.view","stock.view","company.settings.view"];
    let detailed_matrices: [(&str, &[&str]); 5] = [("admin", &admin_detailed), ("manager", &manager_detailed), ("warehouse", &warehouse_detailed), ("sales", &sales_detailed), ("viewer", &viewer_detailed)];
    for (role, items) in detailed_matrices {
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
    avatar_data_url: Option<String>,
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
    let (username, display_name, role_code, role_name, avatar_data_url): (String,String,String,String,Option<String>) = conn.query_row(
        "SELECT u.username,u.display_name,r.code,r.name,u.avatar_data_url FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1 AND u.is_active=1",
        [user_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
    ).map_err(|_| "المستخدم غير موجود أو موقوف".to_string())?;
    let mut permissions = Vec::new();
    if role_code == "admin" {
        // System administrators always retain every registered permission, even if stale overrides exist.
        let mut stmt = conn.prepare("SELECT code FROM permissions ORDER BY code").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
        for row in rows { permissions.push(row.map_err(|e| e.to_string())?); }
    } else {
        let mut stmt = conn.prepare(
            "SELECT p.code FROM permissions p
             JOIN users u ON u.id=?1
             LEFT JOIN role_permissions rp ON rp.role_id=u.role_id AND rp.permission_code=p.code
             LEFT JOIN user_permission_overrides up ON up.user_id=u.id AND up.permission_code=p.code
             WHERE COALESCE(up.allowed, CASE WHEN rp.permission_code IS NULL THEN 0 ELSE 1 END)=1
             ORDER BY p.code"
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([user_id], |r| r.get::<_,String>(0)).map_err(|e| e.to_string())?;
        for row in rows { permissions.push(row.map_err(|e| e.to_string())?); }
    }
    Ok(UserSession { id:user_id, username, display_name, role_code, role_name, avatar_data_url, permissions })
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

fn clean_optional_profile(value: Option<String>, label: &str, max_chars: usize) -> Result<Option<String>, String> {
    let cleaned = value.unwrap_or_default().trim().to_string();
    if cleaned.is_empty() { return Ok(None); }
    if cleaned.chars().count() > max_chars { return Err(format!("حقل {label} أطول من الحد المسموح ({max_chars} حرفًا)")); }
    Ok(Some(cleaned))
}

fn validate_profile_email(value: Option<String>) -> Result<Option<String>, String> {
    let email = clean_optional_profile(value, "البريد الإلكتروني", 254)?;
    if let Some(ref e) = email {
        let valid = e.split_once('@').map(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')).unwrap_or(false);
        if !valid || e.chars().any(char::is_whitespace) { return Err("صيغة البريد الإلكتروني غير صحيحة".into()); }
    }
    Ok(email)
}

fn validate_avatar_data_url(value: Option<String>) -> Result<Option<String>, String> {
    let Some(data) = value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) else { return Ok(None); };
    if data.len() > 1_500_000 { return Err("الصورة كبيرة جدًا؛ اختر صورة أصغر من 500 كيلوبايت تقريبًا".into()); }
    let allowed = ["data:image/jpeg;base64,", "data:image/png;base64,", "data:image/webp;base64,"];
    if !allowed.iter().any(|prefix| data.starts_with(prefix)) { return Err("صيغة الصورة غير مدعومة؛ استخدم JPG أو PNG أو WebP".into()); }
    Ok(Some(data))
}

fn audit_event(app: &tauri::AppHandle, actor: Option<&UserSession>, action: &str, entity_type: &str, entity_id: Option<&str>, details: &str) -> Result<(), String> {
    let path = database_path(app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    write_audit(&conn, actor, action, entity_type, entity_id, details)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserRow { id: i64, username: String, display_name: String, role_code: String, role_name: String, is_active: bool, created_at: String, last_login_at: Option<String>, permission_overrides: i64, email: Option<String>, phone: Option<String>, address: Option<String>, avatar_data_url: Option<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionDetail { code: String, name: String, description: String, allowed: bool, role_allowed: bool, is_override: bool }

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
    let actor = require_permission(state.inner(), "users.profile.edit")?;
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
            if let Some(ref mut session) = *guard { session.display_name = display_name.to_string(); session.avatar_data_url = avatar_data_url.clone(); }
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

#[tauri::command(rename_all = "camelCase")]
fn list_audit_log(app: tauri::AppHandle, state: State<'_, AppState>, query: Option<String>, entity_type: Option<String>, action: Option<String>) -> Result<Vec<AuditRow>, String> {
    let _actor=require_permission(state.inner(),"audit.view")?;
    let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
    let q=query.unwrap_or_default();let normalized_query=q.trim();let like=format!("%{}%",normalized_query);
    let kind=entity_type.unwrap_or_default().trim().to_ascii_lowercase();
    let act=action.unwrap_or_default().trim().to_ascii_lowercase();
    let mut stmt=conn.prepare("SELECT id,action,entity_type,entity_id,details,actor_username,created_at FROM audit_log WHERE (?1='' OR action LIKE ?2 OR entity_type LIKE ?2 OR COALESCE(entity_id,'') LIKE ?2 OR COALESCE(details,'') LIKE ?2 OR COALESCE(actor_username,'') LIKE ?2) AND (?3='' OR (?3='stock' AND (lower(trim(entity_type))='stock' OR lower(trim(action)) IN ('stock_in','stock_out'))) OR (?3='product' AND lower(trim(entity_type))='product' AND lower(trim(action)) NOT IN ('stock_in','stock_out')) OR (?3 NOT IN ('stock','product') AND lower(trim(entity_type))=?3)) AND (?4='' OR lower(trim(action))=?4) ORDER BY id DESC LIMIT 500").map_err(|e|e.to_string())?;
    let rows=stmt.query_map(params![normalized_query,like,kind,act],|r|Ok(AuditRow{id:r.get(0)?,action:r.get(1)?,entity_type:r.get(2)?,entity_id:r.get(3)?,details:r.get(4)?,actor_username:r.get(5)?,created_at:r.get(6)?})).map_err(|e|e.to_string())?;
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
    let actor = require_permission(state.inner(), "products.create")?;
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
    let purchase_price_minor = if actor.permissions.iter().any(|p| p == "products.purchase_price.edit") { purchase_price_minor } else { 0 };
    let sale_price_minor = if actor.permissions.iter().any(|p| p == "products.sale_price.edit") { sale_price_minor } else { 0 };
    tx.execute("INSERT INTO products(code,name,unit,purchase_price_minor,sale_price_minor) VALUES (?1,?2,?3,?4,?5)", params![product_code, product_name, unit_value, purchase_price_minor, sale_price_minor]).map_err(|e| e.to_string())?;
    let id = tx.last_insert_rowid();
    tx.execute("INSERT INTO product_barcodes(product_id,unit,barcode,barcode_type,is_primary) VALUES (?1,?2,?3,?4,1)", params![id, unit_value, barcode_value, barcode_type]).map_err(|e| e.to_string())?;
    write_audit(&tx, Some(&actor), "create", "product", Some(&id.to_string()), &format!("code={}; barcode={}", product_code, barcode_value))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(ProductRow { id, code: product_code.to_string(), name: product_name.to_string(), unit: unit_value, barcode: barcode_value, purchase_price_minor, sale_price_minor })
}

#[tauri::command]
fn find_product_by_barcode(app: tauri::AppHandle, state: State<'_, AppState>, barcode: String) -> Result<Option<ProductRow>, String> {
    let _actor = require_permission(state.inner(), "products.view")?;
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
fn update_product(app: tauri::AppHandle, state: State<'_, AppState>, id: i64, code: String, name: String, unit: String, purchase_price_minor: i64, sale_price_minor: i64) -> Result<ProductRow, String> {
    let actor = require_permission(state.inner(), "products.edit")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let mut conn = connect(&path)?;
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let code = code.trim();
    let name = name.trim();
    let unit = unit.trim();
    if code.is_empty() || name.is_empty() || unit.is_empty() { return Err("الكود والاسم والوحدة حقول مطلوبة".into()); }
    if purchase_price_minor < 0 || sale_price_minor < 0 { return Err("السعر لا يمكن أن يكون سالبًا".into()); }
    let current: Option<(String,i64,i64)> = tx.query_row("SELECT unit,purchase_price_minor,sale_price_minor FROM products WHERE id=?1 AND is_active=1", [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e| e.to_string())?;
    let Some((current_unit,current_purchase_price,current_sale_price)) = current else { return Err("الصنف غير موجود أو موقوف".into()); };
    if purchase_price_minor != current_purchase_price && !actor.permissions.iter().any(|p| p == "products.purchase_price.edit") { return Err("ليس لديك صلاحية تعديل سعر الشراء".into()); }
    if sale_price_minor != current_sale_price && !actor.permissions.iter().any(|p| p == "products.sale_price.edit") { return Err("ليس لديك صلاحية تعديل سعر البيع".into()); }
    if current_unit != unit {
        let draft_refs: i64 = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM purchase_invoice_items li JOIN purchase_invoices h ON h.id=li.purchase_invoice_id WHERE li.product_id=?1 AND h.status='draft') + (SELECT COUNT(*) FROM sale_items li JOIN sales h ON h.id=li.sale_id WHERE li.product_id=?1 AND h.status='draft')",
            [id], |r| r.get(0)
        ).map_err(|e| e.to_string())?;
        if draft_refs > 0 {
            return Err("لا يمكن تغيير وحدة الصنف مع وجود فواتير مسودة تستخدمه؛ عدّل أو ألغِ المسودات أولًا.".into());
        }
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
    let actor = require_permission(state.inner(), "products.barcode.manage")?;
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
    let _actor = require_permission(state.inner(), "stock.view")?;
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

// Customer/supplier master data and transactional purchase/sales invoice lifecycle.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PartyRow {
    id: i64, code: String, name: String, email: Option<String>, phone: Option<String>,
    address: Option<String>, notes: Option<String>, is_active: bool, created_at: String,
}

#[tauri::command(rename_all = "camelCase")]
fn list_parties(app: tauri::AppHandle, state: State<'_, AppState>, entity_type: String, query: Option<String>) -> Result<Vec<PartyRow>, String> {
    let table = match entity_type.as_str() { "customer" => "customers", "supplier" => "suppliers", _ => return Err("نوع جهة الاتصال غير صالح".into()) };
    let permission = if table == "customers" { "customers.view" } else { "suppliers.view" };
    let _actor = require_permission(state.inner(), permission)?;
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let q = query.unwrap_or_default(); let q = q.trim(); let like = format!("%{q}%");
    let sql = format!("SELECT id,code,name,email,phone,address,notes,is_active,created_at FROM {table} WHERE (?1='' OR code LIKE ?2 OR name LIKE ?2 OR COALESCE(email,'') LIKE ?2 OR COALESCE(phone,'') LIKE ?2 OR COALESCE(address,'') LIKE ?2) ORDER BY is_active DESC,name COLLATE NOCASE LIMIT 1000");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![q,like], |r| Ok(PartyRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, email:r.get(3)?, phone:r.get(4)?, address:r.get(5)?, notes:r.get(6)?, is_active:r.get::<_,i64>(7)?!=0, created_at:r.get(8)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn save_party(app: tauri::AppHandle, state: State<'_, AppState>, entity_type: String, id: Option<i64>, code: String, name: String, email: Option<String>, phone: Option<String>, address: Option<String>, notes: Option<String>) -> Result<String, String> {
    let table = match entity_type.as_str() { "customer" => "customers", "supplier" => "suppliers", _ => return Err("نوع جهة الاتصال غير صالح".into()) };
    let (create_perm, edit_perm) = if table == "customers" { ("customers.create","customers.edit") } else { ("suppliers.create","suppliers.edit") };
    let actor = require_permission(state.inner(), if id.is_some() { edit_perm } else { create_perm })?;
    let code = code.trim(); let name = name.trim();
    if code.is_empty() || name.is_empty() { return Err("الكود والاسم حقول مطلوبة".into()); }
    if code.chars().count() > 60 || name.chars().count() > 180 { return Err("الكود أو الاسم أطول من الحد المسموح".into()); }
    let email = validate_profile_email(email)?;
    let phone = clean_optional_profile(phone, "رقم الهاتف", 60)?;
    let address = clean_optional_profile(address, "العنوان", 500)?;
    let notes = clean_optional_profile(notes, "الملاحظات", 1500)?;
    let path = database_path(&app)?; initialize_database(&path)?; let mut conn = connect(&path)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
    if let Some(id) = id {
        let exists: bool = tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE id=?1)"), [id], |r| Ok(r.get::<_,i64>(0)? != 0)).map_err(|e| e.to_string())?;
        if !exists { return Err("سجل جهة الاتصال غير موجود".into()); }
        tx.execute(&format!("UPDATE {table} SET code=?1,name=?2,email=?3,phone=?4,address=?5,notes=?6,updated_at=datetime('now') WHERE id=?7"), params![code,name,email,phone,address,notes,id]).map_err(|e| format!("تعذر تعديل البيانات (تأكد أن الكود غير مستخدم): {e}"))?;
        write_audit(&tx,Some(&actor),if table=="customers" {"customer_updated"} else {"supplier_updated"},table,Some(&id.to_string()),&format!("code={code}; name={name}"))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(format!("تم تحديث البيانات: {name}"))
    } else {
        tx.execute(&format!("INSERT INTO {table}(code,name,email,phone,address,notes) VALUES (?1,?2,?3,?4,?5,?6)"), params![code,name,email,phone,address,notes]).map_err(|e| format!("تعذر إنشاء السجل (تأكد أن الكود غير مستخدم): {e}"))?;
        let new_id = tx.last_insert_rowid();
        write_audit(&tx,Some(&actor),if table=="customers" {"customer_created"} else {"supplier_created"},table,Some(&new_id.to_string()),&format!("code={code}; name={name}"))?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(format!("تمت إضافة السجل: {name}"))
    }
}

#[tauri::command(rename_all = "camelCase")]
fn set_party_active(app: tauri::AppHandle, state: State<'_, AppState>, entity_type: String, id: i64, active: bool) -> Result<String, String> {
    let table = match entity_type.as_str() { "customer" => "customers", "supplier" => "suppliers", _ => return Err("نوع جهة الاتصال غير صالح".into()) };
    let permission = if table == "customers" { "customers.status.manage" } else { "suppliers.status.manage" };
    let actor = require_permission(state.inner(), permission)?;
    let path = database_path(&app)?; initialize_database(&path)?; let mut conn = connect(&path)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
    let row: Option<(String,i64)> = tx.query_row(&format!("SELECT name,is_active FROM {table} WHERE id=?1"), [id], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e| e.to_string())?;
    let Some((name,current)) = row else { return Err("سجل جهة الاتصال غير موجود".into()); };
    if (current != 0) == active { return Err("حالة جهة الاتصال لم تتغير".into()); }
    tx.execute(&format!("UPDATE {table} SET is_active=?1,updated_at=datetime('now') WHERE id=?2"), params![if active {1} else {0},id]).map_err(|e| e.to_string())?;
    let action = match (table,active) { ("customers",true)=>"customer_activated",("customers",false)=>"customer_deactivated",("suppliers",true)=>"supplier_activated",_=>"supplier_deactivated" };
    write_audit(&tx,Some(&actor),action,table,Some(&id.to_string()),&format!("name={name}"))?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(format!("تم {} {}: {name}", if active {"تفعيل"} else {"إيقاف"}, if table=="customers" {"العميل"} else {"المورد"}))
}

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

fn read_company_profile_json(conn: &Connection) -> Result<serde_json::Value,String> {
    let mut result=serde_json::Map::new();
    for key in ["company_name","company_phone","company_email","company_address","company_tax_number","company_commercial_register","company_website","company_logo_data_url","company_invoice_footer","currency_code"] {
        let value:String=conn.query_row("SELECT value FROM app_settings WHERE key=?1",[key],|r|r.get(0)).optional().map_err(|e|e.to_string())?.unwrap_or_default();
        let out_key=key.strip_prefix("company_").unwrap_or(key);
        result.insert(out_key.to_string(),serde_json::Value::String(value));
    }
    let (tax_rate_bps, prices_include_tax)=read_company_tax_settings(conn)?;
    result.insert("tax_rate".to_string(),serde_json::Value::from(tax_rate_bps as f64 / 100.0));
    result.insert("tax_rate_bps".to_string(),serde_json::Value::from(tax_rate_bps));
    result.insert("prices_include_tax".to_string(),serde_json::Value::Bool(prices_include_tax));
    Ok(serde_json::Value::Object(result))
}

fn read_company_tax_settings(conn: &Connection) -> Result<(i64,bool),String> {
    let get = |key: &str, fallback: &str| -> Result<String,String> {
        conn.query_row("SELECT value FROM app_settings WHERE key=?1",[key],|r|r.get(0))
            .optional().map(|v: Option<String>| v.unwrap_or_else(||fallback.to_string())).map_err(|e|e.to_string())
    };
    let rate: i64 = get("company_tax_rate_bps","0")?.parse().map_err(|_|"نسبة الضريبة المحفوظة غير صالحة".to_string())?;
    if !(0..=10000).contains(&rate) { return Err("نسبة الضريبة يجب أن تكون بين 0 و100%".into()); }
    let inclusive = match get("company_prices_include_tax","false")?.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => true,
        "false" | "0" | "no" | "" => false,
        _ => return Err("إعداد شمول الأسعار للضريبة غير صالح".into()),
    };
    Ok((rate,inclusive))
}

fn calculate_document_tax(subtotal_minor: i64, discount_minor: i64, rate_bps: i64, prices_include_tax: bool) -> Result<(i64,i64),String> {
    if subtotal_minor < 0 || discount_minor < 0 || discount_minor > subtotal_minor { return Err("قيمة الإجمالي أو الخصم غير صالحة".into()); }
    if !(0..=10000).contains(&rate_bps) { return Err("نسبة الضريبة غير صالحة".into()); }
    let base=(subtotal_minor-discount_minor) as i128;
    let rate=rate_bps as i128;
    let tax=if rate==0 || base==0 {0_i128} else if prices_include_tax {
        let divisor=10000_i128+rate;
        (base*rate + divisor/2)/divisor
    } else {
        (base*rate + 5000_i128)/10000_i128
    };
    let tax=i64::try_from(tax).map_err(|_|"قيمة الضريبة أكبر من الحد المسموح")?;
    let total=if prices_include_tax { base } else { base + tax as i128 };
    let total=i64::try_from(total).map_err(|_|"إجمالي المستند أكبر من الحد المسموح")?;
    Ok((tax,total))
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

#[tauri::command(rename_all="camelCase")]
fn get_company_printing_profile(state: State<'_, AppState>, app: tauri::AppHandle, document_type: String) -> Result<serde_json::Value,String> {
    let permission=match document_type.as_str(){"sale"=>"sales.view","purchase"=>"purchases.view","quote"=>"quotations.view",_=>return Err("نوع المستند غير صالح".into())};
    let _actor=require_permission(state.inner(),permission)?;
    let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
    read_company_profile_json(&conn)
}

#[tauri::command(rename_all = "camelCase")]
fn save_company_profile(app: tauri::AppHandle, state: State<'_, AppState>, name: String, phone: Option<String>, email: Option<String>, address: Option<String>, tax_number: Option<String>, commercial_register: Option<String>, website: Option<String>, logo_data_url: Option<String>, invoice_footer: Option<String>, currency_code: Option<String>, tax_rate: f64, prices_include_tax: bool) -> Result<String,String> {
    let actor=require_permission(state.inner(),"company.settings.manage")?;
    let name=name.trim().to_string();
    if name.is_empty() || name.chars().count()>180 {return Err("اسم الشركة مطلوب وبحد أقصى 180 حرفًا".into());}
    if !tax_rate.is_finite() || !(0.0..=100.0).contains(&tax_rate) { return Err("نسبة الضريبة يجب أن تكون رقمًا بين 0 و100%".into()); }
    let tax_rate_bps=(tax_rate*100.0).round() as i64;
    let tax_rate_text=tax_rate_bps.to_string();
    let prices_include_tax_text=if prices_include_tax {"true"} else {"false"};
    let phone=clean_optional_profile(phone,"الهاتف",60)?;
    let email=validate_profile_email(email)?;
    let address=clean_optional_profile(address,"العنوان",500)?;
    let tax_number=clean_optional_profile(tax_number,"الرقم الضريبي",100)?;
    let commercial_register=clean_optional_profile(commercial_register,"السجل التجاري",100)?;
    let website=clean_optional_profile(website,"الموقع الإلكتروني",250)?;
    let invoice_footer=clean_optional_profile(invoice_footer,"تذييل الفاتورة",500)?;
    let logo=validate_avatar_data_url(logo_data_url)?;
    let currency=currency_code.unwrap_or_else(||"EGP".to_string()).trim().to_uppercase();
    if !["EGP","SAR","AED","USD","EUR"].contains(&currency.as_str()) {return Err("رمز العملة غير مدعوم".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    for (key,value) in [("company_name",Some(name.as_str())),("company_phone",phone.as_deref()),("company_email",email.as_deref()),("company_address",address.as_deref()),("company_tax_number",tax_number.as_deref()),("company_commercial_register",commercial_register.as_deref()),("company_website",website.as_deref()),("company_logo_data_url",logo.as_deref()),("company_invoice_footer",invoice_footer.as_deref()),("currency_code",Some(currency.as_str())),("company_tax_rate_bps",Some(tax_rate_text.as_str())),("company_prices_include_tax",Some(prices_include_tax_text))] {
        tx.execute("INSERT INTO app_settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=datetime('now')",params![key,value.unwrap_or("")]).map_err(|e|e.to_string())?;
    }
    write_audit(&tx,Some(&actor),"company_profile_updated","company",None,&format!("name={name}; currency={currency}; tax_rate_bps={tax_rate_bps}; prices_include_tax={prices_include_tax}; logo_updated={}",logo.is_some()))?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok("تم حفظ بيانات الشركة وإعدادات الضريبة".into())
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct PaymentInvoicePick { invoice_type:String, invoice_id:i64, invoice_no:String, party_name:String, invoice_date:String, total_minor:i64, paid_minor:i64, balance_minor:i64 }

#[tauri::command(rename_all="camelCase")]
fn list_payable_invoices(app:tauri::AppHandle,state:State<'_,AppState>,invoice_type:String)->Result<Vec<PaymentInvoicePick>,String>{
    let actor=require_permission(state.inner(),"finance.view")?;
    let ctx=invoice_context(&invoice_type)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission){return Err("لا تملك صلاحية عرض فواتير هذا النوع".into());}
    let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
    let sql=format!("SELECT i.id,i.invoice_no,COALESCE(p.name,CASE WHEN i.{col} IS NULL THEN 'عميل نقدي' ELSE '' END),i.invoice_date,i.total_minor,COALESCE(i.paid_minor,0) FROM {header} i LEFT JOIN {party} p ON p.id=i.{col} WHERE i.status='posted' AND COALESCE(i.paid_minor,0)<i.total_minor ORDER BY i.id DESC LIMIT 500",header=ctx.header,party=ctx.party_table,col=ctx.party_column);
    let mut stmt=conn.prepare(&sql).map_err(|e|e.to_string())?;
    let rows=stmt.query_map([],|r|{let total:i64=r.get(4)?;let paid:i64=r.get(5)?;Ok(PaymentInvoicePick{invoice_type:invoice_type.clone(),invoice_id:r.get(0)?,invoice_no:r.get(1)?,party_name:r.get(2)?,invoice_date:r.get(3)?,total_minor:total,paid_minor:paid,balance_minor:(total-paid).max(0)})}).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct PaymentRow { id:i64, invoice_type:String, invoice_id:i64, invoice_no:String, party_name:String, direction:String, amount_minor:i64, payment_date:String, method:String, reference_no:Option<String>, notes:Option<String>, created_by_username:Option<String>, created_at:String }

#[tauri::command(rename_all="camelCase")]
fn record_payment(app:tauri::AppHandle,state:State<'_,AppState>,invoice_type:String,invoice_id:i64,amount_minor:i64,payment_date:String,method:String,reference_no:Option<String>,notes:Option<String>)->Result<String,String>{
    let actor=require_permission(state.inner(),"finance.payment.record")?;
    let ctx=invoice_context(&invoice_type)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission){return Err("لا تملك صلاحية عرض الفاتورة المحددة".into());}
    if amount_minor<=0{return Err("قيمة الدفعة يجب أن تكون أكبر من صفر".into());}
    if payment_date.len()!=10{return Err("تاريخ الدفعة يجب أن يكون بصيغة YYYY-MM-DD".into());}
    if !["cash","bank","card","transfer","other"].contains(&method.as_str()){return Err("طريقة الدفع غير صالحة".into());}
    let reference_no=clean_optional_profile(reference_no,"المرجع",120)?;let notes=clean_optional_profile(notes,"الملاحظات",1000)?;
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let normalized:Option<String>=tx.query_row("SELECT date(?1)",[payment_date.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;if normalized.as_deref()!=Some(payment_date.trim()){return Err("تاريخ الدفعة غير صالح".into());}
    let invoice:Option<(String,String,i64,i64,Option<i64>)>=tx.query_row(&format!("SELECT invoice_no,status,total_minor,COALESCE(paid_minor,0),{col} FROM {header} WHERE id=?1",header=ctx.header,col=ctx.party_column),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|e|e.to_string())?;
    let Some((invoice_no,status,total,paid,party_id))=invoice else{return Err("الفاتورة غير موجودة".into());};
    if status!="posted"{return Err("يمكن تسجيل دفعات على الفواتير المعتمدة فقط".into());}
    if amount_minor>total.saturating_sub(paid){return Err(format!("الدفعة أكبر من المتبقي على الفاتورة. المتبقي {:.2}",(total-paid).max(0) as f64/100.0));}
    let direction=if ctx.kind=="sale"{"in"}else{"out"};
    tx.execute("INSERT INTO payments(invoice_type,invoice_id,direction,amount_minor,payment_date,method,reference_no,notes,created_by,created_by_username) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![ctx.kind,invoice_id,direction,amount_minor,payment_date.trim(),method,reference_no,notes,actor.id,actor.display_name]).map_err(|e|e.to_string())?;
    let payment_id=tx.last_insert_rowid();
    tx.execute(&format!("UPDATE {header} SET paid_minor=COALESCE(paid_minor,0)+?1,updated_at=datetime('now') WHERE id=?2",header=ctx.header),params![amount_minor,invoice_id]).map_err(|e|e.to_string())?;
    if method=="cash"{
        let description=format!("{} {}",if ctx.kind=="sale"{"تحصيل فاتورة بيع"}else{"سداد فاتورة شراء"},invoice_no);
        tx.execute("INSERT INTO cashbook_entries(direction,amount_minor,entry_date,source_type,source_id,description,notes,created_by,created_by_username) VALUES (?1,?2,?3,'payment',?4,?5,?6,?7,?8)",params![direction,amount_minor,payment_date.trim(),payment_id,description,notes,actor.id,actor.display_name]).map_err(|e|e.to_string())?;
    }
    let party_name=if let Some(pid)=party_id{tx.query_row(&format!("SELECT name FROM {table} WHERE id=?1",table=ctx.party_table),[pid],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.unwrap_or_default()}else{"عميل نقدي".into()};
    write_audit(&tx,Some(&actor),"payment_recorded","payment",Some(&payment_id.to_string()),&format!("invoice_type={}; invoice_no={invoice_no}; party={party_name}; amount_minor={amount_minor}; method={method}; direction={direction}",ctx.kind))?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم تسجيل الدفعة بقيمة {:.2} على الفاتورة {invoice_no}",amount_minor as f64/100.0))
}

#[tauri::command(rename_all="camelCase")]
fn list_payments(app:tauri::AppHandle,state:State<'_,AppState>)->Result<Vec<PaymentRow>,String>{
    let actor=require_permission(state.inner(),"finance.view")?;let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
    let sql="SELECT p.id,p.invoice_type,p.invoice_id,p.direction,p.amount_minor,p.payment_date,p.method,p.reference_no,p.notes,p.created_by_username,p.created_at,CASE WHEN p.invoice_type='sale' THEN s.invoice_no ELSE pi.invoice_no END,CASE WHEN p.invoice_type='sale' THEN COALESCE(c.name,'عميل نقدي') ELSE COALESCE(v.name,'') END FROM payments p LEFT JOIN sales s ON p.invoice_type='sale' AND s.id=p.invoice_id LEFT JOIN customers c ON c.id=s.customer_id LEFT JOIN purchase_invoices pi ON p.invoice_type='purchase' AND pi.id=p.invoice_id LEFT JOIN suppliers v ON v.id=pi.supplier_id ORDER BY p.id DESC LIMIT 300";
    let mut stmt=conn.prepare(sql).map_err(|e|e.to_string())?;let rows=stmt.query_map([],|r|{let t:String=r.get(1)?;Ok(PaymentRow{id:r.get(0)?,invoice_type:t,invoice_id:r.get(2)?,direction:r.get(3)?,amount_minor:r.get(4)?,payment_date:r.get(5)?,method:r.get(6)?,reference_no:r.get(7)?,notes:r.get(8)?,created_by_username:r.get(9)?,created_at:r.get(10)?,invoice_no:r.get(11)?,party_name:r.get(12)?})}).map_err(|e|e.to_string())?;
    let result=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let _=actor;Ok(result)
}

#[tauri::command(rename_all="camelCase")]
fn cashbox_adjustment(app:tauri::AppHandle,state:State<'_,AppState>,direction:String,amount_minor:i64,entry_date:String,description:String,notes:Option<String>)->Result<String,String>{
    let actor=require_permission(state.inner(),"cashbox.adjust")?;if !["in","out"].contains(&direction.as_str()){return Err("اتجاه حركة الخزنة غير صحيح".into());}if amount_minor<=0{return Err("المبلغ يجب أن يكون أكبر من صفر".into());}let description=description.trim().to_string();if description.is_empty()||description.chars().count()>180{return Err("وصف الحركة مطلوب وبحد أقصى 180 حرفًا".into());}let notes=clean_optional_profile(notes,"الملاحظات",1000)?;
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let normalized:Option<String>=tx.query_row("SELECT date(?1)",[entry_date.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;if normalized.as_deref()!=Some(entry_date.trim()){return Err("تاريخ الحركة غير صالح".into());}if direction=="out"{let balance:i64=tx.query_row("SELECT COALESCE(SUM(CASE direction WHEN 'in' THEN amount_minor ELSE -amount_minor END),0) FROM cashbook_entries",[],|r|r.get(0)).map_err(|e|e.to_string())?;if amount_minor>balance{return Err(format!("الرصيد النقدي لا يكفي لهذه التسوية. الرصيد الحالي {:.2}",balance as f64/100.0));}}
    tx.execute("INSERT INTO cashbook_entries(direction,amount_minor,entry_date,source_type,description,notes,created_by,created_by_username) VALUES (?1,?2,?3,'adjustment',?4,?5,?6,?7)",params![direction,amount_minor,entry_date.trim(),description,notes,actor.id,actor.display_name]).map_err(|e|e.to_string())?;let id=tx.last_insert_rowid();write_audit(&tx,Some(&actor),"cashbox_adjustment","cashbox",Some(&id.to_string()),&format!("direction={direction}; amount_minor={amount_minor}; description={description}"))?;tx.commit().map_err(|e|e.to_string())?;Ok("تم تسجيل حركة الخزنة".into())
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct CashbookRow{id:i64,direction:String,amount_minor:i64,entry_date:String,source_type:String,description:String,notes:Option<String>,created_by_username:Option<String>,created_at:String}
#[tauri::command]
fn list_cashbook(app:tauri::AppHandle,state:State<'_,AppState>)->Result<serde_json::Value,String>{let _actor=require_permission(state.inner(),"cashbox.view")?;let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;let mut stmt=conn.prepare("SELECT id,direction,amount_minor,entry_date,source_type,description,notes,created_by_username,created_at FROM cashbook_entries ORDER BY id DESC LIMIT 500").map_err(|e|e.to_string())?;let rows=stmt.query_map([],|r|Ok(CashbookRow{id:r.get(0)?,direction:r.get(1)?,amount_minor:r.get(2)?,entry_date:r.get(3)?,source_type:r.get(4)?,description:r.get(5)?,notes:r.get(6)?,created_by_username:r.get(7)?,created_at:r.get(8)?})).map_err(|e|e.to_string())?;let entries=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let balance:i64=conn.query_row("SELECT COALESCE(SUM(CASE direction WHEN 'in' THEN amount_minor ELSE -amount_minor END),0) FROM cashbook_entries",[],|r|r.get(0)).map_err(|e|e.to_string())?;Ok(serde_json::json!({"balanceMinor":balance,"entries":entries}))}

#[tauri::command(rename_all="camelCase")]
fn get_financial_summary(app:tauri::AppHandle,state:State<'_,AppState>,from_date:String,to_date:String)->Result<serde_json::Value,String>{let _actor=require_permission(state.inner(),"reports.view")?;for d in [&from_date,&to_date]{let n:Option<String>={let path=database_path(&app)?;initialize_database(&path)?;let c=connect(&path)?;c.query_row("SELECT date(?1)",[d.as_str()],|r|r.get(0)).map_err(|e|e.to_string())?};if n.as_deref()!=Some(d.as_str()){return Err("صيغة التاريخ غير صحيحة".into());}}if from_date>to_date{return Err("تاريخ البداية يجب أن يسبق تاريخ النهاية".into());}
 let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
 let sales:i64=conn.query_row("SELECT COALESCE(SUM(total_minor),0) FROM sales WHERE status='posted' AND invoice_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let purchases:i64=conn.query_row("SELECT COALESCE(SUM(total_minor),0) FROM purchase_invoices WHERE status='posted' AND invoice_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let sales_tax:i64=conn.query_row("SELECT COALESCE(SUM(tax_minor),0) FROM sales WHERE status='posted' AND invoice_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let purchases_tax:i64=conn.query_row("SELECT COALESCE(SUM(tax_minor),0) FROM purchase_invoices WHERE status='posted' AND invoice_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let received:i64=conn.query_row("SELECT COALESCE(SUM(amount_minor),0) FROM payments WHERE invoice_type='sale' AND payment_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let paid:i64=conn.query_row("SELECT COALESCE(SUM(amount_minor),0) FROM payments WHERE invoice_type='purchase' AND payment_date BETWEEN ?1 AND ?2",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let outstanding_sales:i64=conn.query_row("SELECT COALESCE(SUM(total_minor-paid_minor),0) FROM sales WHERE status='posted'",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 let outstanding_purchases:i64=conn.query_row("SELECT COALESCE(SUM(total_minor-paid_minor),0) FROM purchase_invoices WHERE status='posted'",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 let gross_profit: i64=conn.query_row("SELECT COALESCE(SUM(s.total_minor - COALESCE(s.tax_minor,0) - COALESCE((SELECT SUM(CAST(si.quantity_milli * si.cost_price_minor / 1000 AS INTEGER)) FROM sale_items si WHERE si.sale_id=s.id AND si.cost_price_minor IS NOT NULL),0)),0) FROM sales s WHERE s.status='posted' AND s.invoice_date BETWEEN ?1 AND ?2 AND NOT EXISTS(SELECT 1 FROM sale_items si WHERE si.sale_id=s.id AND si.cost_price_minor IS NULL)",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let unknown_cost_sales:i64=conn.query_row("SELECT COUNT(*) FROM sales s WHERE s.status='posted' AND s.invoice_date BETWEEN ?1 AND ?2 AND EXISTS(SELECT 1 FROM sale_items si WHERE si.sale_id=s.id AND si.cost_price_minor IS NULL)",params![from_date,to_date],|r|r.get(0)).map_err(|e|e.to_string())?;
 let low_stock:i64=conn.query_row("SELECT COUNT(*) FROM products p WHERE p.is_active=1 AND p.minimum_quantity_milli>0 AND (SELECT COALESCE(SUM(sm.quantity_milli),0) FROM stock_movements sm WHERE sm.product_id=p.id)<p.minimum_quantity_milli",[],|r|r.get(0)).map_err(|e|e.to_string())?;
 Ok(serde_json::json!({"fromDate":from_date,"toDate":to_date,"salesMinor":sales,"purchasesMinor":purchases,"receivedMinor":received,"paidMinor":paid,"outstandingSalesMinor":outstanding_sales,"outstandingPurchasesMinor":outstanding_purchases,"grossProfitMinor":gross_profit,"salesTaxMinor":sales_tax,"purchasesTaxMinor":purchases_tax,"salesMissingCostSnapshot":unknown_cost_sales,"lowStockProducts":low_stock}))
}

#[derive(Serialize)]
#[serde(rename_all="camelCase")]
struct QuotationRow{id:i64,quote_no:String,customer_name:String,quote_date:String,valid_until:Option<String>,subtotal_minor:i64,discount_minor:i64,total_minor:i64,tax_rate_bps:i64,tax_minor:i64,prices_include_tax:bool,notes:Option<String>,status:String,item_count:i64,created_by_name:Option<String>,customer_id:i64,converted_sale_id:Option<i64>}
fn quotation_row(conn:&Connection,id:i64)->Result<Option<QuotationRow>,String>{conn.query_row("SELECT q.id,q.quote_no,c.name,q.quote_date,q.valid_until,q.subtotal_minor,q.discount_minor,q.total_minor,q.tax_rate_bps,q.tax_minor,q.prices_include_tax,q.notes,q.status,(SELECT COUNT(*) FROM quotation_items qi WHERE qi.quotation_id=q.id),COALESCE(q.created_by_username,u.display_name),q.customer_id,q.converted_sale_id FROM quotations q JOIN customers c ON c.id=q.customer_id LEFT JOIN users u ON u.id=q.created_by WHERE q.id=?1",[id],|r|Ok(QuotationRow{id:r.get(0)?,quote_no:r.get(1)?,customer_name:r.get(2)?,quote_date:r.get(3)?,valid_until:r.get(4)?,subtotal_minor:r.get(5)?,discount_minor:r.get(6)?,total_minor:r.get(7)?,tax_rate_bps:r.get(8)?,tax_minor:r.get(9)?,prices_include_tax:r.get::<_,i64>(10)?!=0,notes:r.get(11)?,status:r.get(12)?,item_count:r.get(13)?,created_by_name:r.get(14)?,customer_id:r.get(15)?,converted_sale_id:r.get(16)?})).optional().map_err(|e|e.to_string())}
#[tauri::command(rename_all="camelCase")]
fn list_quotations(app:tauri::AppHandle,state:State<'_,AppState>,query:Option<String>)->Result<Vec<QuotationRow>,String>{let _actor=require_permission(state.inner(),"quotations.view")?;let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;let q=query.unwrap_or_default();let like=format!("%{}%",q.trim());let mut stmt=conn.prepare("SELECT q.id,q.quote_no,c.name,q.quote_date,q.valid_until,q.subtotal_minor,q.discount_minor,q.total_minor,q.tax_rate_bps,q.tax_minor,q.prices_include_tax,q.notes,q.status,(SELECT COUNT(*) FROM quotation_items qi WHERE qi.quotation_id=q.id),COALESCE(q.created_by_username,u.display_name),q.customer_id,q.converted_sale_id FROM quotations q JOIN customers c ON c.id=q.customer_id LEFT JOIN users u ON u.id=q.created_by WHERE (?1='' OR q.quote_no LIKE ?2 OR c.name LIKE ?2 OR COALESCE(q.notes,'') LIKE ?2) ORDER BY q.id DESC LIMIT 500").map_err(|e|e.to_string())?;let rows=stmt.query_map(params![q.trim(),like],|r|Ok(QuotationRow{id:r.get(0)?,quote_no:r.get(1)?,customer_name:r.get(2)?,quote_date:r.get(3)?,valid_until:r.get(4)?,subtotal_minor:r.get(5)?,discount_minor:r.get(6)?,total_minor:r.get(7)?,tax_rate_bps:r.get(8)?,tax_minor:r.get(9)?,prices_include_tax:r.get::<_,i64>(10)?!=0,notes:r.get(11)?,status:r.get(12)?,item_count:r.get(13)?,created_by_name:r.get(14)?,customer_id:r.get(15)?,converted_sale_id:r.get(16)?})).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}
#[tauri::command(rename_all="camelCase")]
fn get_quotation_details(app:tauri::AppHandle,state:State<'_,AppState>,quotation_id:i64)->Result<serde_json::Value,String>{let _actor=require_permission(state.inner(),"quotations.view")?;let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;let quote=quotation_row(&conn,quotation_id)?.ok_or("عرض السعر غير موجود")?;let mut stmt=conn.prepare("SELECT qi.id,qi.product_id,p.code,p.name,COALESCE(qi.unit,p.unit,'قطعة'),qi.quantity_milli,qi.unit_price_minor,qi.line_total_minor FROM quotation_items qi JOIN products p ON p.id=qi.product_id WHERE qi.quotation_id=?1 ORDER BY qi.id").map_err(|e|e.to_string())?;let rows=stmt.query_map([quotation_id],|r|Ok(InvoiceItemRow{id:r.get(0)?,product_id:r.get(1)?,product_code:r.get(2)?,product_name:r.get(3)?,unit:r.get(4)?,quantity_milli:r.get(5)?,unit_price_minor:r.get(6)?,line_total_minor:r.get(7)?})).map_err(|e|e.to_string())?;let items=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;Ok(serde_json::json!({"quotation":quote,"items":items}))}
#[tauri::command(rename_all="camelCase")]
fn save_quotation(app:tauri::AppHandle,state:State<'_,AppState>,quotation_id:Option<i64>,customer_id:i64,quote_date:String,valid_until:Option<String>,discount_minor:i64,notes:Option<String>,items:Vec<InvoiceItemInput>)->Result<QuotationRow,String>{let permission=if quotation_id.is_some(){"quotations.edit"}else{"quotations.create"};let actor=require_permission(state.inner(),permission)?;if !actor.permissions.iter().any(|p|p=="quotations.view")||!actor.permissions.iter().any(|p|p=="customers.view")||!actor.permissions.iter().any(|p|p=="products.view"){return Err("تحتاج صلاحيات عرض عروض الأسعار والعملاء والأصناف".into());}if items.is_empty()||items.len()>200{return Err("عرض السعر يجب أن يحتوي من صنف واحد إلى 200 صنف".into());}if discount_minor<0{return Err("الخصم لا يمكن أن يكون سالبًا".into());}let notes=clean_optional_profile(notes,"الملاحظات",2000)?;let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let norm:Option<String>=tx.query_row("SELECT date(?1)",[quote_date.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;if norm.as_deref()!=Some(quote_date.trim()){return Err("تاريخ العرض غير صحيح".into());}let valid=if let Some(v)=valid_until.as_deref().filter(|v|!v.trim().is_empty()){let d:Option<String>=tx.query_row("SELECT date(?1)",[v.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;if d.as_deref()!=Some(v.trim()){return Err("تاريخ صلاحية العرض غير صحيح".into());}Some(v.trim().to_string())}else{None};let customer_ok:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM customers WHERE id=?1 AND is_active=1)",[customer_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;if !customer_ok{return Err("العميل غير موجود أو موقوف".into());}
 let id=if let Some(id)=quotation_id{let status:Option<String>=tx.query_row("SELECT status FROM quotations WHERE id=?1",[id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;if status.as_deref()!=Some("draft"){return Err("يمكن تعديل مسودات عروض الأسعار فقط".into());}tx.execute("UPDATE quotations SET customer_id=?1,quote_date=?2,valid_until=?3,discount_minor=?4,notes=?5,updated_at=datetime('now') WHERE id=?6",params![customer_id,quote_date.trim(),valid,discount_minor,notes,id]).map_err(|e|e.to_string())?;tx.execute("DELETE FROM quotation_items WHERE quotation_id=?1",[id]).map_err(|e|e.to_string())?;id}else{tx.execute("INSERT INTO quotations(quote_no,customer_id,quote_date,valid_until,discount_minor,notes,created_by,created_by_username) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![format!("TMP-Q-{}",SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos()),customer_id,quote_date.trim(),valid,discount_minor,notes,actor.id,actor.display_name]).map_err(|e|e.to_string())?;tx.last_insert_rowid()};if quotation_id.is_none(){tx.execute("UPDATE quotations SET quote_no=?1 WHERE id=?2",params![format!("QUO-{id:06}"),id]).map_err(|e|e.to_string())?;}
 let mut subtotal=0_i64;for item in &items{if item.quantity_milli<=0||item.unit_price_minor<0{return Err("راجع كمية وسعر كل صنف".into());}let p:Option<(String,String,i64)>=tx.query_row("SELECT unit,name,is_active FROM products WHERE id=?1",[item.product_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;let Some((unit,name,active))=p else{return Err("صنف غير موجود".into());};if active==0{return Err(format!("الصنف {name} موقوف"));}let line=item.quantity_milli.checked_mul(item.unit_price_minor).and_then(|v|v.checked_add(500)).ok_or("قيمة السطر أكبر من الحد")?/1000;subtotal=subtotal.checked_add(line).ok_or("إجمالي العرض أكبر من الحد")?;tx.execute("INSERT INTO quotation_items(quotation_id,product_id,unit,quantity_milli,unit_price_minor,line_total_minor) VALUES (?1,?2,?3,?4,?5,?6)",params![id,item.product_id,unit,item.quantity_milli,item.unit_price_minor,line]).map_err(|e|e.to_string())?;}if discount_minor>subtotal{return Err("الخصم أكبر من إجمالي العرض".into());}let (tax_rate_bps,prices_include_tax)=read_company_tax_settings(&tx)?;let (tax_minor,total)=calculate_document_tax(subtotal,discount_minor,tax_rate_bps,prices_include_tax)?;tx.execute("UPDATE quotations SET subtotal_minor=?1,total_minor=?2,tax_rate_bps=?3,tax_minor=?4,prices_include_tax=?5,updated_at=datetime('now') WHERE id=?6",params![subtotal,total,tax_rate_bps,tax_minor,if prices_include_tax {1} else {0},id]).map_err(|e|e.to_string())?;write_audit(&tx,Some(&actor),if quotation_id.is_some(){"quotation_updated"}else{"quotation_created"},"quotation",Some(&id.to_string()),&format!("total_minor={total}; tax_minor={tax_minor}; tax_rate_bps={tax_rate_bps}"))?;tx.commit().map_err(|e|e.to_string())?;quotation_row(&conn,id)?.ok_or("تعذر تحميل عرض السعر بعد الحفظ".into())}
#[tauri::command(rename_all="camelCase")]
fn set_quotation_status(app:tauri::AppHandle,state:State<'_,AppState>,quotation_id:i64,status:String)->Result<String,String>{let actor=require_permission(state.inner(),"quotations.status.manage")?;if !["sent","accepted","rejected","cancelled"].contains(&status.as_str()){return Err("حالة عرض السعر غير مسموحة".into());}let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let old:Option<(String,String)>=tx.query_row("SELECT quote_no,status FROM quotations WHERE id=?1",[quotation_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;let Some((number,current))=old else{return Err("عرض السعر غير موجود".into());};let allowed=match current.as_str(){"draft"=>["sent","accepted","rejected","cancelled"].contains(&status.as_str()),"sent"=>["accepted","rejected","cancelled"].contains(&status.as_str()),"accepted"=>["cancelled"].contains(&status.as_str()),_=>false};if !allowed{return Err("لا يمكن الانتقال من الحالة الحالية إلى الحالة المطلوبة".into());}tx.execute("UPDATE quotations SET status=?1,updated_at=datetime('now') WHERE id=?2",params![status,quotation_id]).map_err(|e|e.to_string())?;write_audit(&tx,Some(&actor),"quotation_status_changed","quotation",Some(&quotation_id.to_string()),&format!("quote_no={number}; from={current}; to={status}"))?;tx.commit().map_err(|e|e.to_string())?;Ok(format!("تم تحديث حالة عرض السعر {number}"))}
#[tauri::command(rename_all="camelCase")]
fn convert_quotation_to_sale(app:tauri::AppHandle,state:State<'_,AppState>,quotation_id:i64)->Result<String,String>{let actor=require_permission(state.inner(),"quotations.convert")?;if !actor.permissions.iter().any(|p|p=="sales.create")||!actor.permissions.iter().any(|p|p=="sales.view"){return Err("تحتاج صلاحيات إنشاء وعرض فواتير المبيعات".into());}let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let quote:Option<(String,String,i64,i64,i64,i64,i64,i64)>=tx.query_row("SELECT quote_no,status,customer_id,subtotal_minor,discount_minor,tax_rate_bps,tax_minor,prices_include_tax FROM quotations WHERE id=?1",[quotation_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional().map_err(|e|e.to_string())?;let Some((quote_no,status,customer_id,subtotal,discount,tax_rate_bps,tax_minor,prices_include_tax))=quote else{return Err("عرض السعر غير موجود".into());};if status!="accepted"{return Err("يجب قبول عرض السعر قبل تحويله إلى فاتورة بيع".into());}let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM customers WHERE id=?1 AND is_active=1)",[customer_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;if !exists{return Err("العميل موقوف؛ أعد تفعيله أولًا".into());}tx.execute("INSERT INTO sales(invoice_no,customer_id,invoice_date,subtotal_minor,discount_minor,total_minor,tax_rate_bps,tax_minor,prices_include_tax,notes,status,created_by,created_by_username) VALUES (?1,?2,date('now'),?3,?4,?5,?6,?7,?8,?9,'draft',?10,?11)",params![format!("TMP-S-{}",SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos()),customer_id,subtotal,discount,if prices_include_tax!=0 {subtotal-discount} else {subtotal-discount+tax_minor},tax_rate_bps,tax_minor,prices_include_tax,format!("Converted from quotation {quote_no}"),actor.id,actor.display_name]).map_err(|e|e.to_string())?;let sale_id=tx.last_insert_rowid();let sale_no=format!("SAL-{sale_id:06}");tx.execute("UPDATE sales SET invoice_no=?1 WHERE id=?2",params![sale_no,sale_id]).map_err(|e|e.to_string())?;let items={let mut stmt=tx.prepare("SELECT product_id,unit,quantity_milli,unit_price_minor,line_total_minor FROM quotation_items WHERE quotation_id=?1 ORDER BY id").map_err(|e|e.to_string())?;let rows=stmt.query_map([quotation_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};if items.is_empty(){return Err("عرض السعر لا يحتوي أصنافًا".into());}for (product_id,unit,qty,price,line) in items{let cost:i64=tx.query_row("SELECT purchase_price_minor FROM products WHERE id=?1",[product_id],|r|r.get(0)).map_err(|e|e.to_string())?;tx.execute("INSERT INTO sale_items(sale_id,product_id,unit,quantity_milli,unit_price_minor,discount_minor,line_total_minor,cost_price_minor) VALUES (?1,?2,?3,?4,?5,0,?6,?7)",params![sale_id,product_id,unit,qty,price,line,cost]).map_err(|e|e.to_string())?;}tx.execute("UPDATE quotations SET status='converted',converted_sale_id=?1,updated_at=datetime('now') WHERE id=?2",params![sale_id,quotation_id]).map_err(|e|e.to_string())?;write_audit(&tx,Some(&actor),"quotation_converted","quotation",Some(&quotation_id.to_string()),&format!("quote_no={quote_no}; sale_invoice={sale_no}"))?;tx.commit().map_err(|e|e.to_string())?;Ok(format!("تم تحويل عرض السعر {quote_no} إلى مسودة فاتورة البيع {sale_no}"))}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceItemInput { product_id: i64, quantity_milli: i64, unit_price_minor: i64 }

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct InvoiceRow {
    id: i64, invoice_no: String, party_name: String, invoice_date: String,
    subtotal_minor: i64, discount_minor: i64, total_minor: i64, tax_rate_bps: i64, tax_minor: i64, prices_include_tax: bool, paid_minor: i64,
    notes: Option<String>, status: String, item_count: i64, created_by_name: Option<String>, created_at: String, party_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvoiceItemRow {
    id: i64, product_id: i64, product_code: String, product_name: String, unit: String,
    quantity_milli: i64, unit_price_minor: i64, line_total_minor: i64,
}

#[derive(Clone, Copy)]
struct InvoiceContext { kind: &'static str, header: &'static str, lines: &'static str, party_table: &'static str, party_column: &'static str, prefix: &'static str, view_permission: &'static str, create_permission: &'static str, post_permission: &'static str, cancel_permission: &'static str }
fn invoice_context(invoice_type: &str) -> Result<InvoiceContext,String> {
    match invoice_type {
        "purchase" => Ok(InvoiceContext { kind:"purchase",header:"purchase_invoices",lines:"purchase_invoice_items",party_table:"suppliers",party_column:"supplier_id",prefix:"PUR",view_permission:"purchases.view",create_permission:"purchases.create",post_permission:"purchases.post",cancel_permission:"purchases.cancel" }),
        "sale" => Ok(InvoiceContext { kind:"sale",header:"sales",lines:"sale_items",party_table:"customers",party_column:"customer_id",prefix:"SAL",view_permission:"sales.view",create_permission:"sales.create",post_permission:"sales.post",cancel_permission:"sales.cancel" }),
        _ => Err("نوع الفاتورة غير صالح".into())
    }
}
fn invoice_row_from_query(conn: &Connection, ctx: InvoiceContext, invoice_id: i64) -> Result<Option<InvoiceRow>,String> {
    let sql = format!("SELECT i.id,i.invoice_no,COALESCE(p.name,CASE WHEN i.{party_col} IS NULL THEN 'عميل نقدي' ELSE '' END),i.invoice_date,i.subtotal_minor,i.discount_minor,i.total_minor,i.tax_rate_bps,i.tax_minor,i.prices_include_tax,COALESCE(i.paid_minor,0),i.notes,i.status,(SELECT COUNT(*) FROM {lines} il WHERE il.{fk}=i.id),COALESCE(i.created_by_username,u.display_name),i.created_at,i.{party_col} FROM {header} i LEFT JOIN {party} p ON p.id=i.{party_col} LEFT JOIN users u ON u.id=i.created_by WHERE i.id=?1",header=ctx.header,party=ctx.party_table,party_col=ctx.party_column,lines=ctx.lines,fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"});
    conn.query_row(&sql,[invoice_id],|r|Ok(InvoiceRow{id:r.get(0)?,invoice_no:r.get(1)?,party_name:r.get(2)?,invoice_date:r.get(3)?,subtotal_minor:r.get(4)?,discount_minor:r.get(5)?,total_minor:r.get(6)?,tax_rate_bps:r.get(7)?,tax_minor:r.get(8)?,prices_include_tax:r.get::<_,i64>(9)?!=0,paid_minor:r.get(10)?,notes:r.get(11)?,status:r.get(12)?,item_count:r.get(13)?,created_by_name:r.get(14)?,created_at:r.get(15)?,party_id:r.get(16)?})).optional().map_err(|e|e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn list_invoices(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, query: Option<String>) -> Result<Vec<InvoiceRow>,String> {
    let ctx = invoice_context(&invoice_type)?; let _actor = require_permission(state.inner(),ctx.view_permission)?;
    let path = database_path(&app)?; initialize_database(&path)?; let conn = connect(&path)?;
    let q=query.unwrap_or_default(); let q=q.trim(); let like=format!("%{q}%");
    let fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"};
    let sql=format!("SELECT i.id,i.invoice_no,COALESCE(p.name,CASE WHEN i.{party_col} IS NULL THEN 'عميل نقدي' ELSE '' END),i.invoice_date,i.subtotal_minor,i.discount_minor,i.total_minor,i.tax_rate_bps,i.tax_minor,i.prices_include_tax,COALESCE(i.paid_minor,0),i.notes,i.status,(SELECT COUNT(*) FROM {lines} il WHERE il.{fk}=i.id),COALESCE(i.created_by_username,u.display_name),i.created_at,i.{party_col} FROM {header} i LEFT JOIN {party} p ON p.id=i.{party_col} LEFT JOIN users u ON u.id=i.created_by WHERE (?1='' OR i.invoice_no LIKE ?2 OR COALESCE(p.name,'') LIKE ?2 OR COALESCE(i.notes,'') LIKE ?2) ORDER BY i.id DESC LIMIT 500",party_col=ctx.party_column,lines=ctx.lines,fk=fk,header=ctx.header,party=ctx.party_table);
    let mut stmt=conn.prepare(&sql).map_err(|e|e.to_string())?;
    let rows=stmt.query_map(params![q,like],|r|Ok(InvoiceRow{id:r.get(0)?,invoice_no:r.get(1)?,party_name:r.get(2)?,invoice_date:r.get(3)?,subtotal_minor:r.get(4)?,discount_minor:r.get(5)?,total_minor:r.get(6)?,tax_rate_bps:r.get(7)?,tax_minor:r.get(8)?,prices_include_tax:r.get::<_,i64>(9)?!=0,paid_minor:r.get(10)?,notes:r.get(11)?,status:r.get(12)?,item_count:r.get(13)?,created_by_name:r.get(14)?,created_at:r.get(15)?,party_id:r.get(16)?})).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command(rename_all = "camelCase")]
fn get_invoice_details(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, invoice_id: i64) -> Result<serde_json::Value,String> {
    let ctx=invoice_context(&invoice_type)?;let _actor=require_permission(state.inner(),ctx.view_permission)?;
    let path=database_path(&app)?;initialize_database(&path)?;let conn=connect(&path)?;
    let invoice=invoice_row_from_query(&conn,ctx,invoice_id)?.ok_or("الفاتورة غير موجودة")?;
    let fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"};
    let sql=format!("SELECT il.id,il.product_id,p.code,p.name,COALESCE(il.unit,p.unit,'قطعة'),il.quantity_milli,il.unit_price_minor,il.line_total_minor FROM {lines} il JOIN products p ON p.id=il.product_id WHERE il.{fk}=?1 ORDER BY il.id",lines=ctx.lines,fk=fk);
    let mut stmt=conn.prepare(&sql).map_err(|e|e.to_string())?;
    let rows=stmt.query_map([invoice_id],|r|Ok(InvoiceItemRow{id:r.get(0)?,product_id:r.get(1)?,product_code:r.get(2)?,product_name:r.get(3)?,unit:r.get(4)?,quantity_milli:r.get(5)?,unit_price_minor:r.get(6)?,line_total_minor:r.get(7)?})).map_err(|e|e.to_string())?;
    let items=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    Ok(serde_json::json!({"invoice":invoice,"items":items}))
}

#[tauri::command(rename_all = "camelCase")]
fn create_invoice(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, party_id: Option<i64>, invoice_date: String, discount_minor: i64, notes: Option<String>, items: Vec<InvoiceItemInput>) -> Result<InvoiceRow,String> {
    let ctx=invoice_context(&invoice_type)?;let actor=require_permission(state.inner(),ctx.create_permission)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission) || !actor.permissions.iter().any(|p|p=="products.view") { return Err("إنشاء الفاتورة يتطلب صلاحية عرض الفواتير والأصناف".into()); }
    if ctx.kind=="purchase" && !actor.permissions.iter().any(|p|p=="suppliers.view") { return Err("تحتاج صلاحية عرض الموردين لإنشاء فاتورة مشتريات".into()); }
    if ctx.kind=="sale" && !actor.permissions.iter().any(|p|p=="customers.view") { return Err("تحتاج صلاحية عرض العملاء لإنشاء فاتورة مبيعات".into()); }
    if items.is_empty() {return Err("أضف صنفًا واحدًا على الأقل للفاتورة".into());}
    if items.len()>200 {return Err("لا يمكن أن تتجاوز الفاتورة 200 سطر".into());}
    if discount_minor<0 {return Err("الخصم لا يمكن أن يكون سالبًا".into());}
    if invoice_date.len()!=10 {return Err("تاريخ الفاتورة يجب أن يكون بصيغة YYYY-MM-DD".into());}
    let notes=clean_optional_profile(notes,"الملاحظات",2000)?;
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let normalized_date:Option<String>=tx.query_row("SELECT date(?1)",[invoice_date.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;
    if normalized_date.as_deref()!=Some(invoice_date.trim()) {return Err("تاريخ الفاتورة غير صالح".into());}
    if ctx.kind=="purchase" {
        let supplier_id=party_id.ok_or("اختَر المورد لفاتورة المشتريات")?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM suppliers WHERE id=?1 AND is_active=1)",[supplier_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if !exists {return Err("المورد غير موجود أو موقوف".into());}
    } else if let Some(customer_id)=party_id {
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM customers WHERE id=?1 AND is_active=1)",[customer_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if !exists {return Err("العميل غير موجود أو موقوف".into());}
    }
    let temp_no=format!("TMP-{}-{}-{}",ctx.prefix,actor.id,SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos());
    let insert_header=format!("INSERT INTO {header}(invoice_no,{party_column},invoice_date,subtotal_minor,discount_minor,total_minor,notes,status,created_by,created_by_username) VALUES (?1,?2,?3,0,?4,0,?5,'draft',?6,?7)",header=ctx.header,party_column=ctx.party_column);
    tx.execute(&insert_header,params![temp_no,party_id,invoice_date.trim(),discount_minor,notes,actor.id,actor.display_name]).map_err(|e|e.to_string())?;
    let id=tx.last_insert_rowid();let invoice_no=format!("{}-{:06}",ctx.prefix,id);
    tx.execute(&format!("UPDATE {header} SET invoice_no=?1 WHERE id=?2",header=ctx.header),params![invoice_no,id]).map_err(|e|e.to_string())?;
    let mut subtotal=0_i64;
    for item in &items {
        if item.quantity_milli<=0 {return Err("كل كمية يجب أن تكون أكبر من صفر".into());}
        if item.unit_price_minor<0 {return Err("سعر السطر لا يمكن أن يكون سالبًا".into());}
        let product:Option<(String,String,i64,i64,i64)>=tx.query_row("SELECT code,name,purchase_price_minor,sale_price_minor,is_active FROM products WHERE id=?1",[item.product_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional().map_err(|e|e.to_string())?;
        let Some((_code,_name,_purchase_price,sale_price,is_active))=product else {return Err("أحد الأصناف في الفاتورة غير موجود".into());};
        if is_active==0 {return Err("لا يمكن إدراج صنف موقوف في فاتورة جديدة".into());}
        if ctx.kind=="sale" && item.unit_price_minor!=sale_price && !actor.permissions.iter().any(|p|p=="sales.price.override") {
            return Err(format!("لا تملك صلاحية تجاوز سعر البيع المسجل للصنف {}",_code));
        }
        let numerator=item.quantity_milli.checked_mul(item.unit_price_minor).and_then(|v|v.checked_add(500)).ok_or("قيمة السطر أكبر من الحد المسموح")?;
        let line_total=numerator/1000;
        subtotal=subtotal.checked_add(line_total).ok_or("إجمالي الفاتورة أكبر من الحد المسموح")?;
        let unit_price=item.unit_price_minor;
        if ctx.kind == "sale" {
            tx.execute("INSERT INTO sale_items(sale_id,product_id,unit,quantity_milli,unit_price_minor,discount_minor,line_total_minor,cost_price_minor) VALUES (?1,?2,(SELECT unit FROM products WHERE id=?2),?3,?4,0,?5,?6)",params![id,item.product_id,item.quantity_milli,unit_price,line_total,_purchase_price]).map_err(|e|e.to_string())?;
        } else {
            tx.execute("INSERT INTO purchase_invoice_items(purchase_invoice_id,product_id,unit,quantity_milli,unit_price_minor,discount_minor,line_total_minor) VALUES (?1,?2,(SELECT unit FROM products WHERE id=?2),?3,?4,0,?5)",params![id,item.product_id,item.quantity_milli,unit_price,line_total]).map_err(|e|e.to_string())?;
        }
    }
    if discount_minor>subtotal {return Err("خصم الفاتورة لا يمكن أن يتجاوز الإجمالي".into());}
    let (tax_rate_bps,prices_include_tax)=read_company_tax_settings(&tx)?;
    let (tax_minor,total)=calculate_document_tax(subtotal,discount_minor,tax_rate_bps,prices_include_tax)?;
    tx.execute(&format!("UPDATE {header} SET subtotal_minor=?1,total_minor=?2,tax_rate_bps=?3,tax_minor=?4,prices_include_tax=?5,updated_at=datetime('now') WHERE id=?6",header=ctx.header),params![subtotal,total,tax_rate_bps,tax_minor,if prices_include_tax {1} else {0},id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_created"} else {"sale_invoice_created"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&id.to_string()),&format!("invoice_no={invoice_no}; status=draft; items={}; total_minor={total}; tax_minor={tax_minor}; tax_rate_bps={tax_rate_bps}",items.len()))?;
    tx.commit().map_err(|e|e.to_string())?;
    invoice_row_from_query(&conn,ctx,id)?.ok_or("تم إنشاء الفاتورة لكن تعذر تحميلها".into())
}

#[tauri::command(rename_all = "camelCase")]
fn update_invoice_draft(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, invoice_id: i64, party_id: Option<i64>, invoice_date: String, discount_minor: i64, notes: Option<String>, items: Vec<InvoiceItemInput>) -> Result<InvoiceRow,String> {
    let ctx=invoice_context(&invoice_type)?;
    let edit_permission=if ctx.kind=="purchase" {"purchases.edit_draft"} else {"sales.edit_draft"};
    let actor=require_permission(state.inner(),edit_permission)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission) || !actor.permissions.iter().any(|p|p==ctx.create_permission) {return Err("تعديل المسودة يتطلب صلاحية عرض الفواتير وإنشائها".into());}
    if !actor.permissions.iter().any(|p|p=="products.view") {return Err("تعديل الفاتورة يتطلب صلاحية عرض الأصناف".into());}
    if ctx.kind=="purchase" && !actor.permissions.iter().any(|p|p=="suppliers.view") {return Err("تحتاج صلاحية عرض الموردين لتعديل فاتورة مشتريات".into());}
    if ctx.kind=="sale" && !actor.permissions.iter().any(|p|p=="customers.view") {return Err("تحتاج صلاحية عرض العملاء لتعديل فاتورة مبيعات".into());}
    if items.is_empty() || items.len()>200 {return Err("الفاتورة يجب أن تحتوي من صنف واحد إلى 200 صنف".into());}
    if discount_minor<0 {return Err("الخصم لا يمكن أن يكون سالبًا".into());}
    if invoice_date.len()!=10 {return Err("تاريخ الفاتورة يجب أن يكون بصيغة YYYY-MM-DD".into());}
    let notes=clean_optional_profile(notes,"الملاحظات",2000)?;
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let invoice:Option<(String,String)>=tx.query_row(&format!("SELECT invoice_no,status FROM {header} WHERE id=?1",header=ctx.header),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(|e|e.to_string())?;
    let Some((invoice_no,status))=invoice else{return Err("الفاتورة غير موجودة".into());};
    if status!="draft" {return Err("يمكن تعديل المسودات فقط؛ الفاتورة المعتمدة أو الملغاة لا تتغير".into());}
    let normalized_date:Option<String>=tx.query_row("SELECT date(?1)",[invoice_date.trim()],|r|r.get(0)).map_err(|e|e.to_string())?;
    if normalized_date.as_deref()!=Some(invoice_date.trim()) {return Err("تاريخ الفاتورة غير صالح".into());}
    if ctx.kind=="purchase" {
        let supplier_id=party_id.ok_or("اختَر المورد لفاتورة المشتريات")?;
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM suppliers WHERE id=?1 AND is_active=1)",[supplier_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if !exists {return Err("المورد غير موجود أو موقوف".into());}
    } else if let Some(customer_id)=party_id {
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM customers WHERE id=?1 AND is_active=1)",[customer_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if !exists {return Err("العميل غير موجود أو موقوف".into());}
    }
    let fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"};
    tx.execute(&format!("DELETE FROM {lines} WHERE {fk}=?1",lines=ctx.lines,fk=fk),[invoice_id]).map_err(|e|e.to_string())?;
    let mut subtotal=0_i64;
    for item in &items {
        if item.quantity_milli<=0 || item.unit_price_minor<0 {return Err("راجع الكميات والأسعار في الفاتورة".into());}
        let product:Option<(String,i64,i64,i64)>=tx.query_row("SELECT code,purchase_price_minor,sale_price_minor,is_active FROM products WHERE id=?1",[item.product_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
        let Some((code,_purchase_price,sale_price,is_active))=product else{return Err("أحد الأصناف في الفاتورة غير موجود".into());};
        if is_active==0 {return Err(format!("الصنف {code} موقوف؛ لا يمكن حفظ فاتورة جديدة به"));}
        if ctx.kind=="sale" && item.unit_price_minor!=sale_price && !actor.permissions.iter().any(|p|p=="sales.price.override") {return Err(format!("لا تملك صلاحية تجاوز سعر البيع المسجل للصنف {code}"));}
        let line_total=item.quantity_milli.checked_mul(item.unit_price_minor).and_then(|v|v.checked_add(500)).ok_or("قيمة السطر أكبر من الحد المسموح")?/1000;
        subtotal=subtotal.checked_add(line_total).ok_or("إجمالي الفاتورة أكبر من الحد المسموح")?;
        if ctx.kind == "sale" {
            tx.execute("INSERT INTO sale_items(sale_id,product_id,unit,quantity_milli,unit_price_minor,discount_minor,line_total_minor,cost_price_minor) VALUES (?1,?2,(SELECT unit FROM products WHERE id=?2),?3,?4,0,?5,?6)",params![invoice_id,item.product_id,item.quantity_milli,item.unit_price_minor,line_total,_purchase_price]).map_err(|e|e.to_string())?;
        } else {
            tx.execute("INSERT INTO purchase_invoice_items(purchase_invoice_id,product_id,unit,quantity_milli,unit_price_minor,discount_minor,line_total_minor) VALUES (?1,?2,(SELECT unit FROM products WHERE id=?2),?3,?4,0,?5)",params![invoice_id,item.product_id,item.quantity_milli,item.unit_price_minor,line_total]).map_err(|e|e.to_string())?;
        }
    }
    if discount_minor>subtotal {return Err("خصم الفاتورة لا يمكن أن يتجاوز الإجمالي".into());}
    let (tax_rate_bps,prices_include_tax)=read_company_tax_settings(&tx)?;
    let (tax_minor,total)=calculate_document_tax(subtotal,discount_minor,tax_rate_bps,prices_include_tax)?;
    let update=format!("UPDATE {header} SET {party_col}=?1,invoice_date=?2,subtotal_minor=?3,discount_minor=?4,total_minor=?5,tax_rate_bps=?6,tax_minor=?7,prices_include_tax=?8,notes=?9,updated_at=datetime('now') WHERE id=?10",header=ctx.header,party_col=ctx.party_column);
    tx.execute(&update,params![party_id,invoice_date.trim(),subtotal,discount_minor,total,tax_rate_bps,tax_minor,if prices_include_tax {1} else {0},notes,invoice_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_updated"} else {"sale_invoice_updated"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&invoice_id.to_string()),&format!("invoice_no={invoice_no}; draft revised; items={}; total_minor={}; tax_minor={}; tax_rate_bps={tax_rate_bps}",items.len(),total,tax_minor))?;
    tx.commit().map_err(|e|e.to_string())?;
    invoice_row_from_query(&conn,ctx,invoice_id)?.ok_or("تعذر تحميل المسودة المعدلة".into())
}

#[tauri::command(rename_all = "camelCase")]
fn post_invoice(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, invoice_id: i64) -> Result<String,String> {
    let ctx=invoice_context(&invoice_type)?;let actor=require_permission(state.inner(),ctx.post_permission)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission) { return Err("لازم تملك صلاحية عرض الفواتير قبل اعتمادها".into()); }
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let invoice:Option<(String,String,Option<i64>)>=tx.query_row(&format!("SELECT invoice_no,status,{party_column} FROM {header} WHERE id=?1",header=ctx.header,party_column=ctx.party_column),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((invoice_no,status,party_id))=invoice else{return Err("الفاتورة غير موجودة".into());};
    if status!="draft" {return Err("يمكن اعتماد المسودات فقط؛ حالة الفاتورة الحالية لا تسمح بالاعتماد".into());}
    if let Some(party_id)=party_id {
        let active:bool=tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {party_table} WHERE id=?1 AND is_active=1)",party_table=ctx.party_table),[party_id],|r|Ok(r.get::<_,i64>(0)?!=0)).map_err(|e|e.to_string())?;
        if !active {return Err(if ctx.kind=="purchase" {"المورد المرتبط بالفاتورة موقوف؛ أعد تفعيله قبل الاعتماد".into()} else {"العميل المرتبط بالفاتورة موقوف؛ أعد تفعيله قبل الاعتماد".into()});}
    } else if ctx.kind=="purchase" { return Err("فاتورة المشتريات تحتاج إلى مورد".into()); }
    let warehouse_id:i64=tx.query_row("SELECT id FROM warehouses WHERE code='MAIN' AND is_active=1 LIMIT 1",[],|r|r.get(0)).map_err(|e|format!("لا يوجد مخزن رئيسي نشط: {e}"))?;
    let fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"};
    let lines_sql=format!("SELECT il.product_id,il.quantity_milli,COALESCE(il.unit,p.unit,'قطعة'),p.code,p.name,p.is_active FROM {lines} il JOIN products p ON p.id=il.product_id WHERE il.{fk}=?1 ORDER BY il.id",lines=ctx.lines,fk=fk);
    let line_rows={let mut stmt=tx.prepare(&lines_sql).map_err(|e|e.to_string())?;let rows=stmt.query_map([invoice_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,i64>(5)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};
    if line_rows.is_empty(){return Err("لا يمكن اعتماد فاتورة بدون أصناف".into());}
    for (product_id,quantity,unit,code,name,is_active) in line_rows {
        if is_active==0 {return Err(format!("الصنف {code} ({name}) موقوف؛ أعد تفعيله قبل اعتماد الفاتورة"));}
        let signed=if ctx.kind=="purchase" {quantity} else {-quantity};
        if ctx.kind=="sale" {
            let balance:i64=tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1 AND warehouse_id=?2",params![product_id,warehouse_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if balance<quantity {return Err(format!("الرصيد غير كافٍ للصنف {code} ({name}). المتاح {} والمطلوب {}",balance as f64/1000.0,quantity as f64/1000.0));}
        }
        tx.execute("INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,reference_id,notes,unit) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![product_id,warehouse_id,ctx.kind,signed,if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},invoice_id,format!("{} — {}",invoice_no,if ctx.kind=="purchase" {"اعتماد فاتورة مشتريات"} else {"اعتماد فاتورة مبيعات"}),unit]).map_err(|e|e.to_string())?;
        write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"stock_in"} else {"stock_out"},"stock",Some(&product_id.to_string()),&format!("source=invoice; invoice_no={invoice_no}; code={code}; name={name}; quantity_milli={quantity}; unit={unit}"))?;
    }
    tx.execute(&format!("UPDATE {header} SET status='posted',posted_at=datetime('now'),updated_at=datetime('now') WHERE id=?1",header=ctx.header),[invoice_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_posted"} else {"sale_invoice_posted"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&invoice_id.to_string()),&format!("invoice_no={invoice_no}; stock updated atomically"))?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم اعتماد الفاتورة {invoice_no} وتحديث المخزون بنجاح"))
}

#[tauri::command(rename_all = "camelCase")]
fn cancel_invoice(app: tauri::AppHandle, state: State<'_, AppState>, invoice_type: String, invoice_id: i64) -> Result<String,String> {
    let ctx=invoice_context(&invoice_type)?;let actor=require_permission(state.inner(),ctx.cancel_permission)?;
    if !actor.permissions.iter().any(|p|p==ctx.view_permission) { return Err("لازم تملك صلاحية عرض الفواتير قبل إلغائها".into()); }
    let path=database_path(&app)?;initialize_database(&path)?;let mut conn=connect(&path)?;
    let tx=conn.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
    let invoice:Option<(String,String,i64)>=tx.query_row(&format!("SELECT invoice_no,status,COALESCE(paid_minor,0) FROM {header} WHERE id=?1",header=ctx.header),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;
    let Some((invoice_no,status,paid_minor))=invoice else{return Err("الفاتورة غير موجودة".into());};
    if status=="cancelled" {return Err("الفاتورة ملغاة بالفعل".into());}
    if paid_minor > 0 {return Err("لا يمكن إلغاء فاتورة عليها دفعات مسجلة. عالج رد المبلغ أو تسوية الدفعة أولًا حتى لا يختل رصيد الحساب والخزنة.".into());}
    if status=="posted" {
        let warehouse_id:i64=tx.query_row("SELECT id FROM warehouses WHERE code='MAIN' LIMIT 1",[],|r|r.get(0)).map_err(|e|e.to_string())?;
        let fk=if ctx.kind=="purchase" {"purchase_invoice_id"} else {"sale_id"};
        let sql=format!("SELECT il.product_id,il.quantity_milli,COALESCE(il.unit,p.unit,'قطعة'),p.code,p.name FROM {lines} il JOIN products p ON p.id=il.product_id WHERE il.{fk}=?1 ORDER BY il.id",lines=ctx.lines,fk=fk);
        let items={let mut stmt=tx.prepare(&sql).map_err(|e|e.to_string())?;let rows=stmt.query_map([invoice_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};
        for (product_id,quantity,unit,code,name) in items {
            let signed=if ctx.kind=="purchase" {-quantity} else {quantity};
            if ctx.kind=="purchase" {
                let balance:i64=tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1 AND warehouse_id=?2",params![product_id,warehouse_id],|r|r.get(0)).map_err(|e|e.to_string())?;
                if balance<quantity {return Err(format!("لا يمكن إلغاء فاتورة الشراء؛ الرصيد الحالي للصنف {code} ({name}) لا يكفي لعكس الكمية. عالج المرتجعات أو الحركات أولًا."));}
            }
            tx.execute("INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,reference_id,notes,unit) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![product_id,warehouse_id,if ctx.kind=="purchase" {"purchase_return"} else {"sale_return"},signed,if ctx.kind=="purchase" {"purchase_invoice_cancel"} else {"sale_invoice_cancel"},invoice_id,format!("عكس أثر الفاتورة {invoice_no}"),unit]).map_err(|e|e.to_string())?;
            write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"stock_out"} else {"stock_in"},"stock",Some(&product_id.to_string()),&format!("source=invoice_cancel; invoice_no={invoice_no}; code={code}; name={name}; quantity_milli={quantity}; unit={unit}"))?;
        }
    }
    tx.execute(&format!("UPDATE {header} SET status='cancelled',cancelled_at=datetime('now'),updated_at=datetime('now') WHERE id=?1",header=ctx.header),[invoice_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_cancelled"} else {"sale_invoice_cancelled"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&invoice_id.to_string()),&format!("invoice_no={invoice_no}; previous_status={status}; stock_effect_reversed={}",status=="posted"))?;
    tx.commit().map_err(|e|e.to_string())?;
    Ok(format!("تم إلغاء الفاتورة {invoice_no}. تم الحفاظ على سجلها{}",if status=="posted" {" وعكس أثرها على المخزون بحركات موثقة"} else {" كمسودة دون حركة مخزون"}))
}

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

#[derive(Serialize)]
struct RestorePoint {
    path: String,
    file_name: String,
    modified_at_secs: u64,
    size_bytes: u64,
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

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let path = database_path(&app.handle()).map_err(std::io::Error::other)?;
            initialize_database(&path).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![app_info, auth_status, create_initial_admin, login, logout, list_users, create_user, update_user_profile, set_user_active, set_user_role, list_user_permissions, set_user_permissions, reset_user_password, change_own_password, list_audit_log, create_product, find_product_by_barcode, get_dashboard_inventory_counts, list_products, search_products, list_inactive_products, search_inactive_products, reactivate_product, update_product, add_alternate_barcode, deactivate_product, list_stock, list_stock_movements, record_stock_movement, list_parties, save_party, set_party_active, get_company_branding, get_company_profile, get_company_printing_profile, save_company_profile, list_payable_invoices, record_payment, list_payments, cashbox_adjustment, list_cashbook, get_financial_summary, list_quotations, get_quotation_details, save_quotation, set_quotation_status, convert_quotation_to_sale, list_invoices, get_invoice_details, create_invoice, update_invoice_draft, post_invoice, cancel_invoice, create_backup, list_restore_points, delete_restore_point, restore_backup])
        .run(tauri::generate_context!())
        .expect("خطأ أثناء تشغيل مدار");
}
