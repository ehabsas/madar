use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::{Mutex, mpsc::{self, Receiver, Sender}}, thread, time::{SystemTime, UNIX_EPOCH}};
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
        if !table_has_column(&conn, table, "tax_minor")? {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN tax_minor INTEGER NOT NULL DEFAULT 0 CHECK (tax_minor >= 0)"), []).map_err(|e| e.to_string())?;
        }
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
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sales_returns (
            id INTEGER PRIMARY KEY, return_no TEXT NOT NULL UNIQUE, sale_id INTEGER NOT NULL REFERENCES sales(id),
            return_date TEXT NOT NULL, total_minor INTEGER NOT NULL DEFAULT 0 CHECK(total_minor>=0),
            notes TEXT, created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS sales_return_items (
            id INTEGER PRIMARY KEY, return_id INTEGER NOT NULL REFERENCES sales_returns(id) ON DELETE CASCADE,
            sale_item_id INTEGER NOT NULL REFERENCES sale_items(id), product_id INTEGER NOT NULL REFERENCES products(id),
            quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0), unit_price_minor INTEGER NOT NULL CHECK(unit_price_minor>=0),
            line_total_minor INTEGER NOT NULL CHECK(line_total_minor>=0)
        );
        CREATE TABLE IF NOT EXISTS purchase_returns (
            id INTEGER PRIMARY KEY, return_no TEXT NOT NULL UNIQUE, purchase_invoice_id INTEGER NOT NULL REFERENCES purchase_invoices(id),
            return_date TEXT NOT NULL, total_minor INTEGER NOT NULL DEFAULT 0 CHECK(total_minor>=0),
            notes TEXT, created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS purchase_return_items (
            id INTEGER PRIMARY KEY, return_id INTEGER NOT NULL REFERENCES purchase_returns(id) ON DELETE CASCADE,
            purchase_item_id INTEGER NOT NULL REFERENCES purchase_invoice_items(id), product_id INTEGER NOT NULL REFERENCES products(id),
            quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0), unit_price_minor INTEGER NOT NULL CHECK(unit_price_minor>=0),
            line_total_minor INTEGER NOT NULL CHECK(line_total_minor>=0)
        );
        CREATE TABLE IF NOT EXISTS product_batches (
            id INTEGER PRIMARY KEY, product_id INTEGER NOT NULL REFERENCES products(id), warehouse_id INTEGER NOT NULL REFERENCES warehouses(id),
            batch_no TEXT NOT NULL, received_date TEXT NOT NULL, expiry_date TEXT NOT NULL, expiry_known INTEGER NOT NULL DEFAULT 1 CHECK(expiry_known IN (0,1)), quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0),
            remaining_milli INTEGER NOT NULL CHECK(remaining_milli>=0), source_invoice_id INTEGER, notes TEXT,
            created_at TEXT NOT NULL DEFAULT(datetime('now')), UNIQUE(product_id,warehouse_id,batch_no,expiry_date)
        );
        CREATE INDEX IF NOT EXISTS idx_batches_expiry ON product_batches(expiry_date,remaining_milli);
        CREATE TABLE IF NOT EXISTS cash_shifts (
            id INTEGER PRIMARY KEY, opened_at TEXT NOT NULL DEFAULT(datetime('now')), closed_at TEXT,
            opening_minor INTEGER NOT NULL CHECK(opening_minor>=0), expected_minor INTEGER, actual_minor INTEGER, difference_minor INTEGER,
            opened_by INTEGER, opened_by_username TEXT, closed_by INTEGER, closed_by_username TEXT, notes TEXT, status TEXT NOT NULL DEFAULT 'open' CHECK(status IN ('open','closed'))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_one_open_cash_shift ON cash_shifts(status) WHERE status='open';
        CREATE TABLE IF NOT EXISTS gl_accounts (
            id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL, account_type TEXT NOT NULL CHECK(account_type IN ('asset','liability','equity','revenue','expense')), is_active INTEGER NOT NULL DEFAULT 1
        );
        CREATE TABLE IF NOT EXISTS gl_journals (
            id INTEGER PRIMARY KEY, entry_no TEXT NOT NULL UNIQUE, entry_date TEXT NOT NULL, description TEXT NOT NULL,
            source_type TEXT, source_id INTEGER, posted INTEGER NOT NULL DEFAULT 1, created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS gl_journal_lines (
            id INTEGER PRIMARY KEY, journal_id INTEGER NOT NULL REFERENCES gl_journals(id) ON DELETE CASCADE,
            account_id INTEGER NOT NULL REFERENCES gl_accounts(id), debit_minor INTEGER NOT NULL DEFAULT 0 CHECK(debit_minor>=0),
            credit_minor INTEGER NOT NULL DEFAULT 0 CHECK(credit_minor>=0), CHECK((debit_minor=0) != (credit_minor=0))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_gl_journal_source_once ON gl_journals(source_type,source_id) WHERE source_type IS NOT NULL AND source_id IS NOT NULL;
        INSERT OR IGNORE INTO gl_accounts(code,name,account_type) VALUES
          ('1000','النقدية والخزنة','asset'),('1100','العملاء','asset'),('1200','المخزون','asset'),
          ('1010','الحسابات البنكية','asset'),('1020','تحت التحصيل - بطاقات','asset'),('1030','وسائل دفع أخرى قيد التسوية','asset'),('1300','ضريبة قيمة مضافة مدخلات','asset'),('2000','الموردون','liability'),('2100','ضريبة قيمة مضافة مخرجات','liability'),('3000','رأس المال','equity'),('4000','إيرادات المبيعات','revenue'),('5000','تكلفة البضاعة المباعة','expense'),('6000','المصروفات التشغيلية','expense');"
    ).map_err(|e| e.to_string())?;

    if !table_has_column(&conn, "product_batches", "expiry_known")? {
        conn.execute("ALTER TABLE product_batches ADD COLUMN expiry_known INTEGER NOT NULL DEFAULT 1 CHECK(expiry_known IN (0,1))", []).map_err(|e| e.to_string())?;
        // Existing sentinel-dated batches created by older builds are explicitly marked as unknown.
        conn.execute("UPDATE product_batches SET expiry_known=0 WHERE expiry_date='9999-12-31' AND notes LIKE '%تاريخ الصلاحية غير مدخل%'", []).map_err(|e| e.to_string())?;
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
        ("products.barcode.manage", "إدارة الباركودات", "إضافة باركودات بديلة وتغيير الباركود الأساسي للصنف"),
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
    conn.execute(
        "UPDATE permissions SET description=?1 WHERE code='products.barcode.manage'",
        ["إضافة باركودات بديلة وتغيير الباركود الأساسي للصنف"],
    ).map_err(|e| e.to_string())?;
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
    let warehouse_detailed = ["products.view","stock.view","stock.movement.record","suppliers.view","purchases.view"];
    let sales_detailed = ["products.view","stock.view","customers.view","customers.create","sales.view","sales.create","sales.edit_draft","sales.post","quotations.view","quotations.create","quotations.edit","quotations.status.manage","quotations.convert","finance.view","finance.payment.record","cashbox.view","reports.view"];
    let viewer_detailed = ["products.view","stock.view"];
    let detailed_matrices: [(&str, &[&str]); 5] = [("admin", &admin_detailed), ("manager", &manager_detailed), ("warehouse", &warehouse_detailed), ("sales", &sales_detailed), ("viewer", &viewer_detailed)];
    for (role, items) in detailed_matrices {
        let role_id: i64 = conn.query_row("SELECT id FROM roles WHERE code=?1", [role], |r| r.get(0)).map_err(|e| e.to_string())?;
        for item in items {
            conn.execute("INSERT OR IGNORE INTO role_permissions(role_id,permission_code) VALUES (?1,?2)", params![role_id,item]).map_err(|e| e.to_string())?;
        }
    }
    // Company-profile viewing is a default privilege only for managers and system administrators.
    // Explicit per-user overrides remain available through the detailed permission editor.
    conn.execute(
        "DELETE FROM role_permissions WHERE permission_code='company.settings.view' AND role_id IN (SELECT id FROM roles WHERE code NOT IN ('admin','manager'))",
        [],
    ).map_err(|e| e.to_string())?;
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
    email: Option<String>,
    phone: Option<String>,
    address: Option<String>,
    permissions: Vec<String>,
}

#[derive(Default)]
struct AppState { session: Mutex<Option<UserSession>> }
enum ClipboardRequest {
    ReadText(Sender<Result<String, String>>),
    WriteText(String, Sender<Result<(), String>>),
}

struct ClipboardWorkerState { sender: Sender<ClipboardRequest> }

fn start_clipboard_worker() -> Result<Sender<ClipboardRequest>, String> {
    let (sender, receiver): (Sender<ClipboardRequest>, Receiver<ClipboardRequest>) = mpsc::channel();
    thread::Builder::new().name("madar-clipboard".to_string()).spawn(move || {
        // Keep one clipboard instance alive because X11/Wayland clipboard ownership may
        // disappear if an instance is immediately dropped after writing.
        let mut clipboard: Option<arboard::Clipboard> = None;
        while let Ok(request) = receiver.recv() {
            if clipboard.is_none() {
                match arboard::Clipboard::new() {
                    Ok(instance) => clipboard = Some(instance),
                    Err(error) => {
                        let message = format!("تعذر الوصول إلى حافظة النظام: {error}");
                        match request {
                            ClipboardRequest::ReadText(reply) => { let _ = reply.send(Err(message)); },
                            ClipboardRequest::WriteText(_, reply) => { let _ = reply.send(Err(message)); },
                        }
                        continue;
                    }
                }
            }
            match request {
                ClipboardRequest::ReadText(reply) => {
                    // Empty/non-text clipboard is a normal state, not an application error.
                    let result = match clipboard.as_mut().expect("clipboard initialized").get_text() {
                        Ok(text) => Ok(text),
                        Err(_) => Ok(String::new()),
                    };
                    let _ = reply.send(result);
                }
                ClipboardRequest::WriteText(text, reply) => {
                    let result = clipboard.as_mut().expect("clipboard initialized").set_text(text)
                        .map_err(|error| format!("تعذر الكتابة إلى حافظة النظام: {error}"));
                    let _ = reply.send(result);
                }
            }
        }
    }).map_err(|error| format!("تعذر بدء خدمة الحافظة: {error}"))?;
    Ok(sender)
}





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
    let (username, display_name, role_code, role_name, avatar_data_url, email, phone, address): (String,String,String,String,Option<String>,Option<String>,Option<String>,Option<String>) = conn.query_row(
        "SELECT u.username,u.display_name,r.code,r.name,u.avatar_data_url,u.email,u.phone,u.address FROM users u JOIN roles r ON r.id=u.role_id WHERE u.id=?1 AND u.is_active=1",
        [user_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?)),
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
    Ok(UserSession { id:user_id, username, display_name, role_code, role_name, avatar_data_url, email, phone, address, permissions })
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



























#[tauri::command(rename_all = "camelCase")]
fn list_audit_log(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    query: Option<String>,
    entity_type: Option<String>,
    action: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<AuditRow>, String> {
    let _actor = require_permission(state.inner(), "audit.view")?;
    let path = database_path(&app)?;
    initialize_database(&path)?;
    let conn = connect(&path)?;
    let q = query.unwrap_or_default();
    let normalized_query = q.trim();
    let like = format!("%{}%", normalized_query);
    let kind = entity_type.unwrap_or_default().trim().to_ascii_lowercase();
    let act = action.unwrap_or_default().trim().to_ascii_lowercase();
    let page_size = limit.unwrap_or(50).clamp(1, 200);
    let page_offset = offset.unwrap_or(0).max(0);
    // Filtering is performed in SQLite BEFORE LIMIT/OFFSET, so search spans the
    // entire audit history, not just the records already rendered in the UI.
    let mut stmt = conn.prepare(
        "SELECT id,action,entity_type,entity_id,details,actor_username,created_at
         FROM audit_log
         WHERE (?1='' OR action LIKE ?2 OR entity_type LIKE ?2 OR COALESCE(entity_id,'') LIKE ?2 OR COALESCE(details,'') LIKE ?2 OR COALESCE(actor_username,'') LIKE ?2)
           AND (?3='' OR (?3='stock' AND (lower(trim(entity_type))='stock' OR lower(trim(action)) IN ('stock_in','stock_out'))) OR (?3='product' AND lower(trim(entity_type))='product' AND lower(trim(action)) NOT IN ('stock_in','stock_out')) OR (?3 NOT IN ('stock','product') AND lower(trim(entity_type))=?3))
           AND (?4='' OR lower(trim(action))=?4)
         ORDER BY id DESC LIMIT ?5 OFFSET ?6"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![normalized_query, like, kind, act, page_size, page_offset], |r| {
        Ok(AuditRow {
            id: r.get(0)?, action: r.get(1)?, entity_type: r.get(2)?,
            entity_id: r.get(3)?, details: r.get(4)?, actor_username: r.get(5)?, created_at: r.get(6)?,
        })
    }).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct ProductRow {
    id: i64, code: String, name: String, unit: String, barcode: String,
    purchase_price_minor: i64, sale_price_minor: i64, minimum_quantity_milli: i64,
}

fn validate_barcode(value: &str) -> Result<String, String> {
    let barcode = value.trim();
    if barcode.is_empty() {
        return Err("الباركود لا يمكن أن يكون فارغًا".into());
    }
    if barcode.chars().count() > 128 {
        return Err("الباركود أطول من الحد المسموح (128 حرفًا)".into());
    }
    if barcode.chars().any(char::is_control) {
        return Err("الباركود يحتوي على محارف تحكم غير مسموح بها. الصق قيمة الباركود فقط دون أسطر جديدة.".into());
    }
    Ok(barcode.to_string())
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







fn search_products_db(conn: &Connection, query: &str, is_active: bool) -> Result<Vec<ProductRow>, String> {
    let q = query.trim();
    let like = format!("%{}%", q);
    let mut stmt = conn.prepare(
        "SELECT p.id,p.code,p.name,p.unit,COALESCE((SELECT b.barcode FROM product_barcodes b WHERE b.product_id=p.id AND b.is_primary=1 LIMIT 1),''),p.purchase_price_minor,p.sale_price_minor,p.minimum_quantity_milli
         FROM products p WHERE p.is_active=?1 AND (?2='' OR p.name LIKE ?3 OR p.code LIKE ?3 OR EXISTS(SELECT 1 FROM product_barcodes bx WHERE bx.product_id=p.id AND bx.barcode LIKE ?3))
         ORDER BY p.name COLLATE NOCASE LIMIT 500"
    ).map_err(|e| e.to_string())?;
    let rows = stmt.query_map(params![if is_active { 1 } else { 0 }, q, like], |r| Ok(ProductRow { id:r.get(0)?, code:r.get(1)?, name:r.get(2)?, unit:r.get(3)?, barcode:r.get(4)?, purchase_price_minor:r.get(5)?, sale_price_minor:r.get(6)?, minimum_quantity_milli:r.get(7)? })).map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())
}



















#[derive(Serialize)]
struct StockRow {
    product_id: i64,
    code: String,
    name: String,
    unit: String,
    barcode: String,
    quantity_milli: i64,
    minimum_quantity_milli: i64,
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

fn allocate_proportionally(total: i64, weights: &[i64]) -> Result<Vec<i64>, String> {
    if total < 0 || weights.iter().any(|w| *w < 0) { return Err("قيم التوزيع غير صالحة".into()); }
    let sum: i128 = weights.iter().map(|w| *w as i128).sum();
    if weights.is_empty() { return if total == 0 { Ok(Vec::new()) } else { Err("لا توجد بنود لتوزيع المبلغ".into()) }; }
    if sum == 0 { if total == 0 { return Ok(vec![0; weights.len()]); } return Err("لا يمكن توزيع مبلغ على بنود قيمتها صفر".into()); }
    let mut out = Vec::with_capacity(weights.len());
    let mut assigned = 0_i64;
    let mut last_positive = 0usize;
    for (i, w) in weights.iter().enumerate() { if *w > 0 { last_positive = i; } }
    for (i, w) in weights.iter().enumerate() {
        if i == last_positive { out.push(total - assigned); }
        else {
            let share = ((total as i128) * (*w as i128) / sum).clamp(0, i64::MAX as i128) as i64;
            assigned = assigned.checked_add(share).ok_or("مبلغ التوزيع أكبر من الحد")?;
            out.push(share);
        }
    }
    Ok(out)
}

fn persist_line_allocations(tx: &rusqlite::Transaction<'_>, ctx: InvoiceContext, invoice_id: i64, discount_minor: i64, tax_minor: i64) -> Result<(), String> {
    let fk = if ctx.kind == "sale" { "sale_id" } else { "purchase_invoice_id" };
    let mut stmt = tx.prepare(&format!("SELECT id,line_total_minor FROM {lines} WHERE {fk}=?1 ORDER BY id", lines=ctx.lines, fk=fk)).map_err(|e| e.to_string())?;
    let rows = stmt.query_map([invoice_id], |r| Ok((r.get::<_,i64>(0)?, r.get::<_,i64>(1)?))).map_err(|e| e.to_string())?;
    let rows = rows.collect::<Result<Vec<_>,_>>().map_err(|e| e.to_string())?;
    drop(stmt);
    let weights: Vec<i64> = rows.iter().map(|(_,v)| *v).collect();
    let discounts = allocate_proportionally(discount_minor, &weights)?;
    let net_weights: Vec<i64> = weights.iter().zip(discounts.iter()).map(|(w,d)| w.saturating_sub(*d)).collect();
    let taxes = allocate_proportionally(tax_minor, &net_weights)?;
    for (i, (line_id, _)) in rows.iter().enumerate() {
        tx.execute(&format!("UPDATE {lines} SET discount_minor=?1,tax_minor=?2 WHERE id=?3", lines=ctx.lines), params![discounts[i], taxes[i], line_id]).map_err(|e| e.to_string())?;
    }
    Ok(())
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
    // Payment method determines the actual settlement account; receivable/payable is cleared separately.
    let settlement_account=match method.as_str(){"cash"=>"1000","card"=>"1020","bank"|"transfer"=>"1010","other"=>"1030",_=>return Err("طريقة الدفع غير مدعومة".into())};
    let journal_lines=if ctx.kind=="sale" { vec![(settlement_account,amount_minor,0),("1100",0,amount_minor)] } else { vec![("2000",amount_minor,0),(settlement_account,0,amount_minor)] };
    post_automatic_journal(&tx,"payment",payment_id,payment_date.trim(),&format!("دفعة على الفاتورة {invoice_no} - {method}"),&journal_lines,Some(actor.id),Some(&actor.username))?;
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
 let low_stock:i64=conn.query_row("SELECT COUNT(*) FROM products p WHERE p.is_active=1 AND p.minimum_quantity_milli>0 AND (SELECT COALESCE(SUM(sm.quantity_milli),0) FROM stock_movements sm WHERE sm.product_id=p.id)<=p.minimum_quantity_milli",[],|r|r.get(0)).map_err(|e|e.to_string())?;
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
 let mut subtotal=0_i64;for item in &items{if item.quantity_milli<=0||item.unit_price_minor<0{return Err("راجع كمية وسعر كل صنف".into());}let p:Option<(String,String,i64)>=tx.query_row("SELECT unit,name,is_active FROM products WHERE id=?1",[item.product_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(|e|e.to_string())?;let Some((unit,name,active))=p else{return Err("صنف غير موجود".into());};if active==0{return Err(format!("الصنف {name} موقوف"));}let line=item.quantity_milli.checked_mul(item.unit_price_minor).and_then(|v|v.checked_add(500)).ok_or("قيمة السطر أكبر من الحد")?/1000;subtotal=subtotal.checked_add(line).ok_or("إجمالي العرض أكبر من الحد")?;tx.execute("INSERT INTO quotation_items(quotation_id,product_id,unit,quantity_milli,unit_price_minor,line_total_minor) VALUES (?1,?2,?3,?4,?5,?6)",params![id,item.product_id,unit,item.quantity_milli,item.unit_price_minor,line]).map_err(|e|e.to_string())?;}if discount_minor>subtotal{return Err("الخصم أكبر من إجمالي العرض".into());}let (tax_rate_bps,prices_include_tax)=read_company_tax_settings(&tx)?;let (tax_minor,total)=calculate_document_tax(subtotal,discount_minor,tax_rate_bps,prices_include_tax)?;
tx.execute("UPDATE quotations SET subtotal_minor=?1,total_minor=?2,tax_rate_bps=?3,tax_minor=?4,prices_include_tax=?5,updated_at=datetime('now') WHERE id=?6",params![subtotal,total,tax_rate_bps,tax_minor,if prices_include_tax {1} else {0},id]).map_err(|e|e.to_string())?;write_audit(&tx,Some(&actor),if quotation_id.is_some(){"quotation_updated"}else{"quotation_created"},"quotation",Some(&id.to_string()),&format!("total_minor={total}; tax_minor={tax_minor}; tax_rate_bps={tax_rate_bps}"))?;tx.commit().map_err(|e|e.to_string())?;quotation_row(&conn,id)?.ok_or("تعذر تحميل عرض السعر بعد الحفظ".into())}
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
    persist_line_allocations(&tx,ctx,id,discount_minor,tax_minor)?;
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
    persist_line_allocations(&tx,ctx,invoice_id,discount_minor,tax_minor)?;
    let update=format!("UPDATE {header} SET {party_col}=?1,invoice_date=?2,subtotal_minor=?3,discount_minor=?4,total_minor=?5,tax_rate_bps=?6,tax_minor=?7,prices_include_tax=?8,notes=?9,updated_at=datetime('now') WHERE id=?10",header=ctx.header,party_col=ctx.party_column);
    tx.execute(&update,params![party_id,invoice_date.trim(),subtotal,discount_minor,total,tax_rate_bps,tax_minor,if prices_include_tax {1} else {0},notes,invoice_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_updated"} else {"sale_invoice_updated"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&invoice_id.to_string()),&format!("invoice_no={invoice_no}; draft revised; items={}; total_minor={}; tax_minor={}; tax_rate_bps={tax_rate_bps}",items.len(),total,tax_minor))?;
    tx.commit().map_err(|e|e.to_string())?;
    invoice_row_from_query(&conn,ctx,invoice_id)?.ok_or("تعذر تحميل المسودة المعدلة".into())
}

fn invoice_date_for_batch(tx: &rusqlite::Transaction<'_>, header: &str, invoice_id: i64) -> Result<String,String> {
    tx.query_row(&format!("SELECT invoice_date FROM {header} WHERE id=?1"),[invoice_id],|r|r.get(0)).map_err(|e|e.to_string())
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
        if ctx.kind=="purchase" {
            // Maintain a moving weighted-average unit cost in products.purchase_price_minor.
            let old_qty:i64=tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1",[product_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            let old_cost:i64=tx.query_row("SELECT purchase_price_minor FROM products WHERE id=?1",[product_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            let line_cost:i64=tx.query_row("SELECT CAST(SUM(quantity_milli * unit_price_minor) / NULLIF(SUM(quantity_milli),0) AS INTEGER) FROM purchase_invoice_items WHERE purchase_invoice_id=?1 AND product_id=?2",params![invoice_id,product_id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if old_qty>0 && old_qty+quantity>0 {
                let weighted=(((old_qty as i128)*(old_cost as i128)+(quantity as i128)*(line_cost as i128))/(old_qty+quantity) as i128).clamp(0,i64::MAX as i128) as i64;
                tx.execute("UPDATE products SET purchase_price_minor=?1,updated_at=datetime('now') WHERE id=?2",params![weighted,product_id]).map_err(|e|e.to_string())?;
            } else if old_qty<=0 { tx.execute("UPDATE products SET purchase_price_minor=?1,updated_at=datetime('now') WHERE id=?2",params![line_cost,product_id]).map_err(|e|e.to_string())?; }
            let batch_no=format!("{}-{}",invoice_no,product_id);
            tx.execute("INSERT OR IGNORE INTO product_batches(product_id,warehouse_id,batch_no,received_date,expiry_date,expiry_known,quantity_milli,remaining_milli,source_invoice_id,notes) VALUES(?1,?2,?3,?4,'9999-12-31',0,?5,?5,?6,'تشغيلة مولدة تلقائيًا من فاتورة الشراء؛ تاريخ الصلاحية غير مدخل')",params![product_id,warehouse_id,batch_no,invoice_date_for_batch(&tx,ctx.header,invoice_id)?,quantity,invoice_id]).map_err(|e|e.to_string())?;
        } else {
            // Consume known batches by FEFO. Legacy/unbatched stock remains usable; only known batch balances are decremented.
            let batches={let mut st=tx.prepare("SELECT id,remaining_milli FROM product_batches WHERE product_id=?1 AND warehouse_id=?2 AND remaining_milli>0 ORDER BY expiry_date,received_date,id").map_err(|e|e.to_string())?;let rows=st.query_map(params![product_id,warehouse_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};
            let mut left=quantity;for (bid,available) in batches {if left<=0{break;}let take=left.min(available);tx.execute("UPDATE product_batches SET remaining_milli=remaining_milli-?1 WHERE id=?2",params![take,bid]).map_err(|e|e.to_string())?;left-=take;}
        }
        tx.execute("INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,reference_id,notes,unit) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",params![product_id,warehouse_id,ctx.kind,signed,if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},invoice_id,format!("{} — {}",invoice_no,if ctx.kind=="purchase" {"اعتماد فاتورة مشتريات"} else {"اعتماد فاتورة مبيعات"}),unit]).map_err(|e|e.to_string())?;
        write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"stock_in"} else {"stock_out"},"stock",Some(&product_id.to_string()),&format!("source=invoice; invoice_no={invoice_no}; code={code}; name={name}; quantity_milli={quantity}; unit={unit}"))?;
    }
    // Post the invoice and inventory cost atomically with the stock movements.
    let (invoice_date,invoice_total,invoice_subtotal,invoice_discount,invoice_tax,prices_include_tax):(String,i64,i64,i64,i64,i64)=tx.query_row(&format!("SELECT invoice_date,total_minor,subtotal_minor,discount_minor,tax_minor,prices_include_tax FROM {header} WHERE id=?1",header=ctx.header),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(|e|e.to_string())?;
    // Split tax from the taxable base so VAT is not posted as sales revenue/inventory.
    let net_base=(invoice_subtotal-invoice_discount).max(0);
    let taxable_base=if prices_include_tax!=0 { net_base.saturating_sub(invoice_tax) } else { net_base };
    if ctx.kind=="purchase" {
        let mut lines=vec![("1200",taxable_base,0),("1300",invoice_tax,0),("2000",0,invoice_total)];
        lines.retain(|(_,d,c)|*d>0||*c>0);
        post_automatic_journal(&tx,"purchase_invoice",invoice_id,&invoice_date,&format!("فاتورة مشتريات {invoice_no}"),&lines,Some(actor.id),Some(&actor.username))?;
    } else {
        let sale_cost:i64={
            let mut q=tx.prepare("SELECT id,product_id,quantity_milli FROM sale_items WHERE sale_id=?1 ORDER BY id").map_err(|e|e.to_string())?;
            let rows=q.query_map([invoice_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,i64>(2)?))).map_err(|e|e.to_string())?;
            let items=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?; drop(q);
            let mut total=0i64;
            for (item_id,pid,qty) in items { let unit_cost:i64=tx.query_row("SELECT purchase_price_minor FROM products WHERE id=?1",[pid],|r|r.get(0)).map_err(|e|e.to_string())?; let cost=((qty as i128)*(unit_cost as i128)/1000).clamp(0,i64::MAX as i128) as i64; tx.execute("UPDATE sale_items SET cost_price_minor=?1 WHERE id=?2",params![unit_cost,item_id]).map_err(|e|e.to_string())?; total=total.saturating_add(cost); }
            total
        };
        let mut lines=vec![("1100",invoice_total,0),("4000",0,taxable_base),("2100",0,invoice_tax)];
        lines.retain(|(_,d,c)|*d>0||*c>0);
        post_automatic_journal(&tx,"sale_invoice",invoice_id,&invoice_date,&format!("فاتورة مبيعات {invoice_no}"),&lines,Some(actor.id),Some(&actor.username))?;
        if sale_cost>0 { post_automatic_journal(&tx,"sale_cogs",invoice_id,&invoice_date,&format!("تكلفة بضاعة مباعة {invoice_no}"),&[("5000",sale_cost,0),("1200",0,sale_cost)],Some(actor.id),Some(&actor.username))?; }
    }
    tx.execute(&format!("UPDATE {header} SET status='posted',posted_at=datetime('now'),updated_at=datetime('now') WHERE id=?1",header=ctx.header),[invoice_id]).map_err(|e|e.to_string())?;
    write_audit(&tx,Some(&actor),if ctx.kind=="purchase" {"purchase_invoice_posted"} else {"sale_invoice_posted"},if ctx.kind=="purchase" {"purchase_invoice"} else {"sale_invoice"},Some(&invoice_id.to_string()),&format!("invoice_no={invoice_no}; stock and GL updated atomically"))?;
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



#[derive(Serialize)]
struct RestorePoint {
    path: String,
    file_name: String,
    modified_at_secs: u64,
    size_bytes: u64,
}








// Command handlers are kept in focused files; include! preserves the existing command API.
include!("auth_commands.rs");
include!("products_commands.rs");
include!("inventory_commands.rs");
include!("finance_commands.rs");
include!("backup_commands.rs");
include!("system_commands.rs");

// Every automatic posting is idempotent by (source_type, source_id), and is inserted atomically.
fn post_automatic_journal(conn: &Connection, source_type: &str, source_id: i64, entry_date: &str, description: &str, lines: &[(&str, i64, i64)], actor_id: Option<i64>, actor_name: Option<&str>) -> Result<(), String> {
    if conn.query_row("SELECT EXISTS(SELECT 1 FROM gl_journals WHERE source_type=?1 AND source_id=?2)", params![source_type,source_id], |r| r.get::<_,i64>(0)).map_err(|e|e.to_string())? != 0 { return Ok(()); }
    let debit: i128=lines.iter().map(|x|x.1 as i128).sum();
    let credit: i128=lines.iter().map(|x|x.2 as i128).sum();
    if debit<=0 || debit!=credit || lines.iter().any(|x|x.1<0 || x.2<0 || ((x.1==0)==(x.2==0))) { return Err(format!("القيد الآلي غير متوازن للمصدر {source_type}:{source_id}")); }
    let no=format!("AUTO-{source_type}-{source_id}");
    conn.execute("INSERT INTO gl_journals(entry_no,entry_date,description,source_type,source_id,posted,created_by,created_by_username) VALUES(?1,?2,?3,?4,?5,1,?6,?7)",params![no,entry_date,description,source_type,source_id,actor_id,actor_name]).map_err(|e|e.to_string())?;
    let jid=conn.last_insert_rowid();
    for (code,d,c) in lines {
        let aid:i64=conn.query_row("SELECT id FROM gl_accounts WHERE code=?1 AND is_active=1",[code],|r|r.get(0)).map_err(|e|format!("الحساب المحاسبي {code} غير موجود: {e}"))?;
        conn.execute("INSERT INTO gl_journal_lines(journal_id,account_id,debit_minor,credit_minor) VALUES(?1,?2,?3,?4)",params![jid,aid,d,c]).map_err(|e|e.to_string())?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase")]
struct ReturnLineInput { source_item_id:i64, quantity_milli:i64, warehouse_id:Option<i64>, batch_id:Option<i64> }

#[tauri::command(rename_all="camelCase")]
fn list_returnable_invoices(app:tauri::AppHandle,state:State<'_,AppState>,invoice_type:String)->Result<Vec<serde_json::Value>,String>{
 let ctx=invoice_context(&invoice_type)?;let _=require_permission(state.inner(),ctx.view_permission)?;let path=database_path(&app)?;initialize_database(&path)?;let c=connect(&path)?;
 let sql=if ctx.kind=="sale" {"SELECT id,invoice_no,invoice_date,total_minor FROM sales WHERE status='posted' ORDER BY id DESC LIMIT 300"}else{"SELECT id,invoice_no,invoice_date,total_minor FROM purchase_invoices WHERE status='posted' ORDER BY id DESC LIMIT 300"};
 let mut st=c.prepare(sql).map_err(|e|e.to_string())?;let rows=st.query_map([],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"invoiceNo":r.get::<_,String>(1)?,"date":r.get::<_,String>(2)?,"totalMinor":r.get::<_,i64>(3)?}))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
}

#[tauri::command(rename_all="camelCase")]
fn create_return(app:tauri::AppHandle,state:State<'_,AppState>,invoice_type:String,invoice_id:i64,return_date:String,notes:Option<String>,items:Vec<ReturnLineInput>)->Result<serde_json::Value,String>{
 let ctx=invoice_context(&invoice_type)?;let actor=require_permission(state.inner(),if ctx.kind=="sale"{"sales.create"}else{"purchases.create"})?;
 if items.is_empty()||items.len()>200{return Err("حدد صنفًا واحدًا على الأقل للمرتجع".into());}if items.iter().any(|x|x.quantity_milli<=0){return Err("كمية المرتجع يجب أن تكون أكبر من صفر".into());}
 let path=database_path(&app)?;initialize_database(&path)?;let mut c=connect(&path)?;let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
 let date:Option<String>=tx.query_row("SELECT date(?1)",[return_date.as_str()],|r|r.get(0)).map_err(|e|e.to_string())?;if date.as_deref()!=Some(return_date.as_str()){return Err("تاريخ المرتجع غير صالح".into());}
 let (header,lines,fk,ret_header,ret_lines,ret_fk,kind)=if ctx.kind=="sale"{("sales","sale_items","sale_id","sales_returns","sales_return_items","sale_item_id","sale_return")}else{("purchase_invoices","purchase_invoice_items","purchase_invoice_id","purchase_returns","purchase_return_items","purchase_item_id","purchase_return")};
 let (status,invoice_subtotal,invoice_total,invoice_tax):(String,i64,i64,i64)=tx.query_row(&format!("SELECT status,subtotal_minor,total_minor,tax_minor FROM {header} WHERE id=?1"),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?.ok_or("الفاتورة الأصلية غير موجودة")?;if status!="posted"{return Err("لا يمكن عمل مرتجع إلا لفاتورة معتمدة".into());}
 let no=format!("RET-{}-{}",if ctx.kind=="sale"{"S"}else{"P"},SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos());
 let return_source_col=if ctx.kind=="sale" {"sale_id"} else {"purchase_invoice_id"};
 let insert= format!("INSERT INTO {ret_header}(return_no,{return_source_col},return_date,total_minor,notes,created_by,created_by_username) VALUES(?1,?2,?3,0,?4,?5,?6)");
 tx.execute(&insert,params![no,invoice_id,return_date,notes,actor.id,actor.username]).map_err(|e|e.to_string())?;let rid=tx.last_insert_rowid();let mut total=0i64;
 for item in &items {
  let (product_id,source_line_total,source_discount,source_tax,unit_price,original_qty):(i64,i64,i64,i64,i64,i64)=tx.query_row(&format!("SELECT product_id,line_total_minor,discount_minor,tax_minor,unit_price_minor,quantity_milli FROM {lines} WHERE id=?1 AND {fk}=?2"),params![item.source_item_id,invoice_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional().map_err(|e|e.to_string())?.ok_or("أحد الأصناف لا يتبع الفاتورة المحددة")?;
  let returned:i64=tx.query_row(&format!("SELECT COALESCE(SUM(quantity_milli),0) FROM {ret_lines} WHERE {ret_fk}=?1"),[item.source_item_id],|r|r.get(0)).map_err(|e|e.to_string())?;
  if item.quantity_milli>original_qty-returned{return Err("كمية المرتجع أكبر من الكمية المتبقية من الفاتورة الأصلية".into());}
  // Pro-rate the persisted source line total by returned quantity, rounding half-up once.
  // This avoids double truncation from deriving a unit price and multiplying it back.
  let gross_return=if original_qty>0 { (((source_line_total as i128)*(item.quantity_milli as i128)+(original_qty as i128/2))/(original_qty as i128)).clamp(0,i64::MAX as i128) as i64 } else { 0 };
  let discount_return=if original_qty>0 { (((source_discount as i128)*(item.quantity_milli as i128)+(original_qty as i128/2))/(original_qty as i128)).clamp(0,gross_return as i128) as i64 } else { 0 };
  let tax_return=if original_qty>0 { (((source_tax as i128)*(item.quantity_milli as i128)+(original_qty as i128/2))/(original_qty as i128)).clamp(0,i64::MAX as i128) as i64 } else { 0 };
  let line_total=gross_return.saturating_sub(discount_return).saturating_add(if invoice_total==invoice_subtotal { 0 } else { tax_return });
  total=total.checked_add(line_total).ok_or("إجمالي المرتجع أكبر من الحد المسموح")?;
  tx.execute(&format!("INSERT INTO {ret_lines}(return_id,{ret_fk},product_id,quantity_milli,unit_price_minor,line_total_minor) VALUES(?1,?2,?3,?4,?5,?6)"),params![rid,item.source_item_id,product_id,item.quantity_milli,unit_price,line_total]).map_err(|e|e.to_string())?;
  let warehouse:i64=if let Some(w)=item.warehouse_id { let ok:i64=tx.query_row("SELECT COUNT(*) FROM warehouses WHERE id=?1 AND is_active=1",[w],|r|r.get(0)).map_err(|e|e.to_string())?; if ok==0{return Err("المخزن المحدد غير موجود أو غير نشط".into());} w } else { tx.query_row("SELECT id FROM warehouses WHERE code='MAIN' LIMIT 1",[],|r|r.get(0)).map_err(|e|e.to_string())? };
  let signed=if ctx.kind=="sale"{item.quantity_milli}else{-item.quantity_milli};
  if ctx.kind=="purchase" {
   let stock:i64=tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1 AND warehouse_id=?2",params![product_id,warehouse],|r|r.get(0)).map_err(|e|e.to_string())?;
   if stock<item.quantity_milli{return Err("لا يمكن إرجاع كمية مشتريات أكبر من الرصيد المتاح بالمخزن الرئيسي".into());}
   // Keep batch balances aligned. If the operator selected a batch, enforce that exact batch and its available quantity.
   if let Some(batch_id)=item.batch_id {
    let available:Option<i64>=tx.query_row("SELECT remaining_milli FROM product_batches WHERE id=?1 AND product_id=?2 AND warehouse_id=?3",params![batch_id,product_id,warehouse],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
    let available=available.ok_or("التشغيلة المحددة لا تخص الصنف والمخزن المختارين")?;
    if available<item.quantity_milli{return Err("كمية المرتجع أكبر من المتبقي في التشغيلة المحددة".into());}
    tx.execute("UPDATE product_batches SET remaining_milli=remaining_milli-?1 WHERE id=?2",params![item.quantity_milli,batch_id]).map_err(|e|e.to_string())?;
   } else {
    let batches={let mut st=tx.prepare("SELECT id,remaining_milli FROM product_batches WHERE product_id=?1 AND warehouse_id=?2 AND remaining_milli>0 ORDER BY expiry_date,received_date,id").map_err(|e|e.to_string())?;let rows=st.query_map(params![product_id,warehouse],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};
    let mut left=item.quantity_milli;for (bid,available) in batches {if left<=0{break;}let take=left.min(available);tx.execute("UPDATE product_batches SET remaining_milli=remaining_milli-?1 WHERE id=?2",params![take,bid]).map_err(|e|e.to_string())?;left-=take;}
   }
  }
  let unit:String=tx.query_row("SELECT unit FROM products WHERE id=?1",[product_id],|r|r.get(0)).map_err(|e|e.to_string())?;
  tx.execute("INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,reference_id,notes,unit) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![product_id,warehouse,kind,signed,ret_header,rid,format!("مرتجع مرتبط بالفاتورة {}",invoice_id),unit]).map_err(|e|e.to_string())?;
  if ctx.kind=="sale" {
   let batch_no=format!("{}-{}",no,product_id);
   tx.execute("INSERT INTO product_batches(product_id,warehouse_id,batch_no,received_date,expiry_date,expiry_known,quantity_milli,remaining_milli,source_invoice_id,notes) VALUES(?1,?2,?3,?4,'9999-12-31',0,?5,?5,NULL,'مرتجع مبيعات؛ تاريخ الصلاحية/التشغيلة الأصلية غير متاح')",params![product_id,warehouse,batch_no,return_date,item.quantity_milli]).map_err(|e|e.to_string())?;
  }
 }
 // Allocate invoice-level discount/tax proportionally to the returned net line value.
 let adjusted_total=total;
 tx.execute(&format!("UPDATE {ret_header} SET total_minor=?1 WHERE id=?2"),params![adjusted_total,rid]).map_err(|e|e.to_string())?;
 let (ret_date,_party_id):(String,Option<i64>)=tx.query_row(&format!("SELECT return_date,{party_col} FROM {header} WHERE id=?1",header=header,party_col=if ctx.kind=="sale"{"customer_id"}else{"supplier_id"}),[invoice_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;
 let return_tax=if invoice_total>0 { ((adjusted_total as i128)*(invoice_tax as i128)/(invoice_total as i128)).clamp(0,adjusted_total as i128) as i64 } else { 0 };
 let return_net=adjusted_total.saturating_sub(return_tax);
 if ctx.kind=="sale" {
   let mut journal_lines=vec![("4000",return_net,0),("2100",return_tax,0),("1100",0,adjusted_total)];
   journal_lines.retain(|(_,d,c)|*d>0||*c>0);
   post_automatic_journal(&tx,"sales_return",rid,&ret_date,&format!("مرتجع مبيعات {no}"),&journal_lines,Some(actor.id),Some(&actor.username))?;
   let mut cost=0i64;for item in &items {let c:Option<i64>=tx.query_row("SELECT cost_price_minor FROM sale_items WHERE id=?1",[item.source_item_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;cost=cost.saturating_add(((item.quantity_milli as i128)*(c.unwrap_or(0) as i128)/1000).clamp(0,i64::MAX as i128) as i64);}
   if cost>0 {post_automatic_journal(&tx,"sales_return_cogs",rid,&ret_date,&format!("عكس تكلفة مرتجع المبيعات {no}"),&[("1200",cost,0),("5000",0,cost)],Some(actor.id),Some(&actor.username))?;}
 } else {
   let mut journal_lines=vec![("2000",adjusted_total,0),("1200",0,return_net),("1300",0,return_tax)];
   journal_lines.retain(|(_,d,c)|*d>0||*c>0);
   post_automatic_journal(&tx,"purchase_return",rid,&ret_date,&format!("مرتجع مشتريات {no}"),&journal_lines,Some(actor.id),Some(&actor.username))?;
 }
 write_audit(&tx,Some(&actor),"create",ret_header,Some(&rid.to_string()),&format!("{} — الفاتورة {} — الإجمالي {}",no,invoice_id,adjusted_total))?;tx.commit().map_err(|e|e.to_string())?;Ok(serde_json::json!({"id":rid,"returnNo":no,"totalMinor":adjusted_total}))
}

#[tauri::command(rename_all="camelCase")]
fn get_party_statement(app:tauri::AppHandle,state:State<'_,AppState>,party_type:String,party_id:i64,from_date:Option<String>,to_date:Option<String>)->Result<serde_json::Value,String>{
 let customer=party_type=="customer";if !customer&&party_type!="supplier"{return Err("نوع الحساب غير صحيح".into());}let _=require_permission(state.inner(),if customer{"customers.view"}else{"suppliers.view"})?;let path=database_path(&app)?;initialize_database(&path)?;let c=connect(&path)?;
 let (inv,ret,partycol,ptype)=if customer{("sales","sales_returns","customer_id","sale")}else{("purchase_invoices","purchase_returns","supplier_id","purchase")};
 let from=from_date.unwrap_or_else(||"0001-01-01".into());let to=to_date.unwrap_or_else(||"9999-12-31".into());
 let ret_source_col=if customer {"sale_id"} else {"purchase_invoice_id"}; let sql=format!("SELECT date,description,ref, debit,credit FROM (SELECT i.invoice_date date,('فاتورة '||i.invoice_no) description,i.invoice_no ref,CASE WHEN '{ptype}'='sale' THEN i.total_minor ELSE 0 END debit,CASE WHEN '{ptype}'='purchase' THEN i.total_minor ELSE 0 END credit FROM {inv} i WHERE i.{partycol}=?1 AND i.status='posted' UNION ALL SELECT p.payment_date,('سداد '||COALESCE(p.reference_no,'رقم '||p.id)),COALESCE(p.reference_no,CAST(p.id AS TEXT)),CASE WHEN p.direction='out' THEN p.amount_minor ELSE 0 END,CASE WHEN p.direction='in' THEN p.amount_minor ELSE 0 END FROM payments p JOIN {inv} i ON p.invoice_id=i.id AND p.invoice_type='{ptype}' WHERE i.{partycol}=?1 UNION ALL SELECT r.return_date,('مرتجع '||r.return_no),r.return_no,CASE WHEN '{ptype}'='purchase' THEN r.total_minor ELSE 0 END,CASE WHEN '{ptype}'='sale' THEN r.total_minor ELSE 0 END FROM {ret} r JOIN {inv} i ON i.id=r.{ret_source_col} WHERE i.{partycol}=?1) ORDER BY date,ref");
 let mut st=c.prepare(&sql).map_err(|e|e.to_string())?;let rows=st.query_map(params![party_id],|r|Ok(serde_json::json!({"date":r.get::<_,String>(0)?,"description":r.get::<_,String>(1)?,"reference":r.get::<_,String>(2)?,"debitMinor":r.get::<_,i64>(3)?,"creditMinor":r.get::<_,i64>(4)?}))).map_err(|e|e.to_string())?;let all_entries=rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let mut opening=0i64;let mut balance=0i64;let mut total_debit=0i64;let mut total_credit=0i64;let mut entries=Vec::new();for e in all_entries{let date=e["date"].as_str().unwrap_or("");let d=e["debitMinor"].as_i64().unwrap_or(0);let cr=e["creditMinor"].as_i64().unwrap_or(0);if date<from.as_str(){opening+=d-cr;continue;}if date>to.as_str(){continue;}total_debit+=d;total_credit+=cr;balance+=d-cr;entries.push(e);}Ok(serde_json::json!({"entries":entries,"openingMinor":opening,"debitMinor":total_debit,"creditMinor":total_credit,"balanceMinor":opening+balance}))
}

#[tauri::command(rename_all="camelCase")]
fn list_warehouses(app:tauri::AppHandle,state:State<'_,AppState>)->Result<Vec<serde_json::Value>,String>{let _=require_permission(state.inner(),"stock.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT id,code,name,is_active FROM warehouses ORDER BY id").map_err(|e|e.to_string())?;let r=s.query_map([],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"code":x.get::<_,String>(1)?,"name":x.get::<_,String>(2)?,"isActive":x.get::<_,i64>(3)?!=0}))).map_err(|e|e.to_string())?;r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}
#[tauri::command(rename_all="camelCase")]
fn create_warehouse(app:tauri::AppHandle,state:State<'_,AppState>,code:String,name:String)->Result<i64,String>{let a=require_permission(state.inner(),"stock.movement.record")?;let code=code.trim();let name=name.trim();if code.is_empty()||name.is_empty(){return Err("كود واسم المخزن مطلوبان".into());}let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;c.execute("INSERT INTO warehouses(code,name) VALUES(?1,?2)",params![code,name]).map_err(|e|format!("تعذر إنشاء المخزن: {e}"))?;let id=c.last_insert_rowid();write_audit(&c,Some(&a),"create","warehouse",Some(&id.to_string()),name)?;Ok(id)}
#[tauri::command(rename_all="camelCase")]
fn transfer_stock(app:tauri::AppHandle,state:State<'_,AppState>,product_id:i64,from_warehouse_id:i64,to_warehouse_id:i64,quantity_milli:i64,notes:Option<String>)->Result<(),String>{let a=require_permission(state.inner(),"stock.movement.record")?;if from_warehouse_id==to_warehouse_id||quantity_milli<=0{return Err("اختر مخزنين مختلفين وكمية موجبة".into());}let p=database_path(&app)?;initialize_database(&p)?;let mut c=connect(&p)?;let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let stock:i64=tx.query_row("SELECT COALESCE(SUM(quantity_milli),0) FROM stock_movements WHERE product_id=?1 AND warehouse_id=?2",params![product_id,from_warehouse_id],|r|r.get(0)).map_err(|e|e.to_string())?;if stock<quantity_milli{return Err("الرصيد غير كافٍ في المخزن المصدر".into());}
 // Transfer tracked batch balances in FEFO order and preserve expiry metadata at the destination.
 let batches={let mut st=tx.prepare("SELECT id,batch_no,received_date,expiry_date,remaining_milli,notes,source_invoice_id,expiry_known FROM product_batches WHERE product_id=?1 AND warehouse_id=?2 AND remaining_milli>0 ORDER BY expiry_date,received_date,id").map_err(|e|e.to_string())?;let rows=st.query_map(params![product_id,from_warehouse_id],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,i64>(4)?,r.get::<_,Option<String>>(5)?,r.get::<_,Option<i64>>(6)?,r.get::<_,i64>(7)?))).map_err(|e|e.to_string())?;rows.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?};let mut left=quantity_milli;for (bid,bno,received,expiry,avail,bnotes,source,expiry_known) in batches{if left<=0{break;}let take=left.min(avail);tx.execute("UPDATE product_batches SET remaining_milli=remaining_milli-?1 WHERE id=?2",params![take,bid]).map_err(|e|e.to_string())?;let dest_no=format!("{}-T{}",bno,from_warehouse_id);tx.execute("INSERT INTO product_batches(product_id,warehouse_id,batch_no,received_date,expiry_date,expiry_known,quantity_milli,remaining_milli,source_invoice_id,notes) VALUES(?1,?2,?3,?4,?5,?9,?6,?6,?7,?8) ON CONFLICT(product_id,warehouse_id,batch_no,expiry_date) DO UPDATE SET quantity_milli=quantity_milli+excluded.quantity_milli,remaining_milli=remaining_milli+excluded.remaining_milli,expiry_known=MIN(expiry_known,excluded.expiry_known)",params![product_id,to_warehouse_id,dest_no,received,expiry,take,source,bnotes,expiry_known]).map_err(|e|e.to_string())?;left-=take;}
 let unit:String=tx.query_row("SELECT unit FROM products WHERE id=?1",[product_id],|r|r.get(0)).map_err(|e|e.to_string())?;let note=notes.unwrap_or_default();for (wid,kind,q) in [(from_warehouse_id,"transfer_out",-quantity_milli),(to_warehouse_id,"transfer_in",quantity_milli)]{tx.execute("INSERT INTO stock_movements(product_id,warehouse_id,movement_type,quantity_milli,reference_type,reference_id,notes,unit) VALUES(?1,?2,?3,?4,'warehouse_transfer',NULL,?5,?6)",params![product_id,wid,kind,q,format!("تحويل مخزني: {note}"),unit]).map_err(|e|e.to_string())?;}write_audit(&tx,Some(&a),"transfer","stock",Some(&product_id.to_string()),&format!("{} إلى {}، كمية {}",from_warehouse_id,to_warehouse_id,quantity_milli))?;tx.commit().map_err(|e|e.to_string())}

#[tauri::command(rename_all="camelCase")]
fn save_product_batch(app:tauri::AppHandle,state:State<'_,AppState>,product_id:i64,warehouse_id:i64,batch_no:String,received_date:String,expiry_date:String,quantity_milli:i64,notes:Option<String>)->Result<i64,String>{let a=require_permission(state.inner(),"stock.movement.record")?;if quantity_milli<=0||batch_no.trim().is_empty()||expiry_date<received_date{return Err("تحقق من رقم التشغيلة والكمية وتواريخ الاستلام والصلاحية".into());}let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;for d in [&received_date,&expiry_date]{let normalized:Option<String>=c.query_row("SELECT date(?1)",[d.as_str()],|r|r.get(0)).map_err(|e|e.to_string())?;if normalized.as_deref()!=Some(d.as_str()){return Err("صيغة تاريخ التشغيلة غير صحيحة".into());}}c.execute("INSERT INTO product_batches(product_id,warehouse_id,batch_no,received_date,expiry_date,quantity_milli,remaining_milli,notes) VALUES(?1,?2,?3,?4,?5,?6,?6,?7)",params![product_id,warehouse_id,batch_no.trim(),received_date,expiry_date,quantity_milli,notes]).map_err(|e|format!("تعذر حفظ التشغيلة: {e}"))?;let id=c.last_insert_rowid();write_audit(&c,Some(&a),"create","product_batch",Some(&id.to_string()),batch_no.trim())?;Ok(id)}
#[tauri::command(rename_all="camelCase")]
fn list_return_batches(app:tauri::AppHandle,state:State<'_,AppState>,product_id:i64,warehouse_id:i64)->Result<Vec<serde_json::Value>,String>{let _=require_permission(state.inner(),"stock.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT id,batch_no,expiry_date,remaining_milli,expiry_known FROM product_batches WHERE product_id=?1 AND warehouse_id=?2 AND remaining_milli>0 ORDER BY expiry_known DESC,expiry_date,received_date,id").map_err(|e|e.to_string())?;let r=s.query_map(params![product_id,warehouse_id],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"batchNo":x.get::<_,String>(1)?,"expiryDate":x.get::<_,String>(2)?,"remainingMilli":x.get::<_,i64>(3)?,"expiryKnown":x.get::<_,i64>(4)?!=0}))).map_err(|e|e.to_string())?;r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}

#[tauri::command(rename_all="camelCase")]
fn list_expiring_batches(app:tauri::AppHandle,state:State<'_,AppState>,days:i64)->Result<Vec<serde_json::Value>,String>{let _=require_permission(state.inner(),"stock.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT b.id,p.code,p.name,w.name,b.batch_no,b.received_date,b.expiry_date,b.remaining_milli FROM product_batches b JOIN products p ON p.id=b.product_id JOIN warehouses w ON w.id=b.warehouse_id WHERE b.remaining_milli>0 AND b.expiry_known=1 AND b.expiry_date<=date('now', '+'||?1||' days') ORDER BY b.expiry_date").map_err(|e|e.to_string())?;let r=s.query_map([days.clamp(0,3650)],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"code":x.get::<_,String>(1)?,"product":x.get::<_,String>(2)?,"warehouse":x.get::<_,String>(3)?,"batch":x.get::<_,String>(4)?,"received":x.get::<_,String>(5)?,"expiry":x.get::<_,String>(6)?,"remainingMilli":x.get::<_,i64>(7)?}))).map_err(|e|e.to_string())?;r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}

#[tauri::command(rename_all="camelCase")]
fn open_cash_shift(app:tauri::AppHandle,state:State<'_,AppState>,opening_minor:i64,notes:Option<String>)->Result<i64,String>{let a=require_permission(state.inner(),"cashbox.adjust")?;if opening_minor<0{return Err("الرصيد الافتتاحي لا يمكن أن يكون سالبًا".into());}let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;c.execute("INSERT INTO cash_shifts(opening_minor,opened_by,opened_by_username,notes) VALUES(?1,?2,?3,?4)",params![opening_minor,a.id,a.username,notes]).map_err(|e|format!("تعذر فتح الوردية: {e}"))?;Ok(c.last_insert_rowid())}
#[tauri::command(rename_all="camelCase")]
fn get_cash_shift(app:tauri::AppHandle,state:State<'_,AppState>)->Result<Option<serde_json::Value>,String>{let _=require_permission(state.inner(),"cashbox.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;c.query_row("SELECT id,opened_at,opening_minor,opened_by_username FROM cash_shifts WHERE status='open' ORDER BY id DESC LIMIT 1",[],|r|Ok(serde_json::json!({"id":r.get::<_,i64>(0)?,"openedAt":r.get::<_,String>(1)?,"openingMinor":r.get::<_,i64>(2)?,"openedBy":r.get::<_,String>(3)?}))).optional().map_err(|e|e.to_string())}
#[tauri::command(rename_all="camelCase")]
fn close_cash_shift(app:tauri::AppHandle,state:State<'_,AppState>,shift_id:i64,actual_minor:i64,notes:Option<String>)->Result<serde_json::Value,String>{let a=require_permission(state.inner(),"cashbox.adjust")?;if actual_minor<0{return Err("النقدية الفعلية لا يمكن أن تكون سالبة".into());}let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let (opened,opening):(String,i64)=c.query_row("SELECT opened_at,opening_minor FROM cash_shifts WHERE id=?1 AND status='open'",[shift_id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|e|e.to_string())?;let flow:i64=c.query_row("SELECT COALESCE(SUM(CASE direction WHEN 'in' THEN amount_minor ELSE -amount_minor END),0) FROM cashbook_entries WHERE created_at>=?1",[opened.as_str()],|r|r.get(0)).map_err(|e|e.to_string())?;let expected=opening+flow;let diff=actual_minor-expected;c.execute("UPDATE cash_shifts SET status='closed',closed_at=datetime('now'),expected_minor=?1,actual_minor=?2,difference_minor=?3,closed_by=?4,closed_by_username=?5,notes=COALESCE(?6,notes) WHERE id=?7 AND status='open'",params![expected,actual_minor,diff,a.id,a.username,notes,shift_id]).map_err(|e|e.to_string())?;Ok(serde_json::json!({"expectedMinor":expected,"actualMinor":actual_minor,"differenceMinor":diff}))}

#[tauri::command(rename_all="camelCase")]
fn list_gl_accounts(app:tauri::AppHandle,state:State<'_,AppState>)->Result<Vec<serde_json::Value>,String>{let _=require_permission(state.inner(),"reports.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT id,code,name,account_type FROM gl_accounts WHERE is_active=1 ORDER BY code").map_err(|e|e.to_string())?;let r=s.query_map([],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"code":x.get::<_,String>(1)?,"name":x.get::<_,String>(2)?,"type":x.get::<_,String>(3)?}))).map_err(|e|e.to_string())?;r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}
#[tauri::command(rename_all="camelCase")]
fn create_journal_entry(app:tauri::AppHandle,state:State<'_,AppState>,entry_date:String,description:String,lines:Vec<serde_json::Value>)->Result<i64,String>{let a=require_permission(state.inner(),"reports.view")?;if lines.len()<2||description.trim().is_empty(){return Err("القيد يحتاج وصفًا وسطرين على الأقل".into());}let mut debit=0i64;let mut credit=0i64;for l in &lines{debit+=l["debitMinor"].as_i64().unwrap_or(0);credit+=l["creditMinor"].as_i64().unwrap_or(0);}if debit<=0||debit!=credit{return Err("القيد غير متوازن: يجب أن يتساوى إجمالي المدين والدائن".into());}let p=database_path(&app)?;initialize_database(&p)?;let mut c=connect(&p)?;let tx=c.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|e|e.to_string())?;let normalized:Option<String>=tx.query_row("SELECT date(?1)",[entry_date.as_str()],|r|r.get(0)).map_err(|e|e.to_string())?;if normalized.as_deref()!=Some(entry_date.as_str()){return Err("تاريخ القيد غير صحيح".into());}let no=format!("JE-{}",SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos());tx.execute("INSERT INTO gl_journals(entry_no,entry_date,description,created_by,created_by_username) VALUES(?1,?2,?3,?4,?5)",params![no,entry_date,description.trim(),a.id,a.username]).map_err(|e|e.to_string())?;let id=tx.last_insert_rowid();for l in &lines{let account=l["accountId"].as_i64().ok_or("حساب القيد غير صالح")?;let d=l["debitMinor"].as_i64().unwrap_or(0);let cr=l["creditMinor"].as_i64().unwrap_or(0);if d<0||cr<0||(d==0)==(cr==0){return Err("كل سطر يجب أن يكون مدينًا أو دائنًا فقط".into());}tx.execute("INSERT INTO gl_journal_lines(journal_id,account_id,debit_minor,credit_minor) VALUES(?1,?2,?3,?4)",params![id,account,d,cr]).map_err(|e|e.to_string())?;}write_audit(&tx,Some(&a),"create","gl_journal",Some(&id.to_string()),description.trim())?;tx.commit().map_err(|e|e.to_string())?;Ok(id)}
#[tauri::command(rename_all="camelCase")]
fn get_trial_balance(app:tauri::AppHandle,state:State<'_,AppState>,from_date:String,to_date:String)->Result<serde_json::Value,String>{let _=require_permission(state.inner(),"reports.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT a.code,a.name,a.account_type,COALESCE(SUM(l.debit_minor),0),COALESCE(SUM(l.credit_minor),0) FROM gl_accounts a LEFT JOIN (gl_journal_lines l JOIN gl_journals j ON j.id=l.journal_id AND j.posted=1 AND j.entry_date BETWEEN ?1 AND ?2) ON l.account_id=a.id WHERE a.is_active=1 GROUP BY a.id ORDER BY a.code").map_err(|e|e.to_string())?;let r=s.query_map(params![from_date,to_date],|x|Ok(serde_json::json!({"code":x.get::<_,String>(0)?,"name":x.get::<_,String>(1)?,"type":x.get::<_,String>(2)?,"debitMinor":x.get::<_,i64>(3)?,"creditMinor":x.get::<_,i64>(4)?}))).map_err(|e|e.to_string())?;let rows=r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let debit:i64=rows.iter().map(|v|v["debitMinor"].as_i64().unwrap_or(0)).sum();let credit:i64=rows.iter().map(|v|v["creditMinor"].as_i64().unwrap_or(0)).sum();Ok(serde_json::json!({"rows":rows,"debitMinor":debit,"creditMinor":credit,"balanced":debit==credit}))}

#[tauri::command(rename_all="camelCase")]
fn get_inventory_analytics(app:tauri::AppHandle,state:State<'_,AppState>,stale_days:i64)->Result<serde_json::Value,String>{let _=require_permission(state.inner(),"reports.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT p.id,p.code,p.name,COALESCE(SUM(sm.quantity_milli),0),MAX(sm.created_at),COALESCE(p.purchase_price_minor,0) FROM products p LEFT JOIN stock_movements sm ON sm.product_id=p.id WHERE p.is_active=1 GROUP BY p.id ORDER BY p.name").map_err(|e|e.to_string())?;let r=s.query_map([],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"code":x.get::<_,String>(1)?,"name":x.get::<_,String>(2)?,"quantityMilli":x.get::<_,i64>(3)?,"lastMovement":x.get::<_,Option<String>>(4)?,"purchasePriceMinor":x.get::<_,i64>(5)?}))).map_err(|e|e.to_string())?;let mut stale=Vec::new();for row in r{let v=row.map_err(|e|e.to_string())?;let last=v["lastMovement"].as_str().unwrap_or("");let old=last.is_empty()||c.query_row("SELECT date(?1)<=date('now','-'||?2||' days')",params![last,stale_days.clamp(1,3650)],|x|x.get::<_,i64>(0)).unwrap_or(0)!=0;if old{stale.push(v);}}let mut exp=c.prepare("SELECT b.id,p.code,p.name,b.batch_no,b.expiry_date,b.remaining_milli FROM product_batches b JOIN products p ON p.id=b.product_id WHERE b.remaining_milli>0 AND b.expiry_known=1 AND b.expiry_date<=date('now','+90 days') ORDER BY b.expiry_date").map_err(|e|e.to_string())?;let er=exp.query_map([],|x|Ok(serde_json::json!({"id":x.get::<_,i64>(0)?,"code":x.get::<_,String>(1)?,"name":x.get::<_,String>(2)?,"batch":x.get::<_,String>(3)?,"expiry":x.get::<_,String>(4)?,"remainingMilli":x.get::<_,i64>(5)?}))).map_err(|e|e.to_string())?;let expiring=er.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;Ok(serde_json::json!({"stale":stale,"expiring":expiring}))}

#[tauri::command(rename_all="camelCase")]
fn get_debt_aging(app:tauri::AppHandle,state:State<'_,AppState>)->Result<serde_json::Value,String>{let _=require_permission(state.inner(),"reports.view")?;let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let sql="SELECT party_type,party_name,invoice_no,invoice_date,remaining_minor,age_days FROM (SELECT 'عميل' party_type,COALESCE(c.name,'عميل نقدي') party_name,s.invoice_no,s.invoice_date,MAX(0,s.total_minor-s.paid_minor-COALESCE((SELECT SUM(r.total_minor) FROM sales_returns r WHERE r.sale_id=s.id),0)) remaining_minor,CAST(julianday('now')-julianday(s.invoice_date) AS INTEGER) age_days FROM sales s LEFT JOIN customers c ON c.id=s.customer_id WHERE s.status='posted' UNION ALL SELECT 'مورد',sp.name,p.invoice_no,p.invoice_date,MAX(0,p.total_minor-p.paid_minor-COALESCE((SELECT SUM(r.total_minor) FROM purchase_returns r WHERE r.purchase_invoice_id=p.id),0)),CAST(julianday('now')-julianday(p.invoice_date) AS INTEGER) FROM purchase_invoices p JOIN suppliers sp ON sp.id=p.supplier_id WHERE p.status='posted') WHERE remaining_minor>0 ORDER BY age_days DESC";let mut s=c.prepare(sql).map_err(|e|e.to_string())?;let r=s.query_map([],|x|Ok(serde_json::json!({"partyType":x.get::<_,String>(0)?,"party":x.get::<_,String>(1)?,"invoiceNo":x.get::<_,String>(2)?,"date":x.get::<_,String>(3)?,"remainingMinor":x.get::<_,i64>(4)?,"ageDays":x.get::<_,i64>(5)?}))).map_err(|e|e.to_string())?;let rows=r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;let mut buckets=[0i64;4];for x in &rows{let age=x["ageDays"].as_i64().unwrap_or(0);let amt=x["remainingMinor"].as_i64().unwrap_or(0);let i=if age<=30{0}else if age<=60{1}else if age<=90{2}else{3};buckets[i]+=amt;}Ok(serde_json::json!({"rows":rows,"bucketsMinor":buckets}))}
#[tauri::command(rename_all="camelCase")]
fn get_product_movement_report(app:tauri::AppHandle,state:State<'_,AppState>,product_id:i64,from_date:String,to_date:String)->Result<Vec<serde_json::Value>,String>{let _=require_permission(state.inner(),"reports.view")?;if from_date>to_date{return Err("تاريخ البداية يجب أن يسبق النهاية".into());}let p=database_path(&app)?;initialize_database(&p)?;let c=connect(&p)?;let mut s=c.prepare("SELECT sm.created_at,w.name,sm.movement_type,sm.quantity_milli,sm.reference_type,sm.reference_id,sm.notes,sm.unit FROM stock_movements sm JOIN warehouses w ON w.id=sm.warehouse_id WHERE sm.product_id=?1 AND date(sm.created_at) BETWEEN ?2 AND ?3 ORDER BY sm.id").map_err(|e|e.to_string())?;let r=s.query_map(params![product_id,from_date,to_date],|x|Ok(serde_json::json!({"date":x.get::<_,String>(0)?,"warehouse":x.get::<_,String>(1)?,"type":x.get::<_,String>(2)?,"quantityMilli":x.get::<_,i64>(3)?,"referenceType":x.get::<_,Option<String>>(4)?,"referenceId":x.get::<_,Option<i64>>(5)?,"notes":x.get::<_,Option<String>>(6)?,"unit":x.get::<_,Option<String>>(7)?}))).map_err(|e|e.to_string())?;r.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())}


pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let clipboard_sender = start_clipboard_worker().map_err(std::io::Error::other)?;
            app.manage(ClipboardWorkerState { sender: clipboard_sender });
            let path = database_path(&app.handle()).map_err(std::io::Error::other)?;
            initialize_database(&path).map_err(std::io::Error::other)?;
            Ok(())
        })

        .invoke_handler(tauri::generate_handler![clipboard_read_text, clipboard_write_text, app_info, auth_status, create_initial_admin, login, logout, list_users, create_user, update_user_profile, set_user_active, set_user_role, list_user_permissions, set_user_permissions, reset_user_password, change_own_password, list_audit_log, create_product, find_product_by_barcode, get_dashboard_inventory_counts, list_products, search_products, list_inactive_products, search_inactive_products, reactivate_product, update_product, add_alternate_barcode, deactivate_product, list_stock, list_stock_movements, record_stock_movement, list_parties, save_party, set_party_active, get_company_branding, get_company_profile, get_company_printing_profile, save_company_profile, list_payable_invoices, record_payment, list_payments, cashbox_adjustment, list_cashbook, get_financial_summary, list_quotations, get_quotation_details, save_quotation, set_quotation_status, convert_quotation_to_sale, list_invoices, get_invoice_details, create_invoice, update_invoice_draft, post_invoice, cancel_invoice, create_backup, list_restore_points, delete_restore_point, restore_backup, list_returnable_invoices, create_return, get_party_statement, list_warehouses, create_warehouse, transfer_stock, save_product_batch, list_return_batches, list_expiring_batches, open_cash_shift, get_cash_shift, close_cash_shift, list_gl_accounts, create_journal_entry, get_trial_balance, get_inventory_analytics, get_debt_aging, get_product_movement_report])
        .run(tauri::generate_context!())
        .expect("خطأ أثناء تشغيل مدار");
}
