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
    #[serde(default)]
    pub title: String,
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
    #[serde(default)]
    pub preferences: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ReaderPreferences {
    pub theme: String,
    pub mode: String,
    pub direction: String,
}

impl Default for ReaderPreferences {
    fn default() -> Self { Self { theme: "paper".into(), mode: "single".into(), direction: "ltr".into() } }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicLibrary {
    pub recent: Vec<Document>,
    pub last_opened_id: Option<String>,
    pub preferences: ReaderPreferences,
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
    fn default() -> Self { Self { version: 1, recent: vec![], last_opened_id: None, preferences: Some(serde_json::to_value(ReaderPreferences::default()).unwrap()) } }
}

fn read_library(path: &Path) -> Result<(Library, bool), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    file.take(MAX_LIBRARY_BYTES + 1).read_to_end(&mut data).map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_LIBRARY_BYTES { return Err("本棚のデータが大きすぎます。".into()); }
    let mut library: Library = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if library.version != 1 { return Err("本棚のデータ形式を読み込めません。".into()); }
    library.recent.retain(|entry| entry.document.id.len() == 64 && entry.document.id.bytes().all(|c| c.is_ascii_hexdigit()) && entry.path.is_absolute());
    library.recent.truncate(40);
    let migrated = library.preferences.is_none();
    let preferences = if let Some(value) = &library.preferences {
        sanitize_preferences(value, &ReaderPreferences::default()).unwrap_or_default()
    } else {
        let last = library.recent.iter().find(|entry| Some(&entry.document.id) == library.last_opened_id.as_ref()).or_else(|| library.recent.first());
        let mut preferences = ReaderPreferences::default();
        if let Some(entry) = last {
            for key in ["theme", "mode", "direction"] {
                if let Some(value) = entry.document.state.get(key) {
                    let mut partial = Map::new(); partial.insert(key.into(), value.clone());
                    if let Ok(valid) = sanitize_preferences(&Value::Object(partial), &preferences) { preferences = valid; }
                }
            }
        }
        preferences
    };
    library.preferences = Some(serde_json::to_value(preferences).map_err(|e| e.to_string())?);
    for entry in &mut library.recent {
        entry.document.state = sanitize_state(&entry.document.state).unwrap_or_else(|_| empty_state());
        entry.document.title = sanitize_title(&entry.document.title).unwrap_or_else(|_| entry.document.name.clone());
    }
    Ok((library, migrated))
}

