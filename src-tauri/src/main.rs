mod image;
mod library;
#[cfg(debug_assertions)]
mod smoke;

use library::{Document, PublicLibrary, Store};
use serde_json::Value;
use std::{path::PathBuf, sync::atomic::Ordering};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    webview::{NewWindowResponse, PageLoadEvent, PermissionResponse},
    AppHandle, DragDropEvent, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

fn trusted(window: &WebviewWindow) -> Result<(), String> {
    let url = window.url().map_err(|e| e.to_string())?;
    if window.label() == "main" && url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.path() == "/src/index.html" {
        Ok(())
    } else { Err("この画面からは操作できません。".into()) }
}

struct DialogGuard<'a>(&'a std::sync::atomic::AtomicBool);
impl Drop for DialogGuard<'_> { fn drop(&mut self) { self.0.store(false, Ordering::Release); } }

#[tauri::command]
async fn open_pdf(app: AppHandle, window: WebviewWindow) -> Result<Option<Document>, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<Store>();
        if store.dialog_open.swap(true, Ordering::AcqRel) { return Ok(None); }
        let _guard = DialogGuard(&store.dialog_open);
        let choice = app.dialog().file().set_parent(&window).set_title("PDFを開く").add_filter("PDF", &["pdf"]).blocking_pick_file();
        match choice {
            Some(file) => {
                let intent = store.open_intent.fetch_add(1, Ordering::AcqRel) + 1;
                store.open_for_intent(&file.into_path().map_err(|e| e.to_string())?, intent)
            },
            None => Ok(None),
        }
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn reopen_pdf(app: AppHandle, window: WebviewWindow, id: String) -> Result<Option<Document>, String> {
    trusted(&window)?;
    let intent = app.state::<Store>().open_intent.fetch_add(1, Ordering::AcqRel) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let store = app.state::<Store>();
        let path = store.authorized_path(&id)?;
        store.open_for_intent(&path, intent)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn read_pdf(app: AppHandle, window: WebviewWindow, id: String) -> Result<tauri::ipc::Response, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || app.state::<Store>().read_pdf(&id).map(tauri::ipc::Response::new)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_library(app: AppHandle, window: WebviewWindow) -> Result<PublicLibrary, String> {
    trusted(&window)?;
    app.state::<Store>().public_library()
}

#[tauri::command]
fn save_reading_state(app: AppHandle, window: WebviewWindow, id: String, state: Value) -> Result<(), String> {
    trusted(&window)?;
    // Complete the small atomic write before acknowledging so close/quit cannot discard a queued save.
    app.state::<Store>().save_state(&id, &state)
}

#[tauri::command]
async fn copy_text(app: AppHandle, window: WebviewWindow, text: String) -> Result<(), String> {
    trusted(&window)?;
    if text.len() > 8 * 1024 * 1024 { return Err("コピーする文章が長すぎます。".into()); }
    tauri::async_runtime::spawn_blocking(move || app.clipboard().write_text(text).map_err(|e| e.to_string())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn copy_image(app: AppHandle, window: WebviewWindow, data_url: String) -> Result<(), String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = image::decode_png(&data_url)?;
        let image = tauri::image::Image::from_bytes(&bytes).map_err(|_| "PNG画像を読み込めませんでした。")?;
        app.clipboard().write_image(&image).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn save_image(app: AppHandle, window: WebviewWindow, data_url: String, suggested_name: String) -> Result<bool, String> {
    trusted(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = image::decode_png(&data_url)?;
        let store = app.state::<Store>();
        if store.dialog_open.swap(true, Ordering::AcqRel) { return Ok(false); }
        let _guard = DialogGuard(&store.dialog_open);
        let name = PathBuf::from(suggested_name);
        let name: String = name.file_name().and_then(|n| n.to_str()).unwrap_or("Sumi-capture.png").chars().filter(|c| !c.is_control()).take(180).collect();
        let name = if name.ends_with(".png") { name } else { format!("{name}.png") };
        let choice = app.dialog().file().set_parent(&window).set_title("画像を保存").set_file_name(name).add_filter("PNG画像", &["png"]).blocking_save_file();
        let Some(file) = choice else { return Ok(false); };
        std::fs::write(file.into_path().map_err(|e| e.to_string())?, bytes).map_err(|e| e.to_string())?;
        Ok(true)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn toggle_fullscreen(window: WebviewWindow) -> Result<(), String> {
    trusted(&window)?;
    window.set_fullscreen(!window.is_fullscreen().map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
fn frontend_ready(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    trusted(&window)?;
    let store = app.state::<Store>();
    store.frontend_ready.store(true, Ordering::Release);
    let pending = std::mem::take(&mut *store.pending_files.lock().map_err(|_| "ファイルを開けませんでした。")?);
    window.show().map_err(|e| e.to_string())?;
    if let Some(warning) = store.startup_warning.lock().map_err(|_| "本棚を読み込めませんでした。")?.take() {
        app.emit_to("main", "pdf-error", warning).map_err(|e| e.to_string())?;
    }
    for path in pending { deliver_path(&app, path); }
    if let Some(command) = store.pending_command.lock().map_err(|_| "操作を再開できませんでした。")?.take() {
        app.emit_to("main", "app-command", command).map_err(|e| e.to_string())?;
    }
    #[cfg(debug_assertions)]
    smoke::start(&app, &window);
    Ok(())
}

fn deliver_path(app: &AppHandle, path: PathBuf) {
    let store = app.state::<Store>();
    let intent = store.open_intent.fetch_add(1, Ordering::AcqRel) + 1;
    if !store.frontend_ready.load(Ordering::Acquire) {
        if let Ok(mut pending) = store.pending_files.lock() {
            pending.clear();
            pending.push(path);
        }
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let store = handle.state::<Store>();
        if intent != store.open_intent.load(Ordering::Acquire) { return; }
        let result = store.open_for_intent(&path, intent);
        if intent != store.open_intent.load(Ordering::Acquire) { return; }
        match result {
            Ok(Some(document)) => { let _ = handle.emit_to("main", "pdf-opened", document); }
            Ok(None) => return,
            Err(error) => { let _ = handle.emit_to("main", "pdf-error", error); }
        }
        if let Some(window) = handle.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); }
    });
}

fn ensure_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("main") { window.show()?; window.set_focus()?; return Ok(window); }
    app.state::<Store>().frontend_ready.store(false, Ordering::Release);
    let builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("src/index.html".into()))
        .title("Sumi").inner_size(1380.0, 940.0).min_inner_size(860.0, 620.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(20.0, 11.0))
        .visible(false)
        .on_navigation(|url| url.scheme() == "tauri" && url.host_str() == Some("localhost") && url.path() == "/src/index.html")
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_permission_request(|_, _| PermissionResponse::Deny)
        .on_page_load(|window, payload| {
            if matches!(payload.event(), PageLoadEvent::Started) { window.state::<Store>().frontend_ready.store(false, Ordering::Release); }
        });
    #[cfg(debug_assertions)]
    let builder = builder.initialization_script(r#"(() => { const log = value => window.__TAURI__?.core.invoke('debug_log', {message: String(value)}).catch(() => {}); window.addEventListener('error', e => log(e.message)); window.addEventListener('unhandledrejection', e => log(e.reason?.message || e.reason)); const original = console.error.bind(console); console.error = (...args) => { original(...args); log(args.map(value => value?.stack ? String(value.message) + '\n' + value.stack : value?.message || String(value)).join(' ')); }; })();"#);
    let window = builder.build()?;
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        tauri::WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
            let _ = handle.emit_to("main", "file-drag", false);
            if let Some(path) = paths.iter().find(|path| path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("pdf"))) { deliver_path(&handle, path.clone()); }
            else { let _ = handle.emit_to("main", "pdf-error", "PDFファイルをドロップしてください。"); }
        }
        tauri::WindowEvent::DragDrop(DragDropEvent::Enter { .. }) => { let _ = handle.emit_to("main", "file-drag", true); }
        tauri::WindowEvent::DragDrop(DragDropEvent::Leave) => { let _ = handle.emit_to("main", "file-drag", false); }
        tauri::WindowEvent::Destroyed => { handle.state::<Store>().frontend_ready.store(false, Ordering::Release); }
        _ => {}
    });
    Ok(window)
}

