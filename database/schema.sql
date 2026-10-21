PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    address TEXT,
    notes TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS suppliers (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    email TEXT,
    phone TEXT,
    address TEXT,
    notes TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    category_id INTEGER,
    unit TEXT NOT NULL DEFAULT 'قطعة',
    purchase_price_minor INTEGER NOT NULL DEFAULT 0 CHECK (purchase_price_minor >= 0),
    sale_price_minor INTEGER NOT NULL DEFAULT 0 CHECK (sale_price_minor >= 0),
    minimum_quantity_milli INTEGER NOT NULL DEFAULT 0 CHECK (minimum_quantity_milli >= 0),
    notes TEXT,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- A product can have more than one barcode (e.g. alternate unit/package barcodes).
-- Barcode values are stored as TEXT to preserve leading zeros and scanner input exactly.
CREATE TABLE IF NOT EXISTS product_barcodes (
    id INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    unit TEXT NOT NULL DEFAULT 'قطعة',
    barcode TEXT NOT NULL UNIQUE,
    barcode_type TEXT NOT NULL CHECK (barcode_type IN ('legacy', 'internal', 'alternate', 'unit')),
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK (length(trim(barcode)) > 0)
);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_product ON product_barcodes(product_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_product_one_primary_barcode ON product_barcodes(product_id) WHERE is_primary = 1;
CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);

CREATE TABLE IF NOT EXISTS warehouses (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS stock_movements (
    id INTEGER PRIMARY KEY,
    product_id INTEGER NOT NULL REFERENCES products(id),
    warehouse_id INTEGER NOT NULL REFERENCES warehouses(id),
    movement_type TEXT NOT NULL CHECK (movement_type IN ('opening', 'purchase', 'sale', 'purchase_return', 'sale_return', 'adjustment', 'transfer_in', 'transfer_out')),
    quantity_milli INTEGER NOT NULL CHECK (quantity_milli <> 0),
    reference_type TEXT,
    reference_id INTEGER,
    notes TEXT,
    unit TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_stock_movements_product_warehouse ON stock_movements(product_id, warehouse_id, created_at);

CREATE TABLE IF NOT EXISTS purchase_invoices (
    id INTEGER PRIMARY KEY,
    invoice_no TEXT NOT NULL UNIQUE,
    supplier_id INTEGER NOT NULL REFERENCES suppliers(id),
    invoice_date TEXT NOT NULL,
    subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK (subtotal_minor >= 0),
    discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    total_minor INTEGER NOT NULL DEFAULT 0 CHECK (total_minor >= 0),
    tax_rate_bps INTEGER NOT NULL DEFAULT 0,
    tax_minor INTEGER NOT NULL DEFAULT 0,
    prices_include_tax INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0,1)),
    paid_minor INTEGER NOT NULL DEFAULT 0 CHECK (paid_minor >= 0),
    notes TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','posted','cancelled')),
    created_by INTEGER,
    created_by_username TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    posted_at TEXT,
    cancelled_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_purchase_invoices_date ON purchase_invoices(invoice_date, id DESC);
CREATE TABLE IF NOT EXISTS purchase_invoice_items (
    id INTEGER PRIMARY KEY,
    purchase_invoice_id INTEGER NOT NULL REFERENCES purchase_invoices(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id),
    unit TEXT NOT NULL DEFAULT 'قطعة',
    quantity_milli INTEGER NOT NULL CHECK (quantity_milli > 0),
    unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
    discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    line_total_minor INTEGER NOT NULL CHECK (line_total_minor >= 0)
);
CREATE INDEX IF NOT EXISTS idx_purchase_invoice_items_invoice ON purchase_invoice_items(purchase_invoice_id);

CREATE TABLE IF NOT EXISTS sales (
    id INTEGER PRIMARY KEY,
    invoice_no TEXT NOT NULL UNIQUE,
    customer_id INTEGER REFERENCES customers(id),
    invoice_date TEXT NOT NULL DEFAULT (date('now')),
    subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK (subtotal_minor >= 0),
    discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    total_minor INTEGER NOT NULL DEFAULT 0 CHECK (total_minor >= 0),
    tax_rate_bps INTEGER NOT NULL DEFAULT 0,
    tax_minor INTEGER NOT NULL DEFAULT 0,
    prices_include_tax INTEGER NOT NULL DEFAULT 0 CHECK (prices_include_tax IN (0,1)),
    paid_minor INTEGER NOT NULL DEFAULT 0 CHECK (paid_minor >= 0),
    notes TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft','posted','cancelled')),
    created_by INTEGER,
    created_by_username TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    posted_at TEXT,
    cancelled_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_sales_date ON sales(invoice_date, id DESC);
CREATE TABLE IF NOT EXISTS sale_items (
    id INTEGER PRIMARY KEY,
    sale_id INTEGER NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id),
    unit TEXT NOT NULL DEFAULT 'قطعة',
    quantity_milli INTEGER NOT NULL CHECK (quantity_milli > 0),
    unit_price_minor INTEGER NOT NULL CHECK (unit_price_minor >= 0),
    discount_minor INTEGER NOT NULL DEFAULT 0 CHECK (discount_minor >= 0),
    line_total_minor INTEGER NOT NULL CHECK (line_total_minor >= 0),
    cost_price_minor INTEGER
);
CREATE INDEX IF NOT EXISTS idx_sale_items_invoice ON sale_items(sale_id);

CREATE TABLE IF NOT EXISTS roles (id INTEGER PRIMARY KEY, code TEXT NOT NULL UNIQUE, name TEXT NOT NULL, is_system INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL DEFAULT (datetime('now')));
CREATE TABLE IF NOT EXISTS permissions (code TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '');
CREATE TABLE IF NOT EXISTS role_permissions (role_id INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE, permission_code TEXT NOT NULL REFERENCES permissions(code) ON DELETE CASCADE, PRIMARY KEY(role_id,permission_code));
CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE COLLATE NOCASE, display_name TEXT NOT NULL, password_hash TEXT NOT NULL, role_id INTEGER NOT NULL REFERENCES roles(id), is_active INTEGER NOT NULL DEFAULT 1 CHECK(is_active IN (0,1)), created_at TEXT NOT NULL DEFAULT (datetime('now')), updated_at TEXT NOT NULL DEFAULT (datetime('now')), last_login_at TEXT, email TEXT, phone TEXT, address TEXT, avatar_data_url TEXT);
CREATE TABLE IF NOT EXISTS user_permission_overrides (user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE, permission_code TEXT NOT NULL REFERENCES permissions(code) ON DELETE CASCADE, allowed INTEGER NOT NULL CHECK(allowed IN (0,1)), PRIMARY KEY(user_id,permission_code));
CREATE TABLE IF NOT EXISTS audit_log (id INTEGER PRIMARY KEY, action TEXT NOT NULL, entity_type TEXT NOT NULL, entity_id TEXT, details TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')), actor_user_id INTEGER, actor_username TEXT);
CREATE INDEX IF NOT EXISTS idx_audit_log_filters ON audit_log(entity_type, action, id DESC);


INSERT OR IGNORE INTO warehouses(code,name) VALUES ('MAIN', 'المخزن الرئيسي');

INSERT OR IGNORE INTO app_settings(key, value) VALUES
('app_name', 'مدار'),
('database_schema_version', '10'),
('barcode_prefix', 'MAD'),
('barcode_auto_generate', 'true'),
('backup_reminder_enabled', 'true'),
('company_tax_rate_bps','0'),
('company_prices_include_tax','false');


-- Commercial feature extension (v10): quotations, payments, cashbook, VAT snapshots and sale cost snapshots.
CREATE TABLE IF NOT EXISTS quotations (
    id INTEGER PRIMARY KEY, quote_no TEXT NOT NULL UNIQUE,
    customer_id INTEGER NOT NULL REFERENCES customers(id), quote_date TEXT NOT NULL, valid_until TEXT,
    subtotal_minor INTEGER NOT NULL DEFAULT 0 CHECK(subtotal_minor>=0), discount_minor INTEGER NOT NULL DEFAULT 0 CHECK(discount_minor>=0),
    total_minor INTEGER NOT NULL DEFAULT 0 CHECK(total_minor>=0),
    tax_rate_bps INTEGER NOT NULL DEFAULT 0, tax_minor INTEGER NOT NULL DEFAULT 0,
    prices_include_tax INTEGER NOT NULL DEFAULT 0 CHECK(prices_include_tax IN (0,1)), notes TEXT,
    status TEXT NOT NULL DEFAULT 'draft' CHECK(status IN ('draft','sent','accepted','rejected','expired','converted','cancelled')),
    converted_sale_id INTEGER, created_by INTEGER, created_by_username TEXT,
    created_at TEXT NOT NULL DEFAULT(datetime('now')), updated_at TEXT NOT NULL DEFAULT(datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_quotations_date ON quotations(quote_date,id DESC);
CREATE TABLE IF NOT EXISTS quotation_items (
    id INTEGER PRIMARY KEY, quotation_id INTEGER NOT NULL REFERENCES quotations(id) ON DELETE CASCADE,
    product_id INTEGER NOT NULL REFERENCES products(id), unit TEXT NOT NULL DEFAULT 'قطعة', quantity_milli INTEGER NOT NULL CHECK(quantity_milli>0),
    unit_price_minor INTEGER NOT NULL CHECK(unit_price_minor>=0), discount_minor INTEGER NOT NULL DEFAULT 0 CHECK(discount_minor>=0), line_total_minor INTEGER NOT NULL CHECK(line_total_minor>=0)
);
CREATE INDEX IF NOT EXISTS idx_quotation_items_quote ON quotation_items(quotation_id);
CREATE TABLE IF NOT EXISTS payments (
    id INTEGER PRIMARY KEY, invoice_type TEXT NOT NULL CHECK(invoice_type IN ('sale','purchase')), invoice_id INTEGER NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('in','out')), amount_minor INTEGER NOT NULL CHECK(amount_minor>0), payment_date TEXT NOT NULL,
    method TEXT NOT NULL DEFAULT 'cash' CHECK(method IN ('cash','bank','card','transfer','other')), reference_no TEXT, notes TEXT,
    created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_payments_invoice ON payments(invoice_type,invoice_id,id DESC);
CREATE INDEX IF NOT EXISTS idx_payments_date ON payments(payment_date,id DESC);
CREATE TABLE IF NOT EXISTS cashbook_entries (
    id INTEGER PRIMARY KEY, direction TEXT NOT NULL CHECK(direction IN ('in','out')), amount_minor INTEGER NOT NULL CHECK(amount_minor>0),
    entry_date TEXT NOT NULL, source_type TEXT NOT NULL, source_id INTEGER, description TEXT NOT NULL DEFAULT '', notes TEXT,
    created_by INTEGER, created_by_username TEXT, created_at TEXT NOT NULL DEFAULT(datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_cashbook_date ON cashbook_entries(entry_date,id DESC);

INSERT OR IGNORE INTO app_settings(key,value) VALUES ('company_name',''),('company_phone',''),('company_email',''),('company_address',''),('company_tax_number',''),('company_commercial_register',''),('company_website',''),('company_logo_data_url',''),('company_invoice_footer',''),('currency_code','EGP');
