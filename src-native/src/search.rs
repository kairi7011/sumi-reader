use crate::pdf::PDFDocument;
use objc2::rc::autoreleasepool;
use objc2_foundation::NSString;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime};

const MAX_RESULTS: usize = 80;
const MAX_BOOKS: usize = 128;
const MAX_PAGES: usize = 20_000;
const MAX_QUERY_CHARS: usize = 200;
const MAX_PAGE_CHARS: usize = 300_000;
const SNIPPET_CHARS: usize = 260;

#[derive(Clone)]
pub struct SearchBook {
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub password: Option<String>,
}

impl std::fmt::Debug for SearchBook {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("SearchBook").field("id", &self.id).field("path", &self.path)
            .field("title", &self.title).field("has_password", &self.password.is_some()).finish()
    }
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub query: String,
    /// Put the currently open book first to receive its matches first.
    pub books: Vec<SearchBook>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub book_id: String,
    pub title: String,
    /// PDF page numbers are one-based.
    pub page: usize,
    pub snippet: String,
    /// Unicode character offsets within snippet, not UTF-8 bytes or UTF-16 units.
    pub match_start: usize,
    pub match_len: usize,
}

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub request_id: u64,
    /// Each message replaces the previous result list for this request.
    pub hits: Vec<SearchHit>,
    pub complete: bool,
    pub scanned_pages: usize,
    pub warnings: Vec<String>,
}

struct Job {
    id: u64,
    request: SearchRequest,
}

struct Shared {
    pending: Mutex<Option<Job>>,
    wake: Condvar,
    generation: AtomicU64,
    stopped: AtomicBool,
}

pub struct SearchEngine {
    shared: Arc<Shared>,
    results: Receiver<SearchResult>,
    worker: Option<JoinHandle<()>>,
}

impl Default for SearchEngine {
    fn default() -> Self { Self::new() }
}

impl SearchEngine {
    pub fn new() -> Self {
        let shared = Arc::new(Shared {
            pending: Mutex::new(None), wake: Condvar::new(),
            generation: AtomicU64::new(0), stopped: AtomicBool::new(false),
        });
        let (sender, results) = mpsc::sync_channel(4);
        let state = Arc::clone(&shared);
        let worker = thread::Builder::new().name("sumi-search".into()).spawn(move || {
            let mut fingerprints = HashMap::new();
            loop {
                let job = {
                    let mut pending = state.pending.lock().unwrap_or_else(|error| error.into_inner());
                    while pending.is_none() && !state.stopped.load(Ordering::Acquire) {
                        pending = state.wake.wait(pending).unwrap_or_else(|error| error.into_inner());
                    }
                    if state.stopped.load(Ordering::Acquire) { break; }
                    pending.take().unwrap()
                };
                run_search(job, &state, &sender, &mut fingerprints);
            }
        }).expect("Could not start the local PDF search worker");
        Self { shared, results, worker: Some(worker) }
    }

    pub fn submit(&self, request: SearchRequest) -> u64 {
        let id = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.shared.pending.lock().unwrap_or_else(|error| error.into_inner()) = Some(Job { id, request });
        self.shared.wake.notify_one();
        id
    }

    pub fn cancel(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.shared.pending.lock().unwrap_or_else(|error| error.into_inner()).take();
    }

    pub fn try_recv(&self) -> Option<SearchResult> {
        while let Ok(result) = self.results.try_recv() {
            if result.request_id == self.shared.generation.load(Ordering::Acquire) { return Some(result); }
        }
        None
    }
}

impl Drop for SearchEngine {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Release);
        self.cancel();
        self.shared.wake.notify_one();
        // PDFKit can be inside a page decode; closing the UI must never wait for it.
        self.worker.take();
    }
}

fn cancelled(shared: &Shared, id: u64) -> bool {
    shared.stopped.load(Ordering::Acquire) || shared.generation.load(Ordering::Acquire) != id
}