fn make_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let app_menu = Submenu::with_items(app, "Sumi", true, &[
        &PredefinedMenuItem::about(app, Some("Sumiについて"), None)?,
        &PredefinedMenuItem::separator(app)?,
        &PredefinedMenuItem::hide(app, Some("Sumiを隠す"))?,
        &PredefinedMenuItem::hide_others(app, Some("ほかを隠す"))?,
        &PredefinedMenuItem::show_all(app, Some("すべて表示"))?,
        &PredefinedMenuItem::separator(app)?,
        &PredefinedMenuItem::quit(app, Some("Sumiを終了"))?,
    ])?;
    let file = Submenu::with_items(app, "ファイル", true, &[
        &MenuItem::with_id(app, "open", "PDFを開く…", true, Some("CmdOrCtrl+O"))?,
        &PredefinedMenuItem::separator(app)?, &PredefinedMenuItem::close_window(app, Some("ウインドウを閉じる"))?,
    ])?;
    let edit = Submenu::with_items(app, "編集", true, &[
        &PredefinedMenuItem::undo(app, Some("元に戻す"))?, &PredefinedMenuItem::redo(app, Some("やり直す"))?,
        &PredefinedMenuItem::separator(app)?, &PredefinedMenuItem::cut(app, Some("カット"))?,
        &PredefinedMenuItem::copy(app, Some("コピー"))?, &PredefinedMenuItem::paste(app, Some("ペースト"))?,
        &PredefinedMenuItem::select_all(app, Some("すべて選択"))?, &PredefinedMenuItem::separator(app)?,
        &MenuItem::with_id(app, "find", "PDF内を検索", true, Some("CmdOrCtrl+F"))?,
    ])?;
    let view = Submenu::with_items(app, "表示", true, &[
        &MenuItem::with_id(app, "focus", "集中モード", true, Some("CmdOrCtrl+Shift+F"))?,
        &MenuItem::with_id(app, "capture", "範囲を画像でコピー", true, Some("CmdOrCtrl+Shift+X"))?,
        &PredefinedMenuItem::separator(app)?, &PredefinedMenuItem::fullscreen(app, Some("フルスクリーン"))?,
    ])?;
    #[cfg(debug_assertions)]
    view.append(&MenuItem::with_id(app, "inspector", "Webインスペクタ", true, Some("CmdOrCtrl+Alt+I"))?)?;
    let window = Submenu::with_items(app, "ウインドウ", true, &[
        &PredefinedMenuItem::minimize(app, Some("しまう"))?, &PredefinedMenuItem::maximize(app, Some("拡大／縮小"))?,
        &PredefinedMenuItem::separator(app)?, &PredefinedMenuItem::bring_all_to_front(app, Some("すべてを手前に移動"))?,
    ])?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &window])
}

