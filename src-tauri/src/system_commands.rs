#[tauri::command]
fn clipboard_read_text(state: State<'_, ClipboardWorkerState>) -> Result<String, String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    state.sender.send(ClipboardRequest::ReadText(reply_tx)).map_err(|_| "خدمة الحافظة غير متاحة".to_string())?;
    match reply_rx.recv() { Ok(result) => result, Err(_) => Err("خدمة الحافظة لم تستجب".to_string()) }
}

#[tauri::command]
fn clipboard_write_text(text: String, state: State<'_, ClipboardWorkerState>) -> Result<(), String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    state.sender.send(ClipboardRequest::WriteText(text, reply_tx)).map_err(|_| "خدمة الحافظة غير متاحة".to_string())?;
    match reply_rx.recv() { Ok(result) => result, Err(_) => Err("خدمة الحافظة لم تستجب".to_string()) }
}

#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({"name":"مدار", "englishName":"Madar", "version":env!("CARGO_PKG_VERSION"), "database":"SQLite", "mode":"local", "barcodePolicy":"one primary barcode per product; preserve supplied barcode or generate internal barcode"})
}
