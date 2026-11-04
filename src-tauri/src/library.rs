use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicU64, Ordering}, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

pub const MAX_PDF_BYTES: u64 = 250 * 1024 * 1024;
const MAX_LIBRARY_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: String,
    pub name: String,
    pub size: u64,
    pub modified_at: f64,
    pub opened_at: f64,
    #[serde(default = "empty_state")]
    pub state: Value,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub document: Document,
    pub path: PathBuf,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub version: u8,
    pub recent: Vec<Entry>,
    pub last_opened_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLibrary {
    pub recent: Vec<Document>,
    pub last_opened_id: Option<String>,
}

pub struct Store {
    pub library: Mutex<Library>,
    pub path: PathBuf,
    pub dialog_open: AtomicBool,
    pub frontend_ready: AtomicBool,
    pub pending_files: Mutex<Vec<PathBuf>>,
    pub pending_command: Mutex<Option<String>>,
    pub open_intent: AtomicU64,
    pub startup_warning: Mutex<Option<String>>,
}

fn empty_state() -> Value { Value::Object(Map::new()) }

impl Default for Library {
    fn default() -> Self { Self { version: 1, recent: vec![], last_opened_id: None } }
}

fn read_library(path: &Path) -> Result<Library, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.take(MAX_LIBRARY_BYTES + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_LIBRARY_BYTES { return Err("本棚のデータが大きすぎます。".into()); }
    let mut library: Library = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if library.version != 1 { return Err("本棚のデータ形式を読み込めません。".into()); }
    library.recent.retain(|entry| entry.document.id.len() == 64 && entry.document.id.bytes().all(|c| c.is_ascii_hexdigit()) && entry.path.is_absolute());
    library.recent.truncate(40);
    for entry in &mut library.recent {
        entry.document.state = sanitize_state(&entry.document.state).unwrap_or_else(|_| empty_state());
    }
    Ok(library)
}

impl Store {
    pub fn load(path: PathBuf, legacy: Option<&Path>) -> Result<Self, String> {
        let mut imported = false;
        let mut warning = None;
        let library = if path.exists() {
            match read_library(&path) {
                Ok(library) => library,
                Err(_) => {
                    let backup = path.with_file_name(format!("library.corrupt-{}.json", milliseconds(SystemTime::now()) as u64));
                    fs::rename(&path, backup).map_err(|e| e.to_string())?;
                    warning = Some("本棚の記録を読み込めなかったため、新しい本棚を開きました。元の記録はバックアップに残しています。".into());
                    Library::default()
                }
            }
        } else if let Some(legacy) = legacy.filter(|legacy| legacy.exists()) {
            match read_library(legacy) {
                Ok(library) => { imported = true; library }
                Err(_) => {
                    warning = Some("以前の本棚を読み込めなかったため、新しい本棚を開きました。以前の記録はそのまま残しています。".into());
                    Library::default()
                }
            }
        } else { Library::default() };
        let store = Self { library: Mutex::new(library), path, dialog_open: AtomicBool::new(false), frontend_ready: AtomicBool::new(false), pending_files: Mutex::new(vec![]), pending_command: Mutex::new(None), open_intent: AtomicU64::new(0), startup_warning: Mutex::new(warning) };
        if imported {
            let library = store.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
            store.persist(&library)?;
        }
        Ok(store)
    }