fn main() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init());
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(tauri::generate_handler![open_pdf, reopen_pdf, read_pdf, get_library, save_reading_state, copy_text, copy_image, save_image, toggle_fullscreen, frontend_ready]);
    #[cfg(debug_assertions)]
    let builder = builder.invoke_handler(tauri::generate_handler![open_pdf, reopen_pdf, read_pdf, get_library, save_reading_state, copy_text, copy_image, save_image, toggle_fullscreen, frontend_ready, smoke::debug_smoke_info, smoke::debug_open_fixture, smoke::debug_resize_window, smoke::debug_clipboard_text, smoke::debug_clipboard_image_size, smoke::debug_smoke_report, smoke::debug_log]);
    let application = builder
        .setup(|app| {
            let mut data = app.path().app_data_dir()?.join("library.json");
            let legacy = app.path().home_dir()?.join("Library/Application Support/Sumi/library.json");
            let mut legacy = Some(legacy.as_path());
            #[cfg(debug_assertions)]
            if let Some(smoke) = smoke::Smoke::from_args().map_err(std::io::Error::other)? {
                data = smoke.output.join(format!("user-data/library-{}.json", std::process::id()));
                legacy = None;
                app.manage(smoke);
                smoke::arm_timeout(app.handle());
            }
            let store = Store::load(data, legacy).map_err(std::io::Error::other)?;
            app.manage(store);
            app.set_menu(make_menu(app.handle())?)?;
            ensure_window(app.handle())?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            let command = event.id().as_ref();
            #[cfg(debug_assertions)]
            if command == "inspector" {
                if let Some(window) = app.get_webview_window("main") { window.open_devtools(); }
            }
            if ["open", "find", "focus", "capture"].contains(&command) {
                if ensure_window(app).is_ok() {
                    let store = app.state::<Store>();
                    if store.frontend_ready.load(Ordering::Acquire) { let _ = app.emit_to("main", "app-command", command); }
                    else if let Ok(mut pending) = store.pending_command.lock() { *pending = Some(command.into()); }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Sumiを起動できませんでした。");
    application.run(|app, event| match event {
        tauri::RunEvent::Opened { urls } => {
            let _ = ensure_window(app);
            if let Some(path) = urls.into_iter().filter_map(|url| url.to_file_path().ok()).next() { deliver_path(app, path); }
        }
        tauri::RunEvent::Reopen { .. } => { let _ = ensure_window(app); }
        tauri::RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        _ => {}
    });
}
