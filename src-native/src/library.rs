use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicU64, Ordering}, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_LIBRARY_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 50_000;
const MAX_SCAN_DEPTH: usize = 32;
const MAX_IMPORT_ERRORS: usize = 100;
static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize)]
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub document: Document,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    let mut ids = std::collections::HashSet::new();
    let mut paths = std::collections::HashSet::new();
    library.recent.retain(|entry| ids.insert(entry.document.id.clone()) && paths.insert(entry.path.clone()));
    if !library.recent.iter().any(|entry| Some(&entry.document.id) == library.last_opened_id.as_ref()) { library.last_opened_id = None; }
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

#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub added: Vec<Entry>,
    pub duplicates: usize,
    pub skipped: usize,
    pub errors: Vec<ImportError>,
    pub limited: bool,
}

#[derive(Clone, Debug)]
pub struct ImportError { pub path: PathBuf, pub message: String }

impl ImportReport {
    fn error(&mut self, path: &Path, message: impl Into<String>) {
        if self.errors.len() < MAX_IMPORT_ERRORS {
            self.errors.push(ImportError { path: path.into(), message: message.into() });
        } else { self.limited = true; }
    }
}

impl Store {
    pub fn load_default() -> Result<Self, String> {
        let home = std::env::var_os("HOME").ok_or("ユーザーフォルダが見つかりません。")?;
        let support = PathBuf::from(home).join("Library/Application Support");
        Self::load(support.join("com.kairi7011.sumi/library.json"), Some(&support.join("Sumi/library.json")))
    }

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
        let store = Self { library: Mutex::new(library), path, startup_warning: Mutex::new(warning) };
        if persist_on_load {
            let library = store.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
            store.persist(&library)?;
        }
        Ok(store)
    }

    fn persist(&self, library: &Library) -> Result<(), String> {
        let parent = self.path.parent().ok_or("保存先が見つかりません。")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec(library).map_err(|e| e.to_string())?;
        if data.len() as u64 > MAX_LIBRARY_BYTES { return Err("本棚の記録が保存できる大きさを超えています。".into()); }
        let sequence = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self.path.with_extension(format!("json.{}.{}.tmp", std::process::id(), sequence));
        let result = (|| {
            let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temporary).map_err(|e| e.to_string())?;
            file.write_all(&data).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            fs::rename(&temporary, &self.path).map_err(|e| e.to_string())
        })();
        if result.is_err() { let _ = fs::remove_file(&temporary); }
        result
    }

    fn update<T>(&self, change: impl FnOnce(&mut Library) -> Result<T, String>) -> Result<T, String> {
        let mut current = self.library.lock().map_err(|_| "本棚を読み込めませんでした。")?;
        let mut next = current.clone();
        let result = change(&mut next)?;
        self.persist(&next)?;
        *current = next;
        Ok(result)
    }

    pub fn snapshot(&self) -> Result<Library, String> {
        self.library.lock().map(|library| library.clone()).map_err(|_| "本棚を読み込めませんでした。".into())
    }

    pub fn public_library(&self) -> Result<PublicLibrary, String> {
        let library = self.snapshot()?;
        Ok(PublicLibrary { recent: library.recent.iter().map(|entry| entry.document.clone()).collect(), last_opened_id: library.last_opened_id.clone(), preferences: preferences_of(&library) })
    }

    pub fn preferences(&self) -> Result<ReaderPreferences, String> { Ok(preferences_of(&self.snapshot()?)) }

    pub fn entry(&self, id: &str) -> Result<Entry, String> {
        self.snapshot()?.recent.into_iter().find(|entry| entry.document.id == id).ok_or_else(|| "本棚にあるPDFを選んでください。".into())
    }

    pub fn authorized_path(&self, id: &str) -> Result<PathBuf, String> {
        let entry = self.entry(id)?;
        let canonical = fs::canonicalize(&entry.path).map_err(|_| "ファイルが見つかりません。移動先のPDFを選び直してください。")?;
        if canonical != entry.path { return Err("ファイルの場所が変わりました。移動先のPDFを選び直してください。".into()); }
        Ok(canonical)
    }

    /// Imports references only; opening and the last-read timestamp are separate operations.
    pub fn import_paths(&self, paths: &[PathBuf]) -> Result<ImportReport, String> {
        let mut report = ImportReport::default();
        let mut found = Vec::new();
        let mut visited = 0;
        let mut directories = std::collections::HashSet::new();
        for path in paths {
            collect_pdfs(path, 0, &mut visited, &mut directories, &mut found, &mut report);
            if visited >= MAX_SCAN_ENTRIES { report.limited = true; break; }
        }
        let mut candidates = Vec::new();
        for path in found {
            match describe_pdf(&path) {
                Ok(entry) => candidates.push(entry),
                Err(error) => report.error(&path, error),
            }
        }
        if candidates.is_empty() { return Ok(report); }
        self.update(|library| {
            let mut existing: std::collections::HashSet<PathBuf> = library.recent.iter().map(|entry| entry.path.clone()).collect();
            let mut ids: std::collections::HashSet<String> = library.recent.iter().map(|entry| entry.document.id.clone()).collect();
            for mut entry in candidates {
                if !existing.insert(entry.path.clone()) { report.duplicates += 1; continue; }
                // A relocated book keeps its original ID; a new PDF can later reuse that old path.
                let mut suffix = 0u64;
                while !ids.insert(entry.document.id.clone()) {
                    suffix += 1;
                    let identity = format!("{}#{}", entry.path.to_string_lossy(), suffix);
                    entry.document.id = Sha256::digest(identity.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
                }
                report.added.push(entry.clone());
                library.recent.push(entry);
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Called only after the UI chooses a book to read, including an imported book.
    pub fn open(&self, id: &str) -> Result<Entry, String> {
        let path = self.authorized_path(id)?;
        let metadata = describe_pdf(&path)?;
        self.update(|library| {
            let index = library.recent.iter().position(|entry| entry.document.id == id).ok_or("本棚にあるPDFを選んでください。")?;
            let mut entry = library.recent.remove(index);
            entry.document.size = metadata.document.size;
            entry.document.modified_at = metadata.document.modified_at;
            entry.document.opened_at = milliseconds(SystemTime::now());
            library.last_opened_id = Some(id.into());
            library.recent.insert(0, entry.clone());
            Ok(entry)
        })
    }

    pub fn open_authorized_path(&self, path: &Path) -> Result<Entry, String> {
        let canonical = fs::canonicalize(path).map_err(|_| "ファイルが見つかりません。")?;
        self.import_paths(&[canonical.clone()])?;
        let id = self.snapshot()?.recent.into_iter().find(|entry| entry.path == canonical).map(|entry| entry.document.id).ok_or("PDFを本棚に追加できませんでした。")?;
        self.open(&id)
    }

    /// Relinking preserves the book ID, title, reading position and bookmarks.
    pub fn relink(&self, id: &str, path: &Path) -> Result<Entry, String> {
        let replacement = describe_pdf(path)?;
        self.update(|library| {
            if library.recent.iter().any(|entry| entry.document.id != id && entry.path == replacement.path) { return Err("このPDFはすでに本棚にあります。".into()); }
            let entry = library.recent.iter_mut().find(|entry| entry.document.id == id).ok_or("本棚にあるPDFを選んでください。")?;
            if entry.document.title == entry.document.name { entry.document.title = replacement.document.name.clone(); }
            entry.path = replacement.path.clone();
            entry.document.name = replacement.document.name.clone();
            entry.document.size = replacement.document.size;
            entry.document.modified_at = replacement.document.modified_at;
            Ok(entry.clone())
        })
    }

    /// Removes metadata only. The original PDF is never moved or deleted.
    pub fn remove(&self, id: &str) -> Result<bool, String> {
        self.update(|library| {
            let before = library.recent.len();
            library.recent.retain(|entry| entry.document.id != id);
            if library.last_opened_id.as_deref() == Some(id) { library.last_opened_id = None; }
            Ok(before != library.recent.len())
        })
    }

    pub fn save_state(&self, id: &str, state: &Value) -> Result<(), String> {
        let state = sanitize_state(state)?;
        self.update(|library| {
            let entry = library.recent.iter_mut().find(|entry| entry.document.id == id).ok_or("本棚にあるPDFを選んでください。")?;
            entry.document.state = state;
            Ok(())
        })
    }

    pub fn save_preferences(&self, update: &Value) -> Result<ReaderPreferences, String> {
        self.update(|library| {
            let preferences = sanitize_preferences(update, &preferences_of(library))?;
            library.preferences = Some(serde_json::to_value(&preferences).map_err(|e| e.to_string())?);
            Ok(preferences)
        })
    }

    pub fn set_book_title(&self, id: &str, title: &str) -> Result<(), String> {
        let title = sanitize_title(title)?;
        self.update(|library| {
            let entry = library.recent.iter_mut().find(|entry| entry.document.id == id).ok_or("本棚にあるPDFを選んでください。")?;
            entry.document.title = title;
            Ok(())
        })
    }
}

fn preferences_of(library: &Library) -> ReaderPreferences {
    library.preferences.as_ref().and_then(|value| sanitize_preferences(value, &ReaderPreferences::default()).ok()).unwrap_or_default()
}

fn describe_pdf(path: &Path) -> Result<Entry, String> {
    if !path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) { return Err("PDFファイルを選んでください。".into()); }
    let canonical = fs::canonicalize(path).map_err(|_| "ファイルが見つかりません。移動先のPDFを選び直してください。")?;
    if !fs::metadata(&canonical).map_err(|e| e.to_string())?.is_file() { return Err("PDFファイルを選んでください。".into()); }
    let mut file = File::open(&canonical).map_err(|_| "PDFを開けませんでした。アクセス権を確認してください。")?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() { return Err("PDFファイルを選んでください。".into()); }
    let mut header = [0u8; 1024];
    let length = file.read(&mut header).map_err(|e| e.to_string())?;
    if !header[..length].windows(5).any(|part| part == b"%PDF-") { return Err("PDF形式として読み込めませんでした。".into()); }
    let canonical_text = canonical.to_str().ok_or("ファイル名を読み込めませんでした。")?;
    let id = Sha256::digest(canonical_text.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
    let name: String = canonical.file_name().and_then(|name| name.to_str()).ok_or("ファイル名を読み込めませんでした。")?.into();
    Ok(Entry { document: Document { id, title: name.clone(), name, size: metadata.len(), modified_at: milliseconds(metadata.modified().unwrap_or(UNIX_EPOCH)), opened_at: 0.0, state: empty_state() }, path: canonical })
}

fn collect_pdfs(path: &Path, depth: usize, visited: &mut usize, directories: &mut std::collections::HashSet<PathBuf>, found: &mut Vec<PathBuf>, report: &mut ImportReport) {
    if *visited >= MAX_SCAN_ENTRIES { report.limited = true; return; }
    *visited += 1;
    let metadata = match fs::symlink_metadata(path) { Ok(metadata) => metadata, Err(error) => { report.error(path, error.to_string()); return; } };
    if metadata.file_type().is_symlink() && path.is_dir() { report.skipped += 1; return; }
    if metadata.is_dir() {
        if depth >= MAX_SCAN_DEPTH { report.limited = true; report.error(path, "フォルダの階層が深いため、ここから先は追加していません。"); return; }
        let canonical = match fs::canonicalize(path) { Ok(path) => path, Err(error) => { report.error(path, error.to_string()); return; } };
        if !directories.insert(canonical) { return; }
        let entries = match fs::read_dir(path) { Ok(entries) => entries, Err(error) => { report.error(path, error.to_string()); return; } };
        for entry in entries {
            if *visited >= MAX_SCAN_ENTRIES { report.limited = true; break; }
            match entry {
                Ok(entry) => collect_pdfs(&entry.path(), depth + 1, visited, directories, found, report),
                Err(error) => { *visited += 1; report.error(path, error.to_string()); }
            }
        }
    } else if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("pdf")) {
        found.push(path.into());
    } else { report.skipped += 1; }
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

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let id = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../test-artifacts").join(format!("native-library-{}-{id}", std::process::id()));
            fs::create_dir_all(&root).unwrap(); Self(root)
        }
        fn pdf(&self, name: &str) -> PathBuf { let path = self.0.join(name); fs::create_dir_all(path.parent().unwrap()).unwrap(); fs::write(&path, b"%PDF-1.7\nfixture").unwrap(); path }
        fn store(&self) -> Store { Store::load(self.0.join("state/library.json"), None).unwrap() }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

    #[test]
    fn recursive_import_keeps_more_than_forty_books_without_changing_last_read() {
        let fixture = Fixture::new(); let store = fixture.store();
        let first = fixture.pdf("reading.pdf");
        let reading = store.open_authorized_path(&first).unwrap();
        store.save_state(&reading.document.id, &json!({"page": 9, "bookmarks": [3, 9]})).unwrap();
        for i in 0..65 { fixture.pdf(&format!("books/nested/book-{i:02}.PDF")); }
        let report = store.import_paths(&[fixture.0.join("books")]).unwrap();
        assert_eq!(report.added.len(), 65);
        assert!(report.added.iter().all(|entry| entry.document.opened_at == 0.0));
        let library = store.snapshot().unwrap();
        assert_eq!(library.recent.len(), 66);
        assert_eq!(library.last_opened_id.as_deref(), Some(reading.document.id.as_str()));
        let reload = fixture.store(); assert_eq!(reload.snapshot().unwrap().recent.len(), 66);
        assert_eq!(reload.entry(&reading.document.id).unwrap().document.state["page"], 9);
    }

    #[test]
    fn import_deduplicates_paths_and_symlinks_and_continues_after_invalid_files() {
        let fixture = Fixture::new(); let store = fixture.store();
        let pdf = fixture.pdf("books/good.pdf");
        fs::write(fixture.0.join("books/broken.pdf"), b"not a pdf").unwrap();
        fs::write(fixture.0.join("books/readme.txt"), b"text").unwrap();
        std::os::unix::fs::symlink(&pdf, fixture.0.join("alias.pdf")).unwrap();
        std::os::unix::fs::symlink(fixture.0.join("books"), fixture.0.join("books/loop")).unwrap();
        let report = store.import_paths(&[fixture.0.join("books"), pdf, fixture.0.join("alias.pdf"), fixture.0.join("missing.pdf")]).unwrap();
        assert_eq!(report.added.len(), 1); assert_eq!(report.duplicates, 2);
        assert_eq!(report.errors.len(), 2); assert_eq!(report.skipped, 2);
        assert!(store.snapshot().unwrap().last_opened_id.is_none());
        assert_eq!(store.import_paths(&[fixture.0.join("books/good.pdf")]).unwrap().duplicates, 1);
    }

    #[test]
    fn relink_keeps_identity_and_notes_and_remove_keeps_original_file() {
        let fixture = Fixture::new(); let store = fixture.store();
        let first = fixture.pdf("original.pdf"); let entry = store.open_authorized_path(&first).unwrap(); let id = &entry.document.id;
        store.set_book_title(id, "A useful book").unwrap();
        store.save_state(id, &json!({"page": 18, "bookmarks": [2,18]})).unwrap();
        let moved = fixture.0.join("moved.pdf"); fs::rename(first, &moved).unwrap();
        assert!(store.authorized_path(id).is_err());
        let entry = store.relink(id, &moved).unwrap();
        assert_eq!(entry.document.id, *id); assert_eq!(entry.document.title, "A useful book");
        assert_eq!(entry.document.state, json!({"page":18,"bookmarks":[2,18]}));
        assert_eq!(store.import_paths(&[moved.clone()]).unwrap().duplicates, 1);
        assert_eq!(store.open(id).unwrap().path, fs::canonicalize(&moved).unwrap());
        assert!(store.remove(id).unwrap()); assert!(moved.exists());
        assert!(store.snapshot().unwrap().last_opened_id.is_none());
        assert!(!store.remove(id).unwrap());
    }

    #[test]
    fn importing_at_a_relinked_books_old_path_gets_a_distinct_stable_id() {
        let fixture = Fixture::new(); let store = fixture.store();
        let original = fixture.pdf("original.pdf"); let entry = store.open_authorized_path(&original).unwrap();
        let moved = fixture.0.join("moved.pdf"); fs::rename(&original, &moved).unwrap();
        store.relink(&entry.document.id, &moved).unwrap();
        let new_pdf = fixture.pdf("original.pdf");
        let imported = store.import_paths(&[new_pdf]).unwrap();
        assert_eq!(imported.added.len(), 1);
        assert_ne!(entry.document.id, imported.added[0].document.id);
        let reloaded = fixture.store(); assert_eq!(reloaded.snapshot().unwrap().recent.len(), 2);
        assert_eq!(reloaded.entry(&entry.document.id).unwrap().path, fs::canonicalize(moved).unwrap());
    }

    #[test]
    fn relink_rejects_a_file_already_assigned_to_another_book() {
        let fixture = Fixture::new(); let store = fixture.store();
        let first = store.open_authorized_path(&fixture.pdf("a.pdf")).unwrap();
        let second = fixture.pdf("b.pdf"); store.import_paths(&[second.clone()]).unwrap();
        assert!(store.relink(&first.document.id, &second).is_err());
        assert_eq!(store.snapshot().unwrap().recent.len(), 2);
    }

    #[test]
    fn migrates_legacy_appearance_once_and_preserves_book_positions() {
        let fixture = Fixture::new(); let store = fixture.store();
        let entry = store.open_authorized_path(&fixture.pdf("book.pdf")).unwrap();
        let mut json: Value = serde_json::from_slice(&fs::read(&store.path).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("preferences");
        json["recent"][0]["state"] = json!({"page":7,"bookmarks":[4],"theme":"night","mode":"spread","direction":"rtl"});
        fs::write(&store.path, serde_json::to_vec(&json).unwrap()).unwrap();
        let migrated = Store::load(fixture.0.join("native/library.json"), Some(&store.path)).unwrap();
        assert_eq!(migrated.preferences().unwrap(), ReaderPreferences { theme:"night".into(), mode:"spread".into(), direction:"rtl".into() });
        assert_eq!(migrated.entry(&entry.document.id).unwrap().document.state, json!({"page":7,"bookmarks":[4]}));
        migrated.save_preferences(&json!({"theme":"white"})).unwrap();
        let second = migrated.open_authorized_path(&fixture.pdf("second.pdf")).unwrap();
        migrated.save_state(&second.document.id, &json!({"page":2,"theme":"paper"})).unwrap();
        assert_eq!(migrated.preferences().unwrap().theme, "white");
    }

    #[test]
    fn failed_save_does_not_change_memory_or_overwrite_existing_state() {
        let fixture = Fixture::new(); let mut store = fixture.store();
        let entry = store.open_authorized_path(&fixture.pdf("book.pdf")).unwrap();
        let original = fs::read(&store.path).unwrap();
        let blocker = fixture.0.join("not-a-directory"); fs::write(&blocker,b"keep").unwrap();
        store.path = blocker.join("library.json");
        assert!(store.save_state(&entry.document.id, &json!({"page":99})).is_err());
        assert_eq!(store.entry(&entry.document.id).unwrap().document.state, json!({}));
        assert_eq!(fs::read(fixture.0.join("state/library.json")).unwrap(), original);
    }

    #[test]
    fn malformed_history_is_preserved_and_recovery_remains_launchable() {
        let fixture = Fixture::new(); let path = fixture.0.join("library.json"); fs::write(&path,b"broken").unwrap();
        let recovered = Store::load(path.clone(),None).unwrap();
        assert!(recovered.snapshot().unwrap().recent.is_empty());
        assert!(recovered.startup_warning.lock().unwrap().is_some());
        assert_eq!(fs::read_dir(&fixture.0).unwrap().filter_map(Result::ok).filter(|entry| entry.file_name().to_string_lossy().starts_with("library.corrupt-")).count(),1);
        assert!(Store::load(path,None).is_ok());
    }

    #[test]
    fn traversal_reports_depth_limits_and_still_imports_accessible_siblings() {
        let fixture = Fixture::new(); let store = fixture.store();
        fixture.pdf("folder/root.pdf");
        let name = format!("folder/{}deep.pdf", "nested/".repeat(35)); fixture.pdf(&name);
        let report = store.import_paths(&[fixture.0.join("folder")]).unwrap();
        assert!(report.limited); assert_eq!(report.added.len(),1); assert_eq!(report.errors.len(),1);
    }

    #[test]
    fn state_validation_keeps_page_and_bookmarks_separate_from_global_preferences() {
        assert_eq!(sanitize_state(&json!({"page":4,"bookmarks":[2],"theme":"night","zoom":3})).unwrap(),json!({"page":4,"bookmarks":[2]}));
        for invalid in [json!({"page":0}),json!({"page":"4"}),json!({"bookmarks":[0]})] { assert!(sanitize_state(&invalid).is_err()); }
        assert!(sanitize_preferences(&json!({"mode":"unknown"}), &ReaderPreferences::default()).is_err());
    }
}