    fn persist(&self, library: &Library) -> Result<(), String> {
        let parent = self.path.parent().ok_or("保存先が見つかりません。")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        let data = serde_json::to_vec(library).map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temporary).map_err(|e| e.to_string())?;
        file.write_all(&data).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(temporary, &self.path).map_err(|e| e.to_string())
    }

    pub fn public_library(&self) -> Result<PublicLibrary, String> {
        let library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        Ok(PublicLibrary { recent: library.recent.iter().map(|entry| entry.document.clone()).collect(), last_opened_id: library.last_opened_id.clone() })
    }

    #[cfg(test)]
    pub fn open_authorized_path(&self, path: &Path) -> Result<Document, String> {
        self.open_path(path, None)?.ok_or_else(|| "開く操作が置き換えられました。".into())
    }

    pub fn open_for_intent(&self, path: &Path, intent: u64) -> Result<Option<Document>, String> {
        self.open_path(path, Some(intent))
    }

    fn open_path(&self, path: &Path, intent: Option<u64>) -> Result<Option<Document>, String> {
        if !path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
            return Err("PDFファイルを選んでください。".into());
        }
        let canonical = fs::canonicalize(path).map_err(|_| "ファイルが見つかりません。移動した場合は、もう一度開いてください。")?;
        let mut file = File::open(&canonical).map_err(|_| "PDFを開けませんでした。アクセス権を確認してください。")?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() { return Err("PDFファイルを選んでください。".into()); }
        if metadata.len() > MAX_PDF_BYTES { return Err("このPDFは250 MBを超えています。小さなファイルに分けて開いてください。".into()); }
        let mut header = [0u8; 1024];
        let length = file.read(&mut header).map_err(|e| e.to_string())?;
        if !header[..length].windows(5).any(|part| part == b"%PDF-") { return Err("PDF形式として読み込めませんでした。".into()); }
        let canonical_text = canonical.to_str().ok_or("ファイル名を読み込めませんでした。")?;
        let id: String = Sha256::digest(canonical_text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
        let mut library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        if intent.is_some_and(|intent| intent != self.open_intent.load(Ordering::Acquire)) { return Ok(None); }
        let state = library.recent.iter().find(|entry| entry.document.id == id || entry.path == canonical).map(|entry| entry.document.state.clone()).unwrap_or_else(empty_state);
        let document = Document {
            id: id.clone(), name: canonical.file_name().and_then(|name| name.to_str()).ok_or("ファイル名を読み込めませんでした。")?.into(), size: metadata.len(),
            modified_at: milliseconds(metadata.modified().unwrap_or(UNIX_EPOCH)), opened_at: milliseconds(SystemTime::now()), state,
        };
        library.recent.retain(|entry| entry.document.id != id);
        library.recent.insert(0, Entry { document: document.clone(), path: canonical });
        library.recent.truncate(40);
        library.last_opened_id = Some(id);
        self.persist(&library)?;
        Ok(Some(document))
    }

    pub fn authorized_path(&self, id: &str) -> Result<PathBuf, String> {
        let library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        let path = library.recent.iter().find(|entry| entry.document.id == id).map(|entry| entry.path.clone()).ok_or("履歴にあるPDFを選んでください。")?;
        let canonical = fs::canonicalize(&path).map_err(|_| "ファイルが見つかりません。移動した場合は、もう一度開いてください。")?;
        if canonical != path { return Err("ファイルの場所が変わりました。「開く」から選び直してください。".into()); }
        Ok(path)
    }

    pub fn read_pdf(&self, id: &str) -> Result<Vec<u8>, String> {
        let path = self.authorized_path(id)?;
        let file = File::open(path).map_err(|_| "PDFを読み込めませんでした。")?;
        let mut bytes = Vec::new();
        file.take(MAX_PDF_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_PDF_BYTES { return Err("このPDFは250 MBを超えています。".into()); }
        if !bytes[..bytes.len().min(1024)].windows(5).any(|part| part == b"%PDF-") { return Err("PDF形式として読み込めませんでした。".into()); }
        Ok(bytes)
    }

    pub fn save_state(&self, id: &str, state: &Value) -> Result<(), String> {
        let state = sanitize_state(state)?;
        let mut library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        let entry = library.recent.iter_mut().find(|entry| entry.document.id == id).ok_or("このPDFは開かれていません。")?;
        entry.document.state = state;
        self.persist(&library)
    }
}

fn milliseconds(time: SystemTime) -> f64 { time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() * 1000.0 }

