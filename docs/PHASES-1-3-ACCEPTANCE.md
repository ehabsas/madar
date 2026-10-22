# Acceptance checklist — Customers, Suppliers, Purchases and Sales

Use a disposable test database first. Never run the first end-to-end test against the only copy of live business data.

## 1. Customers and suppliers

1. Sign in as a user with the appropriate `customers.create` or `suppliers.create` permission.
2. Add a customer and supplier with unique codes; include email, phone, address and notes.
3. Search separately by code, name, email and phone; edit a record and confirm the new values persist after restarting Madar.
4. Deactivate and reactivate a customer/supplier. Confirm the record is retained, and an inactive party cannot be used for a new invoice.
5. Sign in with a restricted account and confirm the Rust command denies actions without the corresponding permission.

## 2. Purchase invoices

1. Create a supplier and an active product. Record its current stock balance.
2. Create a purchase invoice with quantity `10`; save it as a draft. Confirm stock has not changed.
3. Reopen and edit the draft, change supplier/date/quantity/price, save and confirm it remains a draft.
4. Post the draft after confirmation. Confirm the inventory increases exactly once and the invoice is `posted`.
5. Try to edit a posted invoice; it must be disallowed. Cancel a posted purchase invoice in a disposable database with enough remaining stock; confirm a reversing movement is recorded and the invoice row remains.

## 3. Sales invoices

1. Starting with at least `10` units on hand, create a sales invoice for `3.5` units and save as draft. Confirm stock does not change.
2. Post it; confirm stock becomes `6.5` and the audit log records both invoice posting and the stock movement with the acting user.
3. Attempt to post a sale for `7` units while only `6.5` remain; posting must fail and the balance must remain `6.5`.
4. Cancel the posted sale; confirm a positive reversing movement is added and the invoice remains in the register.
5. Test barcode scanning in the invoice modal: scan a primary and an alternate barcode. The matching product should be added; scanning the same product again should increment its quantity by one.
6. With an account lacking `sales.price.override`, confirm its sale-price field is read-only and the Rust command rejects any changed sales price submitted directly.

## Expected safeguards

- Invoice numbers are unique (`PUR-######` and `SAL-######`).
- `draft` invoices do not affect stock; only posting does.
- Posting and all corresponding stock movement/audit rows happen in a single SQLite transaction.
- Cancelling a posted invoice preserves the original invoice and records reversing movement rows; it never deletes history.
- Barcode values remain text so leading zeros are preserved.
- Use `PRAGMA foreign_key_check` and `PRAGMA quick_check` against the test database.