fn publish(sender: &SyncSender<SearchResult>, shared: &Shared, result: SearchResult) {
    let mut value = result;
    loop {
        if cancelled(shared, value.request_id) { return; }
        match sender.try_send(value) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => return,
            Err(TrySendError::Full(result)) => value = result,
        }
        thread::sleep(Duration::from_millis(8));
    }
}

fn warning(result: &mut SearchResult, text: String) {
    if result.warnings.len() < 8 { result.warnings.push(text); }
}

fn run_search(job: Job, shared: &Shared, sender: &SyncSender<SearchResult>, fingerprints: &mut HashMap<PathBuf, Fingerprint>) {
    let mut result = SearchResult { request_id: job.id, hits: Vec::new(), complete: false, scanned_pages: 0, warnings: Vec::new() };
    let query = fold(&display_text(&job.request.query).chars().take(MAX_QUERY_CHARS).collect::<String>());
    if query.is_empty() {
        result.complete = true;
        publish(sender, shared, result);
        return;
    }
    if job.request.query.chars().count() > MAX_QUERY_CHARS { warning(&mut result, "検索語は先頭の200文字を使っています。".into()); }
    if job.request.books.len() > MAX_BOOKS { warning(&mut result, "今回の検索は先頭の128冊までです。".into()); }
    let limit = job.request.limit.clamp(1, MAX_RESULTS);
    let mut last_update = Instant::now();
    let index = NativeIndex::open_default();
    for book in job.request.books.iter().take(MAX_BOOKS) {
        if cancelled(shared, job.id) || result.hits.len() >= limit { break; }
        if let Some(indexed) = index.as_ref().and_then(|index| index.search(book, &query, limit - result.hits.len(), shared, job.id, fingerprints)) {
            result.scanned_pages += indexed.page_count;
            result.hits.extend(indexed.hits);
            publish(sender, shared, result.clone());
            last_update = Instant::now();
            continue;
        }
        if cancelled(shared, job.id) { break; }
        autoreleasepool(|_| {
            let Some(document) = PDFDocument::from_path(&book.path) else {
                warning(&mut result, format!("「{}」を開けませんでした。", book.title));
                return;
            };
            if document.is_locked() && !book.password.as_deref().is_some_and(|password| document.unlock(password)) {
                warning(&mut result, format!("「{}」の検索にはパスワードが必要です。", book.title));
                return;
            }
            let count = document.page_count();
            if count > MAX_PAGES { warning(&mut result, format!("「{}」は先頭の20,000ページを検索します。", book.title)); }
            for index in 0..count.min(MAX_PAGES) {
                if cancelled(shared, job.id) || result.hits.len() >= limit { break; }
                let text = autoreleasepool(|_| document.page(index).map(|page| page.text()).unwrap_or_default());
                if cancelled(shared, job.id) { break; }
                result.scanned_pages += 1;
                let added = if let Some((snippet, match_start, match_len)) = matching_snippet(&text, &query) {
                    result.hits.push(SearchHit { book_id: book.id.clone(), title: book.title.clone(), page: index + 1, snippet, match_start, match_len });
                    true
                } else { false };
                if (added && result.hits.len() == 1) || last_update.elapsed() >= Duration::from_millis(120) {
                    publish(sender, shared, result.clone());
                    last_update = Instant::now();
                }
            }
        });
    }
    if cancelled(shared, job.id) { return; }
    if result.hits.len() >= limit { warning(&mut result, format!("検索結果は先頭の{}件を表示しています。", limit)); }
    result.complete = true;
    publish(sender, shared, result);
}

struct Fingerprint {
    modified: SystemTime,
    length: u64,
    sha256: String,
}

