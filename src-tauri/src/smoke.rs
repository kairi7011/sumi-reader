use crate::{deliver_path, image, trusted};
use serde::Serialize;
use serde_json::Value;
use std::{path::PathBuf, sync::{atomic::{AtomicBool, Ordering}, Mutex}};
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_clipboard_manager::ClipboardExt;

pub struct Smoke {
    pub output: PathBuf,
    files: Vec<PathBuf>,
    scenario: Option<String>,
    started: AtomicBool,
    clipboard: Mutex<Option<(String, Option<tauri::image::Image<'static>>)>>,
}

impl Smoke {
    pub fn from_args() -> Result<Option<Self>, String> {
        let mut args = std::env::args().skip(1);
        let mut files = vec![];
        let mut scenario = None;
        let mut output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts/tauri-smoke");
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--smoke-scenario" => scenario = Some(args.next().ok_or("Missing scenario")?),
                "--smoke-file" => files.push(PathBuf::from(args.next().ok_or("Missing --smoke-file path")?)),
                "--smoke-output" => output = PathBuf::from(args.next().ok_or("Missing --smoke-output path")?),
                _ => {}
            }
        }
        if files.is_empty() { return Ok(None); }
        for path in &mut files { *path = std::fs::canonicalize(&path).map_err(|e| e.to_string())?; }
        std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
        Ok(Some(Self { output, files, scenario, started: AtomicBool::new(false), clipboard: Mutex::new(None) }))
    }
}

pub fn start(app: &AppHandle, window: &WebviewWindow) {
    let Some(smoke) = app.try_state::<Smoke>() else { return; };
    if smoke.started.swap(true, Ordering::AcqRel) { return; }
    let text = app.clipboard().read_text().unwrap_or_default();
    let original_image = app.clipboard().read_image().ok().map(|image| tauri::image::Image::new_owned(image.rgba().to_vec(), image.width(), image.height()));
    *smoke.clipboard.lock().unwrap() = Some((text, original_image));
    let script = if smoke.scenario.as_deref() == Some("page-map-race") { include_str!("../../tests/page-map-race.js") } else { include_str!("../../tests/tauri-smoke.js") };
    let _ = window.eval(script);
}

pub fn arm_timeout(app: &AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(180));
        if let Some(smoke) = handle.try_state::<Smoke>() {
            let _ = std::fs::write(smoke.output.join("timeout.txt"), "Native smoke exceeded 180 seconds");
        }
        handle.exit(1);
    });
}

#[tauri::command]
pub fn debug_log(window: WebviewWindow, message: String) -> Result<(), String> {
    trusted(&window)?;
    eprintln!("WKWebView: {}", message.chars().take(4000).collect::<String>());
    Ok(())
}

#[derive(Serialize)]
pub struct FixtureInfo { name: String }

#[tauri::command]
pub fn debug_smoke_info(app: AppHandle, window: WebviewWindow) -> Result<Vec<FixtureInfo>, String> {
    trusted(&window)?;
    let smoke = app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    Ok(smoke.files.iter().map(|path| FixtureInfo { name: path.file_name().unwrap().to_string_lossy().into_owned() }).collect())
}

#[tauri::command]
pub fn debug_open_fixture(app: AppHandle, window: WebviewWindow, index: usize) -> Result<(), String> {
    trusted(&window)?;
    let smoke = app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    let path = smoke.files.get(index).ok_or("Unknown fixture")?.clone();
    deliver_path(&app, path);
    Ok(())
}

#[tauri::command]
pub fn debug_resize_window(app: AppHandle, window: WebviewWindow, width: u32, height: u32) -> Result<(), String> {
    trusted(&window)?;
    app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    if !(860..=2000).contains(&width) || !(620..=1400).contains(&height) { return Err("Unsupported smoke viewport".into()); }
    window.set_size(tauri::LogicalSize::new(width as f64, height as f64)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn debug_clipboard_text(app: AppHandle, window: WebviewWindow) -> Result<String, String> {
    trusted(&window)?;
    app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    app.clipboard().read_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn debug_clipboard_image_size(app: AppHandle, window: WebviewWindow) -> Result<(u32, u32), String> {
    trusted(&window)?;
    app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    app.clipboard().read_image().map(|image| (image.width(), image.height())).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn debug_smoke_report(app: AppHandle, window: WebviewWindow, report: Value, snapshot: Option<String>) -> Result<(), String> {
    trusted(&window)?;
    let smoke = app.try_state::<Smoke>().ok_or("Smoke mode is not active")?;
    let json = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if json.len() > 1024 * 1024 { return Err("Smoke report is too large".into()); }
    std::fs::write(smoke.output.join("report.json"), json).map_err(|e| e.to_string())?;
    if let Some(snapshot) = snapshot { std::fs::write(smoke.output.join("fixture-page.png"), image::decode_png(&snapshot)?).map_err(|e| e.to_string())?; }
    if let Some((text, image)) = smoke.clipboard.lock().unwrap().take() {
        if let Some(image) = image { let _ = app.clipboard().write_image(&image); }
        else { let _ = app.clipboard().write_text(text); }
    }
    println!("{}", report);
    let code = if report["passed"].as_bool() == Some(true) { 0 } else { 1 };
    let handle = app.clone();
    std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(100)); handle.exit(code); });
    Ok(())
}