pub fn sanitize_state(value: &Value) -> Result<Value, String> {
    let object = value.as_object().ok_or("読書設定を読み込めませんでした。")?;
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 64 * 1024 { return Err("読書設定が大きすぎます。".into()); }
    let mut result = Map::new();
    for key in ["page", "zoom", "fit", "mode", "direction", "theme", "bookmarks"] {
        if let Some(value) = object.get(key) { result.insert(key.into(), value.clone()); }
    }
    if result.get("page").is_some_and(|v| !v.as_u64().is_some_and(|n| (1..=1_000_000).contains(&n))) { return Err("ページ番号が正しくありません。".into()); }
    if result.get("zoom").is_some_and(|v| !v.as_f64().is_some_and(|n| (0.1..=10.0).contains(&n))) { return Err("倍率が正しくありません。".into()); }
    for key in ["fit", "mode", "direction", "theme"] {
        if result.get(key).is_some_and(|v| !v.as_str().is_some_and(|s| s.len() <= 32)) { return Err("読書設定が正しくありません。".into()); }
    }
    if result.get("bookmarks").is_some_and(|v| !v.as_array().is_some_and(|a| a.len() <= 2000 && a.iter().all(|p| p.as_u64().is_some_and(|n| (1..=1_000_000).contains(&n))))) { return Err("しおりが正しくありません。".into()); }
    Ok(Value::Object(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_rejects_invalid_values_and_discards_unrelated_fields() {
        assert_eq!(sanitize_state(&json!({"page": 4, "bookmarks": [2], "path": "/private"})).unwrap(), json!({"page": 4, "bookmarks": [2]}));
        for state in [json!({"page": 0}), json!({"zoom": 11}), json!({"bookmarks": ["2"]}), json!({"theme": {}})] { assert!(sanitize_state(&state).is_err()); }
    }

    #[test]
    fn imports_electron_history_and_reads_only_authorized_documents() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-library-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let pdf = root.join("sample.pdf");
        fs::write(&pdf, b"%PDF-1.7\nfixture").unwrap();
        let legacy = Store::load(root.join("electron/library.json"), None).unwrap();
        let document = legacy.open_authorized_path(&pdf).unwrap();
        legacy.save_state(&document.id, &json!({"page": 3, "bookmarks": [1, 3]})).unwrap();
        let native = Store::load(root.join("tauri/library.json"), Some(&legacy.path)).unwrap();
        assert_eq!(native.public_library().unwrap().recent[0].state["page"], 3);
        assert!(native.read_pdf("unknown").is_err());
        assert!(native.read_pdf(&document.id).unwrap().starts_with(b"%PDF"));
        fs::remove_file(pdf).unwrap();
        assert!(native.read_pdf(&document.id).unwrap_err().contains("見つかりません"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn superseded_open_does_not_overwrite_the_last_document() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-open-intent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let first = root.join("first.pdf");
        let latest = root.join("latest.pdf");
        fs::write(&first, b"%PDF-1.7\nfirst").unwrap();
        fs::write(&latest, b"%PDF-1.7\nlatest").unwrap();
        let store = Store::load(root.join("library.json"), None).unwrap();
        store.open_intent.store(2, Ordering::Release);
        let document = store.open_for_intent(&latest, 2).unwrap().unwrap();
        assert!(store.open_for_intent(&first, 1).unwrap().is_none());
        let saved = Store::load(store.path.clone(), None).unwrap().public_library().unwrap();
        assert_eq!(saved.last_opened_id.as_deref(), Some(document.id.as_str()));
        assert_eq!(saved.recent.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_current_and_legacy_histories_do_not_prevent_startup() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let current = root.join("library.json");
        fs::write(&current, "{broken").unwrap();
        let recovered = Store::load(current.clone(), None).unwrap();
        assert!(recovered.public_library().unwrap().recent.is_empty());
        assert!(recovered.startup_warning.lock().unwrap().is_some());
        assert!(!current.exists());
        assert!(fs::read_dir(&root).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with("library.corrupt-")));
        let legacy = root.join("legacy.json");
        fs::write(&legacy, "{broken legacy").unwrap();
        let recovered = Store::load(current, Some(&legacy)).unwrap();
        assert!(recovered.public_library().unwrap().recent.is_empty());
        assert!(recovered.startup_warning.lock().unwrap().is_some());
        assert_eq!(fs::read_to_string(&legacy).unwrap(), "{broken legacy");
        fs::remove_dir_all(root).unwrap();
    }
}