fn fingerprint(path: &Path, shared: &Shared, id: u64, cache: &mut HashMap<PathBuf, Fingerprint>) -> Option<String> {
    let metadata = path.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    if let Some(known) = cache.get(path) {
        if known.modified == modified && known.length == metadata.len() { return Some(known.sha256.clone()); }
    }
    let mut file = File::open(path).ok()?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65_536];
    loop {
        if cancelled(shared, id) { return None; }
        let length = file.read(&mut buffer).ok()?;
        if length == 0 { break; }
        digest.update(&buffer[..length]);
    }
    let after = file.metadata().ok()?;
    if after.modified().ok()? != modified || after.len() != metadata.len() { return None; }
    let sha256: String = digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect();
    if cache.len() >= MAX_BOOKS { cache.clear(); }
    cache.insert(path.to_path_buf(), Fingerprint { modified, length: metadata.len(), sha256: sha256.clone() });
    Some(sha256)
}

#[repr(C)]
struct SqliteConnection { _opaque: [u8; 0] }
#[repr(C)]
struct SqliteStatement { _opaque: [u8; 0] }

#[link(name = "sqlite3")]
unsafe extern "C" {
    fn sqlite3_open_v2(path: *const c_char, database: *mut *mut SqliteConnection, flags: c_int, vfs: *const c_char) -> c_int;
    fn sqlite3_close(database: *mut SqliteConnection) -> c_int;
    fn sqlite3_busy_timeout(database: *mut SqliteConnection, milliseconds: c_int) -> c_int;
    fn sqlite3_prepare_v2(database: *mut SqliteConnection, sql: *const c_char, bytes: c_int, statement: *mut *mut SqliteStatement, tail: *mut *const c_char) -> c_int;
    fn sqlite3_bind_text(statement: *mut SqliteStatement, parameter: c_int, text: *const c_char, bytes: c_int, destructor: Option<unsafe extern "C" fn(*mut c_void)>) -> c_int;
    fn sqlite3_bind_int64(statement: *mut SqliteStatement, parameter: c_int, value: i64) -> c_int;
    fn sqlite3_step(statement: *mut SqliteStatement) -> c_int;
    fn sqlite3_finalize(statement: *mut SqliteStatement) -> c_int;
    fn sqlite3_column_text(statement: *mut SqliteStatement, column: c_int) -> *const u8;
    fn sqlite3_column_bytes(statement: *mut SqliteStatement, column: c_int) -> c_int;
    fn sqlite3_column_int64(statement: *mut SqliteStatement, column: c_int) -> i64;
    fn sqlite3_progress_handler(database: *mut SqliteConnection, operations: c_int, callback: Option<unsafe extern "C" fn(*mut c_void) -> c_int>, context: *mut c_void);
}

struct NativeIndex { database: *mut SqliteConnection }
struct Statement<'a> { pointer: *mut SqliteStatement, _owner: std::marker::PhantomData<&'a NativeIndex> }
struct IndexedMatches { page_count: usize, hits: Vec<SearchHit> }
struct QueryCancellation<'a> { shared: &'a Shared, id: u64 }
struct ProgressGuard(*mut SqliteConnection);

unsafe extern "C" fn stop_sqlite(context: *mut c_void) -> c_int {
    let token = unsafe { &*(context as *const QueryCancellation<'_>) };
    c_int::from(cancelled(token.shared, token.id))
}

impl Drop for ProgressGuard {
    fn drop(&mut self) { unsafe { sqlite3_progress_handler(self.0, 0, None, std::ptr::null_mut()); } }
}

impl Drop for NativeIndex {
    fn drop(&mut self) { unsafe { sqlite3_close(self.database); } }
}

impl Drop for Statement<'_> {
    fn drop(&mut self) { unsafe { sqlite3_finalize(self.pointer); } }
}

impl Statement<'_> {
    fn bind(&self, parameter: c_int, text: &CString) -> bool {
        unsafe { sqlite3_bind_text(self.pointer, parameter, text.as_ptr(), -1, None) == 0 }
    }
    fn text(&self, column: c_int) -> String {
        unsafe {
            let pointer = sqlite3_column_text(self.pointer, column);
            let length = sqlite3_column_bytes(self.pointer, column).max(0) as usize;
            if pointer.is_null() || length == 0 { return String::new(); }
            String::from_utf8_lossy(std::slice::from_raw_parts(pointer, length.min(MAX_PAGE_CHARS * 4))).into_owned()
        }
    }
    fn integer(&self, column: c_int) -> i64 { unsafe { sqlite3_column_int64(self.pointer, column) } }
    fn step(&self) -> c_int { unsafe { sqlite3_step(self.pointer) } }
}