impl Store {
    pub fn load(path: PathBuf, legacy: Option<&Path>) -> Result<Self, String> {
        let mut persist_on_load = false;
        let mut warning = None;
        let library = if path.exists() {
            match read_library(&path) {
                Ok((library, migrated)) => { persist_on_load = migrated; library },
                Err(_) => {
                    let backup = path.with_file_name(format!("library.corrupt-{}.json", milliseconds(SystemTime::now()) as u64));
                    fs::rename(&path, backup).map_err(|e| e.to_string())?;
                    warning = Some("本棚の記録を読み込めなかったため、新しい本棚を開きました。元の記録はバックアップに残しています。".into());
                    persist_on_load = true;
                    Library::default()
                }
            }
        } else if let Some(legacy) = legacy.filter(|legacy| legacy.exists()) {
            match read_library(legacy) {
                Ok((library, _)) => { persist_on_load = true; library }
                Err(_) => {
                    warning = Some("以前の本棚を読み込めなかったため、新しい本棚を開きました。以前の記録はそのまま残しています。".into());
                    persist_on_load = true;
                    Library::default()
                }
            }
        } else { Library::default() };
        let store = Self { library: Mutex::new(library), path, dialog_open: AtomicBool::new(false), frontend_ready: AtomicBool::new(false), pending_files: Mutex::new(vec![]), pending_command: Mutex::new(None), open_intent: AtomicU64::new(0), startup_warning: Mutex::new(warning) };
        if persist_on_load {
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
        let preferences = library.preferences.as_ref().and_then(|value| sanitize_preferences(value, &ReaderPreferences::default()).ok()).unwrap_or_default();
        Ok(PublicLibrary { recent: library.recent.iter().map(|entry| entry.document.clone()).collect(), last_opened_id: library.last_opened_id.clone(), preferences })
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
        let previous = library.recent.iter().find(|entry| entry.document.id == id || entry.path == canonical);
        let state = previous.map(|entry| entry.document.state.clone()).unwrap_or_else(empty_state);
        let name: String = canonical.file_name().and_then(|name| name.to_str()).ok_or("ファイル名を読み込めませんでした。")?.into();
        let title = previous.map(|entry| entry.document.title.clone()).filter(|title| !title.is_empty()).unwrap_or_else(|| name.clone());
        let document = Document {
            id: id.clone(), name, title, size: metadata.len(),
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

    pub fn save_preferences(&self, update: &Value) -> Result<ReaderPreferences, String> {
        let mut library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        let existing = library.preferences.as_ref().and_then(|value| sanitize_preferences(value, &ReaderPreferences::default()).ok()).unwrap_or_default();
        let preferences = sanitize_preferences(update, &existing)?;
        library.preferences = Some(serde_json::to_value(&preferences).map_err(|e| e.to_string())?);
        self.persist(&library)?;
        Ok(preferences)
    }

    pub fn set_book_title(&self, id: &str, title: &str) -> Result<(), String> {
        let title = sanitize_title(title)?;
        let mut library = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        let entry = library.recent.iter_mut().find(|entry| entry.document.id == id).ok_or("このPDFは開かれていません。")?;
        entry.document.title = title;
        self.persist(&library)
    }
}

fn milliseconds(time: SystemTime) -> f64 { time.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64() * 1000.0 }

pub fn sanitize_state(value: &Value) -> Result<Value, String> {
    let object = value.as_object().ok_or("読書設定を読み込めませんでした。")?;
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 64 * 1024 { return Err("読書設定が大きすぎます。".into()); }
    let mut result = Map::new();
    for key in ["page", "bookmarks"] {
        if let Some(value) = object.get(key) { result.insert(key.into(), value.clone()); }
    }
    if result.get("page").is_some_and(|v| !v.as_u64().is_some_and(|n| (1..=1_000_000).contains(&n))) { return Err("ページ番号が正しくありません。".into()); }
    if result.get("bookmarks").is_some_and(|v| !v.as_array().is_some_and(|a| a.len() <= 2000 && a.iter().all(|p| p.as_u64().is_some_and(|n| (1..=1_000_000).contains(&n))))) { return Err("しおりが正しくありません。".into()); }
    Ok(Value::Object(result))
}

pub fn sanitize_preferences(value: &Value, existing: &ReaderPreferences) -> Result<ReaderPreferences, String> {
    let object = value.as_object().ok_or("表示設定を読み込めませんでした。")?;
    if serde_json::to_vec(value).map_err(|e| e.to_string())?.len() > 2048 { return Err("表示設定が大きすぎます。".into()); }
    let mut preferences = existing.clone();
    for (key, choices) in [("theme", &["paper", "white", "night"][..]), ("mode", &["single", "spread"][..]), ("direction", &["ltr", "rtl"][..])] {
        if let Some(value) = object.get(key) {
            let selected = value.as_str().filter(|selected| choices.contains(selected)).ok_or("表示設定が正しくありません。")?;
            match key { "theme" => preferences.theme = selected.into(), "mode" => preferences.mode = selected.into(), _ => preferences.direction = selected.into() }
        }
    }
    Ok(preferences)
}

fn sanitize_title(value: &str) -> Result<String, String> {
    if value.len() > 4000 { return Err("書名が長すぎます。".into()); }
    let text: String = value.chars().filter(|character| !character.is_control() || character.is_whitespace()).collect();
    let title = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() || title.chars().count() > 500 { return Err("書名を読み込めませんでした。".into()); }
    Ok(title)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn state_rejects_invalid_values_and_discards_unrelated_fields() {
        assert_eq!(sanitize_state(&json!({"page": 4, "bookmarks": [2], "path": "/private", "theme": "night", "mode": "spread", "zoom": 2})).unwrap(), json!({"page": 4, "bookmarks": [2]}));
        for state in [json!({"page": 0}), json!({"page": "4"}), json!({"bookmarks": ["2"]}), json!({"bookmarks": [0]})] { assert!(sanitize_state(&state).is_err()); }
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
    fn migrates_last_read_appearance_once_and_keeps_future_books_independent() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-preferences-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let first = root.join("first.pdf"); let second = root.join("second.pdf");
        fs::write(&first, b"%PDF-1.7\nfirst").unwrap(); fs::write(&second, b"%PDF-1.7\nsecond").unwrap();
        let store = Store::load(root.join("library.json"), None).unwrap();
        let first_document = store.open_authorized_path(&first).unwrap();
        let second_document = store.open_authorized_path(&second).unwrap();
        let mut legacy: Value = serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("preferences");
        legacy["lastOpenedId"] = json!(first_document.id);
        legacy["recent"][0]["state"] = json!({"page": 2, "theme": "white", "mode": "single"});
        legacy["recent"][1]["state"] = json!({"page": 7, "bookmarks": [2, 7], "theme": "night", "mode": "spread", "direction": "rtl"});
        for entry in legacy["recent"].as_array_mut().unwrap() { entry.as_object_mut().unwrap().remove("title"); }
        fs::write(&store.path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let migrated = Store::load(store.path.clone(), None).unwrap();
        let public = migrated.public_library().unwrap();
        assert_eq!(public.preferences, ReaderPreferences { theme: "night".into(), mode: "spread".into(), direction: "rtl".into() });
        assert_eq!(public.recent[1].state, json!({"page": 7, "bookmarks": [2, 7]}));
        assert_eq!(public.recent[0].title, public.recent[0].name);
        let persisted: Value = serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        assert_eq!(persisted["preferences"]["theme"], "night");
        assert!(persisted["recent"][1]["state"].get("theme").is_none());
        let preferences = migrated.save_preferences(&json!({"theme": "paper"})).unwrap();
        assert_eq!(preferences.mode, "spread");
        migrated.save_state(&second_document.id, &json!({"page": 5, "bookmarks": [5], "theme": "white", "mode": "single"})).unwrap();
        migrated.open_authorized_path(&second).unwrap();
        let reloaded = Store::load(store.path, None).unwrap().public_library().unwrap();
        assert_eq!(reloaded.preferences.theme, "paper");
        assert_eq!(reloaded.preferences.mode, "spread");
        assert_eq!(reloaded.recent[0].state, json!({"page": 5, "bookmarks": [5]}));
        legacy["lastOpenedId"] = Value::Null;
        let fallback_path = root.join("fallback.json");
        fs::write(&fallback_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        assert_eq!(Store::load(fallback_path, None).unwrap().public_library().unwrap().preferences, ReaderPreferences { theme: "white".into(), mode: "single".into(), direction: "ltr".into() });
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn global_preferences_are_validated_and_saved_without_an_open_book() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-empty-preferences-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let store = Store::load(root.join("library.json"), None).unwrap();
        let preferences = store.save_preferences(&json!({"theme": "night", "ignored": "value"})).unwrap();
        for invalid in [json!({"theme": "unknown"}), json!({"mode": 2}), json!({"direction": "vertical"}), json!([])] { assert!(store.save_preferences(&invalid).is_err()); }
        let saved = Store::load(store.path.clone(), None).unwrap().public_library().unwrap();
        assert_eq!(saved.preferences, preferences);
        assert!(saved.recent.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stored_titles_do_not_rename_files_and_require_an_authorized_book() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("rust-titles-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let file = root.join("opaque-123.pdf"); fs::write(&file, b"%PDF-1.7\nbook").unwrap();
        let store = Store::load(root.join("library.json"), None).unwrap();
        let document = store.open_authorized_path(&file).unwrap();
        assert_eq!(document.title, "opaque-123.pdf");
        assert!(store.set_book_title("not-authorized", "Book title").is_err());
        assert!(store.set_book_title(&document.id, "  \n ").is_err());
        assert!(store.set_book_title(&document.id, &"あ".repeat(501)).is_err());
        store.set_book_title(&document.id, "  読書と技術\n  のノート\0  ").unwrap();
        let reopened = store.open_authorized_path(&file).unwrap();
        assert_eq!(reopened.title, "読書と技術 のノート");
        assert_eq!(reopened.name, "opaque-123.pdf");
        let saved = Store::load(store.path.clone(), None).unwrap().public_library().unwrap();
        assert_eq!(saved.recent[0].title, reopened.title);
        assert!(file.exists());
        assert!(store.read_pdf(&document.id).unwrap().starts_with(b"%PDF"));
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
        assert!(current.exists());
        assert!(Store::load(current, None).unwrap().startup_warning.lock().unwrap().is_none());
        assert!(fs::read_dir(&root).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with("library.corrupt-")));
        let legacy = root.join("legacy.json");
        fs::write(&legacy, "{broken legacy").unwrap();
        let recovered = Store::load(root.join("imported.json"), Some(&legacy)).unwrap();
        assert!(recovered.public_library().unwrap().recent.is_empty());
        assert!(recovered.startup_warning.lock().unwrap().is_some());
        assert_eq!(fs::read_to_string(&legacy).unwrap(), "{broken legacy");
        fs::remove_dir_all(root).unwrap();
    }
}