impl NativeIndex {
    fn open_default() -> Option<Self> {
        let directory = std::env::var_os("HOME")?;
        Self::open(&PathBuf::from(directory).join("Library/Application Support/Sumi/library-index/books.sqlite3"))
    }

    fn open(path: &Path) -> Option<Self> {
        let path = CString::new(path.to_str()?).ok()?;
        let mut database = std::ptr::null_mut();
        // SQLITE_OPEN_READONLY never creates or changes the user's separate AI index.
        if unsafe { sqlite3_open_v2(path.as_ptr(), &mut database, 1, std::ptr::null()) } != 0 {
            if !database.is_null() { unsafe { sqlite3_close(database); } }
            return None;
        }
        unsafe { sqlite3_busy_timeout(database, 40); }
        Some(Self { database })
    }

    fn prepare(&self, sql: &CStr) -> Option<Statement<'_>> {
        let mut pointer = std::ptr::null_mut();
        if unsafe { sqlite3_prepare_v2(self.database, sql.as_ptr(), -1, &mut pointer, std::ptr::null_mut()) } != 0 { return None; }
        Some(Statement { pointer, _owner: std::marker::PhantomData })
    }

    fn search(&self, book: &SearchBook, query: &str, limit: usize, shared: &Shared, id: u64, fingerprints: &mut HashMap<PathBuf, Fingerprint>) -> Option<IndexedMatches> {
        let canonical = book.path.canonicalize().ok()?;
        let path = CString::new(canonical.to_str()?).ok()?;
        let query_string = CString::new(query).ok()?;
        let mut token = QueryCancellation { shared, id };
        unsafe { sqlite3_progress_handler(self.database, 2000, Some(stop_sqlite), (&mut token as *mut QueryCancellation<'_>).cast()); }
        let _progress = ProgressGuard(self.database);
        let metadata = self.prepare(c"SELECT sha256, page_count FROM books WHERE path = ?1")?;
        if !metadata.bind(1, &path) || metadata.step() != 100 { return None; }
        let expected_hash = metadata.text(0);
        let page_count = usize::try_from(metadata.integer(1)).ok()?;
        drop(metadata);
        if fingerprint(&canonical, shared, id, fingerprints)? != expected_hash { return None; }
        if cancelled(shared, id) { return None; }
        let statement = self.prepare(c"SELECT p.page, p.text FROM pages p JOIN books b ON b.id = p.book_id WHERE b.path = ?1 AND instr(p.search_text, ?2) > 0 ORDER BY p.page LIMIT ?3")?;
        if !statement.bind(1, &path) || !statement.bind(2, &query_string) { return None; }
        if unsafe { sqlite3_bind_int64(statement.pointer, 3, limit as i64) } != 0 { return None; }
        let mut hits = Vec::new();
        loop {
            match statement.step() {
                100 => {
                    if cancelled(shared, id) { return None; }
                    let page = usize::try_from(statement.integer(0)).ok()?;
                    if let Some((snippet, match_start, match_len)) = matching_snippet(&statement.text(1), query) {
                        hits.push(SearchHit { book_id: book.id.clone(), title: book.title.clone(), page, snippet, match_start, match_len });
                    }
                }
                101 => break,
                _ => return None,
            }
        }
        Some(IndexedMatches { page_count, hits })
    }
}

fn cjk(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
}

fn compatibility(text: &str) -> String {
    autoreleasepool(|_| NSString::from_str(text).precomposedStringWithCompatibilityMapping().precomposedStringWithCanonicalMapping().to_string())
}

fn display_text(text: &str) -> String {
    let bounded: String = text.chars().take(MAX_PAGE_CHARS).collect();
    let normalized = compatibility(&bounded).replace("\r\n", "\n");
    let chars: Vec<char> = normalized.chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if matches!(ch, '\0' | '\u{200b}'..='\u{200f}' | '\u{feff}') { index += 1; continue; }
        if ch == '-' && chars.get(index + 1) == Some(&'\n') && index > 0 && chars[index - 1].is_ascii_alphabetic() && chars.get(index + 2).is_some_and(char::is_ascii_alphabetic) {
            index += 2;
            continue;
        }
        if ch.is_whitespace() {
            let start = index;
            while index < chars.len() && chars[index].is_whitespace() { index += 1; }
            let line_break = chars[start..index].contains(&'\n');
            let join = line_break && output.chars().last().is_some_and(cjk) && chars.get(index).copied().is_some_and(cjk);
            if !join && !output.is_empty() && index < chars.len() { output.push(' '); }
            continue;
        }
        output.push(ch);
        index += 1;
    }
    output
}

fn fold(text: &str) -> String {
    let mut output = String::new();
    for ch in text.chars() {
        match ch {
            'ß' | 'ẞ' => output.push_str("ss"),
            'ς' => output.push('σ'),
            _ => output.extend(ch.to_lowercase()),
        }
    }
    output
}

/// Returns a bounded, case-preserving snippet and character-based highlight range.
pub fn snippet_for_query(text: &str, query: &str) -> Option<(String, usize, usize)> {
    let query = fold(&display_text(query));
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS { return None; }
    matching_snippet(text, &query)
}

fn matching_snippet(text: &str, query: &str) -> Option<(String, usize, usize)> {
    let display = display_text(text);
    let folded = fold(&display);
    let byte_start = folded.find(query)?;
    let folded_start = folded[..byte_start].chars().count();
    let folded_end = folded_start + query.chars().count();
    let chars: Vec<char> = display.chars().collect();
    let mut position = 0;
    let mut match_start = 0;
    let mut match_end = chars.len();
    for (index, ch) in chars.iter().enumerate() {
        let length = fold(&ch.to_string()).chars().count();
        if position <= folded_start && position + length > folded_start { match_start = index; }
        position += length;
        if position >= folded_end { match_end = index + 1; break; }
    }
    let start = match_start.saturating_sub(40);
    let end = (start + SNIPPET_CHARS).max(match_end).min(chars.len());
    let mut snippet = String::new();
    if start > 0 { snippet.push('…'); }
    snippet.extend(chars[start..end].iter());
    if end < chars.len() { snippet.push('…'); }
    Some((snippet, match_start - start + usize::from(start > 0), match_end - match_start))
}

pub fn derive_title(path: &Path) -> Option<String> {
    autoreleasepool(|_| {
        let document = PDFDocument::from_path(path)?;
        clean_title(&document.title()?, path.file_name()?.to_str()?)
    })
}

pub fn clean_title(value: &str, filename: &str) -> Option<String> {
    let title = display_text(value);
    let lower = title.to_lowercase();
    let length = title.chars().count();
    if !(2..=220).contains(&length) { return None; }
    if !title.chars().any(char::is_alphabetic) && !(length <= 4 && title.chars().all(|ch| ch.is_ascii_digit())) { return None; }
    let generic = lower.trim_end_matches(|ch: char| ch.is_ascii_digit() || " -_".contains(ch));
    if ["untitled", "document", "pdf", "scan", "cover", "front cover", "back cover", "body", "output", "final", "proof", "無題", "名称未設定", "新規ドキュメント", "表紙", "本文", "奥付"].contains(&generic) { return None; }
    if [".pdf", ".indd", ".idml", ".doc", ".docx", ".pptx", ".eps", ".ps"].iter().any(|suffix| lower.ends_with(suffix)) { return None; }
    if ["adobe indesign", "microsoft word", "microsoft powerpoint"].iter().any(|prefix| lower.starts_with(prefix)) { return None; }
    if title.starts_with('/') || title.starts_with("~/") { return None; }
    let digits: String = title.chars().filter(char::is_ascii_digit).collect();
    if digits.len() >= 10 && (digits.starts_with("978") || digits.starts_with("979") || lower.contains("isbn")) { return None; }
    if title.len() >= 24 && title.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-') { return None; }
    let generated = title.contains('_') || title.split(|ch: char| !ch.is_ascii_digit()).any(|part| part.len() >= 6);
    if generated && ["cover", "body", "final", "export", "proof", "print", "output", "入稿", "校了", "表紙", "本文"].iter().any(|word| lower.contains(word)) { return None; }
    if generated && lower == filename.trim_end_matches(".pdf").to_lowercase() { return None; }
    if digits.len() >= 5 && title.chars().filter(|ch| ch.is_alphabetic()).count() <= 4 && !title.chars().any(cjk) { return None; }
    Some(normalize_edition(&title))
}

fn normalize_edition(title: &str) -> String {
    let Some(start) = title.find('第') else { return title.to_owned(); };
    let tail = &title[start + '第'.len_utf8()..];
    let Some(end) = tail.find('版') else { return title.to_owned(); };
    let number: String = tail[..end].chars().filter(|ch| !ch.is_whitespace()).collect();
    if number.is_empty() || !number.chars().all(|ch| ch.is_ascii_digit()) { return title.to_owned(); }
    let prefix = title[..start].trim_end();
    format!("{}{}第{}版{}", prefix, if prefix.is_empty() { "" } else { " " }, number, &tail[end + '版'.len_utf8()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" {
        fn sqlite3_exec(database: *mut SqliteConnection, sql: *const c_char, callback: Option<unsafe extern "C" fn(*mut c_void, c_int, *mut *mut c_char, *mut *mut c_char) -> c_int>, context: *mut c_void, error: *mut *mut c_char) -> c_int;
    }

    struct FixtureDirectory(PathBuf);
    impl FixtureDirectory {
        fn new() -> Self {
            let name = format!("sumi-search-{}-{}", std::process::id(), SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for FixtureDirectory {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
    }

    fn shared(id: u64) -> Shared {
        Shared { pending: Mutex::new(None), wake: Condvar::new(), generation: AtomicU64::new(id), stopped: AtomicBool::new(false) }
    }

    fn quoted(value: &str) -> String { format!("'{}'", value.replace('\'', "''")) }

    #[test]
    fn japanese_wraps_and_width_variants_match_without_losing_display_case() {
        let (snippet, start, length) = snippet_for_query("読むときの認知\n負荷とＡＩ Designについて", "認知負荷とai design").unwrap();
        assert_eq!(snippet, "読むときの認知負荷とAI Designについて");
        assert_eq!(snippet.chars().skip(start).take(length).collect::<String>(), "認知負荷とAI Design");
        assert!(snippet_for_query("インター\nフェース", "インターフェース").is_some());
        assert!(snippet_for_query("ﾌｨｰﾄﾞﾊﾞｯｸ", "フィードバック").is_some(), "normalized {:?} / {:?}", display_text("ﾌｨｰﾄﾞﾊﾞｯｸ"), display_text("フィードバック"));
    }

    #[test]
    fn unicode_expansion_keeps_highlights_on_the_original_visible_characters() {
        let (snippet, start, length) = snippet_for_query("A Straße for reading", "STRASSE").unwrap();
        assert_eq!(snippet.chars().skip(start).take(length).collect::<String>(), "Straße");
        assert!(snippet_for_query("inter-\nface", "interface").is_some());
    }

    #[test]
    fn snippets_remain_bounded_and_queries_are_literal() {
        let content = format!("{} literal [text] {}", "前".repeat(500), "後".repeat(500));
        let (snippet, start, length) = snippet_for_query(&content, "[text]").unwrap();
        assert!(snippet.chars().count() <= SNIPPET_CHARS + 2);
        assert_eq!(snippet.chars().skip(start).take(length).collect::<String>(), "[text]");
        assert!(snippet.starts_with('…') && snippet.ends_with('…'));
        assert!(snippet_for_query("something", "   ").is_none());
        assert!(snippet_for_query("something", "^.*$").is_none());
    }

    #[test]
    fn titles_reject_production_names_but_preserve_actual_japanese_titles() {
        for name in ["oreilly-978-0-0000-0000-1e.pdf", "9780000000001", "Untitled-1", "20241104_本文_校了", "Book_export_v3", "BK_0001837", "Microsoft Word - Document1"] {
            assert_eq!(clean_title(name, "file.pdf"), None, "{name}");
        }
        assert_eq!(clean_title("架空の読書ノート八第 ４ 版", "file.pdf"), Some("架空の読書ノート八 第4版".into()));
        assert_eq!(clean_title("The Book of Proof", "file.pdf"), Some("The Book of Proof".into()));
        assert_eq!(clean_title("1984", "file.pdf"), Some("1984".into()));
    }

    #[test]
    fn replacing_requests_discards_stale_results_and_cancel_emits_nothing() {
        let engine = SearchEngine::new();
        for _ in 0..30 { engine.submit(SearchRequest { query: "old".into(), books: vec![], limit: 10 }); }
        let latest = engine.submit(SearchRequest { query: "latest".into(), books: vec![], limit: 10 });
        let deadline = Instant::now() + Duration::from_secs(2);
        let result = loop {
            if let Some(result) = engine.try_recv() { break result; }
            assert!(Instant::now() < deadline, "latest request never completed");
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(result.request_id, latest);
        assert!(result.complete);
        engine.submit(SearchRequest { query: "cancel me".into(), books: vec![], limit: 10 });
        engine.cancel();
        thread::sleep(Duration::from_millis(10));
        assert!(engine.try_recv().is_none());
    }

    #[test]
    fn indexed_search_is_scoped_bounded_and_rejects_changed_sources() {
        let directory = FixtureDirectory::new();
        let first = directory.0.join("first.pdf");
        let other = directory.0.join("other.pdf");
        std::fs::write(&first, b"%PDF-1.4\nfirst source").unwrap();
        std::fs::write(&other, b"%PDF-1.4\nother source").unwrap();
        let first = first.canonicalize().unwrap();
        let other = other.canonicalize().unwrap();
        let state = shared(1);
        let mut cache = HashMap::new();
        let sha = fingerprint(&first, &state, 1, &mut cache).unwrap();
        let database_path = directory.0.join("books.sqlite3");
        let path = CString::new(database_path.to_str().unwrap()).unwrap();
        let mut raw = std::ptr::null_mut();
        assert_eq!(unsafe { sqlite3_open_v2(path.as_ptr(), &mut raw, 2 | 4, std::ptr::null()) }, 0);
        let schema = format!("CREATE TABLE books(id TEXT, path TEXT, sha256 TEXT, page_count INTEGER); CREATE TABLE pages(book_id TEXT, page INTEGER, text TEXT, search_text TEXT); INSERT INTO books VALUES('indexed-first',{},'{}',2),('indexed-other',{},'unused',1); INSERT INTO pages VALUES('indexed-first',1,'認知\n負荷とＡＩ Design','認知負荷とai design'),('indexed-first',2,'認知負荷の続き','認知負荷の続き'),('indexed-other',1,'別の本の認知負荷','別の本の認知負荷');", quoted(first.to_str().unwrap()), sha, quoted(other.to_str().unwrap()));
        let sql = CString::new(schema).unwrap();
        assert_eq!(unsafe { sqlite3_exec(raw, sql.as_ptr(), None, std::ptr::null_mut(), std::ptr::null_mut()) }, 0);
        unsafe { sqlite3_close(raw); }
        let index = NativeIndex::open(&database_path).unwrap();
        let book = SearchBook { id: "app-book-id".into(), path: first.clone(), title: "読みやすい画面".into(), password: None };
        let results = index.search(&book, "認知負荷", 1, &state, 1, &mut cache).unwrap();
        assert_eq!(results.hits.len(), 1);
        assert_eq!(results.hits[0].book_id, "app-book-id");
        assert_eq!(results.hits[0].page, 1);
        assert_eq!(results.hits[0].snippet, "認知負荷とAI Design");
        state.generation.store(2, Ordering::Release);
        assert!(index.search(&book, "認知負荷", 1, &state, 1, &mut cache).is_none());
        std::fs::write(&first, b"%PDF-1.4\nchanged contents with a different length").unwrap();
        assert!(index.search(&book, "認知負荷", 1, &state, 2, &mut cache).is_none());
        assert!(NativeIndex::open(&directory.0.join("missing.sqlite3")).is_none());
        assert!(!directory.0.join("missing.sqlite3").exists());
    }

    #[test]
    #[ignore = "requires SUMI_TEST_BOOKS_DIR containing explicitly authorized oreilly-*.pdf books"]
    fn authorized_book_search_probe() {
        let directory = PathBuf::from(std::env::var_os("SUMI_TEST_BOOKS_DIR").expect("set SUMI_TEST_BOOKS_DIR"));
        let mut paths: Vec<PathBuf> = std::fs::read_dir(directory).unwrap().filter_map(Result::ok).map(|entry| entry.path())
            .filter(|path| path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("oreilly-")) && path.extension().is_some_and(|ext| ext == "pdf")).collect();
        paths.sort();
        assert!(!paths.is_empty());
        let books: Vec<SearchBook> = paths.iter().enumerate().map(|(index, path)| SearchBook { id: index.to_string(), path: path.clone(), title: derive_title(path).expect("PDF title metadata"), password: None }).collect();
        fn measure(engine: &SearchEngine, books: Vec<SearchBook>, case: &str, ordinal: Option<usize>) -> SearchResult {
            let start = Instant::now();
            let mut first_hit = None;
            let books_requested = books.len();
            engine.submit(SearchRequest { query: "フィードバック".into(), books, limit: 80 });
            let result = loop {
                if let Some(result) = engine.try_recv() {
                    if first_hit.is_none() && !result.hits.is_empty() { first_hit = Some(start.elapsed()); }
                    if result.complete { break result; }
                }
                assert!(start.elapsed() < Duration::from_secs(90), "search did not complete");
                thread::sleep(Duration::from_millis(2));
            };
            assert!(result.hits.len() <= 80);
            eprintln!("search_probe {}", serde_json::json!({
                "case": case, "book_ordinal": ordinal, "books_requested": books_requested,
                "hits": result.hits.len(), "result_cap": 80, "cap_reached": result.hits.len() == 80,
                "scanned_pages": result.scanned_pages, "warning_count": result.warnings.len(),
                "first_hit_ms": first_hit.map(|time| time.as_secs_f64() * 1000.),
                "total_ms": start.elapsed().as_secs_f64() * 1000.
            }));
            result
        }
        let engine = SearchEngine::new();
        for case in ["global_cold", "global_warm"] {
            let result = measure(&engine, books.clone(), case, None);
            assert!(!result.hits.is_empty());
        }
        for (index, book) in books.into_iter().enumerate() {
            let result = measure(&SearchEngine::new(), vec![book], "individual_cold", Some(index + 1));
            assert!(result.scanned_pages > 0);
            assert!(result.warnings.is_empty());
        }
    }
}
