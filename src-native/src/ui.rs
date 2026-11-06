use crate::bookshelf::{Bookshelf, BookCard};
use crate::tabs::NativeTabStrip;
use crate::activity::Activity;
use crate::pdf::PreparedDocument;
use crate::{Command, COMMANDS, enqueue, reader::Reader, search::{SearchEngine, SearchBook, SearchRequest, SearchHit}};
use objc2::{define_class, msg_send, sel, AnyThread, ClassType, MainThreadOnly, rc::Retained, runtime::{AnyObject, ProtocolObject, Sel}};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSNotification, NSString, NSArray, NSPoint, NSRect, NSSize, NSTimer, NSRunLoop, NSRunLoopCommonModes, NSMutableAttributedString, NSRange, NSURL};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, sync::{Arc, mpsc}, time::{Duration, Instant}};
use sumi_native::library::{Store, Document, ReaderPreferences, ImportReport, Session, SessionTab};
use sumi_native::thumbnails::{ThumbnailGrid,PreviewSource,COLUMNS};
use sumi_native::i18n::{text, set_language, is_japanese};

thread_local! { static UI: RefCell<Option<Ui>> = const { RefCell::new(None) }; }

fn rect(x:f64,y:f64,w:f64,h:f64)->NSRect { NSRect::new(NSPoint::new(x,y),NSSize::new(w.max(0.),h.max(0.))) }
fn s(text:&str)->Retained<NSString> { NSString::from_str(text) }
fn contains(r:NSRect,p:NSPoint)->bool { p.x>=r.origin.x && p.y>=r.origin.y && p.x<=r.origin.x+r.size.width && p.y<=r.origin.y+r.size.height }

#[derive(Default)]
pub struct DelegateIvars {}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = DelegateIvars]
    pub struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn launched(&self,_note:&NSNotification) {
            unsafe {
                match Ui::new(self) {
                    Ok(ui) => UI.with(|state| *state.borrow_mut()=Some(ui)),
                    Err(error) => { eprintln!("{error}"); NSApplication::sharedApplication(self.mtm()).terminate(None); }
                }
            }
        }
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn quit_on_close(&self,_app:&NSApplication)->bool { true }
        #[unsafe(method(application:openFiles:))]
        fn open_files(&self,_app:&NSApplication,files:&NSArray<NSString>) {
            let paths=files.iter().map(|f|PathBuf::from(f.to_string())).collect();
            COMMANDS.with(|q|q.borrow_mut().push_back(Command::Files(paths)));
        }
        #[unsafe(method(applicationShouldTerminate:))]
        fn should_terminate(&self,_app:&NSApplication)->NSApplicationTerminateReply {
            if UI.with(|state|state.borrow_mut().as_mut().is_none_or(Ui::save_before_close)) { NSApplicationTerminateReply::TerminateNow }
            else { NSApplicationTerminateReply::TerminateCancel }
        }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowShouldClose:))]
        fn should_close(&self,_window:&NSWindow)->bool { UI.with(|state|state.borrow_mut().as_mut().is_none_or(Ui::save_before_close)) }
        #[unsafe(method(windowDidResize:))]
        fn resized(&self,_note:&NSNotification){ enqueue("layout",0); }
    }
    unsafe impl NSTextFieldDelegate for Delegate {}
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn changed(&self,_note:&NSNotification){enqueue("query",0);}
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn text_command(&self,_control:&NSControl,view:&NSTextView,command:Sel)->bool {
            let composing:bool=unsafe{msg_send![view,hasMarkedText]};
            if composing {false} else if command==sel!(cancelOperation:) {enqueue("escape",0);true}
            else if command==sel!(insertNewline:) {let shifted=NSApplication::sharedApplication(self.mtm()).currentEvent().is_some_and(|e|e.modifierFlags().contains(NSEventModifierFlags::Shift));enqueue(if shifted{"accept-prev"}else{"accept"},0);true}
            else if command==sel!(moveDown:) {enqueue("row-down",0);true}
            else if command==sel!(moveUp:) {enqueue("row-up",0);true}
            else {false}
        }
    }
    impl Delegate {
        #[unsafe(method(tick:))]
        fn tick(&self,_timer:&NSTimer) {
            objc2::rc::autoreleasepool(|_| {
                UI.with(|state| {if let Ok(mut state)=state.try_borrow_mut(){if let Some(ui)=state.as_mut(){unsafe{ui.tick()}}}});
            });
        }
        #[unsafe(method(validateMenuItem:))]
        fn validate_menu(&self,item:&NSMenuItem)->bool {
            UI.with(|state|{let Ok(state)=state.try_borrow()else{return false;};let Some(ui)=state.as_ref()else{return false;};let action=CONTROLS.get(item.tag()as usize).copied().unwrap_or("");if action=="import"{return !ui.import_busy;}if action=="close-active-tab"{return ui.active_tab.is_some();}ui.screen==Screen::Reader||!matches!(action,"previous"|"next"|"pages"|"fit"|"zoom-in"|"zoom-out"|"bookmark"|"capture"|"outline"|"bookmarks"|"focus")})
        }
        #[unsafe(method(activateControl:))]
        fn action(&self,sender:&AnyObject) {let tag:isize=unsafe{msg_send![sender,tag]}; enqueue("control",tag as usize);}
        #[unsafe(method(activateRow:))]
        fn row(&self,sender:&AnyObject) {let tag:isize=unsafe{msg_send![sender,tag]};enqueue("row",tag as usize);}
        #[unsafe(method(activateBook:))]
        fn book(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};enqueue("shelf-open",tag as usize);}
        #[unsafe(method(selectTab:))]
        fn select_tab(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};enqueue("select-tab",tag as usize);}
        #[unsafe(method(closeTab:))]
        fn close_tab(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};enqueue("close-tab",tag as usize);}
        #[unsafe(method(bookAction:))]
        fn book_action(&self,sender:&AnyObject) {
            unsafe {let action:isize=msg_send![sender,tag];let id:Retained<NSString>=msg_send![sender,representedObject];COMMANDS.with(|q|q.borrow_mut().push_back(Command::BookAction(action as usize,id.to_string())));}
        }

    }
);

impl Delegate { fn new(mtm:MainThreadMarker)->Retained<Self>{unsafe{msg_send![super(Self::alloc(mtm).set_ivars(DelegateIvars::default())),init]}} }

define_class!(
    #[unsafe(super = NSWindow)]
    #[thread_kind = MainThreadOnly]
    pub struct ReaderWindow;
    impl ReaderWindow {
        #[unsafe(method(sendEvent:))]
        fn send_event(&self,event:&NSEvent) {
            unsafe {
                if event.r#type()==NSEventType::LeftMouseDown {
                    if let Some(view)=self.contentView(){
                        let point=view.convertPoint_fromView(event.locationInWindow(),None);
                        COMMANDS.with(|q|q.borrow_mut().push_back(Command::Outside(point)));
                    }
                }
                if event.r#type()==NSEventType::KeyDown {
                    let key=event.charactersIgnoringModifiers().map(|s|s.to_string()).unwrap_or_default();
                    let mods=event.modifierFlags();
                    if mods.contains(NSEventModifierFlags::Control)&&event.keyCode()==48{enqueue("cycle-tab",usize::from(mods.contains(NSEventModifierFlags::Shift)));return;}
                    if mods.contains(NSEventModifierFlags::Command)&&!mods.contains(NSEventModifierFlags::Control) {
                        if let Ok(number)=key.parse::<usize>(){if (1..=9).contains(&number){enqueue("number-tab",number);return;}}
                        let command=match key.to_lowercase().as_str(){"f" if mods.contains(NSEventModifierFlags::Shift)=>Some("focus"),"f"=>Some("find"),"k"=>Some("palette"),"t"|"l"=>Some("library"),"w"=>Some("close-active-tab"),"[" if mods.contains(NSEventModifierFlags::Shift)=>Some("previous-tab"),"]" if mods.contains(NSEventModifierFlags::Shift)=>Some("next-tab"),"o"=>Some("import"),"g"=>Some("pages"),"0"=>Some("fit"),"+"|"="=>Some("zoom-in"),"-"=>Some("zoom-out"),"d"=>Some("bookmark"),"x" if mods.contains(NSEventModifierFlags::Shift)=>Some("capture"),_=>None};
                        if let Some(command)=command {enqueue(command,0);return;}
                    }
                    if event.keyCode()==53 {let composing=self.firstResponder().is_some_and(|r|r.isKindOfClass(NSTextView::class())&&msg_send![&r,hasMarkedText]);if !composing{enqueue("escape",0);return;}}
                    let editing=self.firstResponder().is_some_and(|r|r.isKindOfClass(NSTextView::class()));
                    if !editing && !mods.contains(NSEventModifierFlags::Command) && !mods.contains(NSEventModifierFlags::Option) {
                        match event.keyCode(){123=>{enqueue("left",0);return;},124=>{enqueue("right",0);return;},125=>{enqueue("row-down",0);return;},126=>{enqueue("row-up",0);return;},36|76=>{enqueue("accept",0);return;},49=>{enqueue("space",usize::from(mods.contains(NSEventModifierFlags::Shift)));return;},_=>{}}
                        if key=="g" {enqueue("pages",0);return;}
                    }
                }
                let _:()=msg_send![super(self),sendEvent:event];
            }
        }
    }
);

define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    pub struct RootView;
    impl RootView {
        #[unsafe(method(draggingEntered:))]
        fn dragging_entered(&self,_sender:&AnyObject)->usize {1}
        #[unsafe(method(performDragOperation:))]
        fn perform_drop(&self,sender:&AnyObject)->bool {
            unsafe{
                let pasteboard:Retained<NSPasteboard>=msg_send![sender,draggingPasteboard];
                let names:Option<Retained<NSArray<NSString>>>=msg_send![&pasteboard,propertyListForType:&*s("NSFilenamesPboardType")];
                let mut paths:Vec<PathBuf>=names.map(|names|names.iter().map(|p|PathBuf::from(p.to_string())).collect()).unwrap_or_default();
                if paths.is_empty(){
                    let items:Option<Retained<NSArray<AnyObject>>>=msg_send![&pasteboard,pasteboardItems];
                    if let Some(items)=items{for item in items.iter(){
                        let value:Option<Retained<NSString>>=msg_send![&*item,stringForType:&*s("public.file-url")];
                        if let Some(value)=value{if let Some(url)=NSURL::URLWithString(&value){if url.isFileURL(){if let Some(path)=url.path(){paths.push(PathBuf::from(path.to_string()));}}}}
                    }}
                }
                if paths.is_empty(){false}else{COMMANDS.with(|q|q.borrow_mut().push_back(Command::Files(paths)));true}
            }
        }
    }
);

#[derive(Clone,Copy,PartialEq,Eq)]
enum Panel {None,Find,Palette,Pages,Outline,Bookmarks,ImportErrors}
#[derive(Clone,Copy,PartialEq,Eq)]
enum Screen { Shelf, Reader }
#[derive(Clone)]
enum RowAction {Command(&'static str),Book(String),Page(usize),Hit(SearchHit),None}
#[derive(Clone)]
struct Row {title:String,detail:String,action:RowAction}

struct PaletteCommand {
    title: (&'static str, &'static str),
    detail: (&'static str, &'static str),
    action: &'static str,
    aliases: &'static str,
}
impl PaletteCommand {
    fn matches(&self, query: &str) -> bool {
        [self.title.0,self.title.1,self.detail.0,self.detail.1,self.action,self.aliases]
            .iter().any(|value|crate::search::matches_query(value,query))
    }
    fn row(&self) -> Row {
        Row { title:text(self.title.0,self.title.1).into(), detail:text(self.detail.0,self.detail.1).into(), action:RowAction::Command(self.action) }
    }
}

fn palette_commands(preferences: &ReaderPreferences) -> Vec<PaletteCommand> {
    let command=|ja,en,detail_ja,detail_en,action,aliases|PaletteCommand{title:(ja,en),detail:(detail_ja,detail_en),action,aliases};
    let appearance=|selected|if selected{("外観 · 選択中","Appearance · Selected")}else{("外観を変更","Change Appearance")};
    let language=|selected|if selected{("表示言語 · 選択中","Display Language · Selected")}else{("表示言語を変更","Change Display Language")};
    let white=appearance(preferences.theme=="white");let paper=appearance(preferences.theme=="paper");let night=appearance(preferences.theme=="night");
    let system=language(preferences.language=="system");let ja=language(preferences.language=="ja");let en=language(preferences.language=="en");
    vec![
        command("この本の本文を検索","Find in This Book","⌘F","⌘F","find","本文 検索 search text"),
        command("ページを選ぶ","Choose a Page","G / ⌘G","G / ⌘G","pages","ページ一覧 thumbnails"),
        command("目次を開く","Open Contents","章や見出しに移動","Go to a Chapter or Heading","outline","目次 outline table of contents"),
        command("しおりを開く","Open Bookmarks","保存したページに移動","Go to a Saved Page","bookmarks","しおり"),
        command("しおりを切り替える","Toggle Bookmark","⌘D","⌘D","bookmark","しおり"),
        command(if preferences.mode=="spread"{"1ページにする"}else{"見開きにする"},if preferences.mode=="spread"{"Show One Page"}else{"Show Two Pages"},"単ページ・見開き · すべての本に適用","Page Layout · Applies to All Books","spread","見開き 単ページ single page spread two pages"),
        command("全体表示に戻す","Fit Page","⌘0","⌘0","fit","全体表示 reset zoom"),
        command("拡大","Zoom In","⌘+","⌘+","zoom-in","拡大"),
        command("縮小","Zoom Out","⌘−","⌘−","zoom-out","縮小"),
        command("範囲を画像でコピー","Copy an Area as an Image","切り抜き · ⌘⇧X","Capture · ⌘⇧X","capture","スクリーンショット screenshot"),
        command("切り抜いた画像を保存…","Save Captured Image…","最後にコピーした範囲をPNGで保存","Save the Last Capture as a PNG","save-capture","画像 保存 screenshot PNG save image"),
        command("選んだ文章をコピー","Copy Selected Text","書名・ページ付き","Includes the Book Title and Page","copy-selection","選択 文章 selection"),
        command("ページの文章をコピー","Copy Page Text","書名・ページ付き","Includes the Book Title and Page","copy-page","本文 文章"),
        command("ライト","Light",white.0,white.1,"white","外観 appearance theme"),
        command("セピア","Sepia",paper.0,paper.1,"paper","外観 appearance theme"),
        command("ダーク","Dark",night.0,night.1,"night","外観 appearance theme night"),
        command(if preferences.direction=="rtl"{"左から右に読む"}else{"右から左に読む"},if preferences.direction=="rtl"{"Read Left to Right"}else{"Read Right to Left"},"めくる方向を切り替える","Change Reading Direction","direction","読む方向 左 右 reading direction rtl ltr"),
        command("読書に集中","Focus on Reading","⌘⇧F","⌘⇧F","focus","集中 focus mode"),
        command("本を追加…","Add Books…","PDF・フォルダをまとめて取り込む · ⌘O","Add PDFs or Folders · ⌘O","import","本 追加 取り込む import library"),
        command("本棚へ戻る","Go to Library","表紙から本を選ぶ · ⌘L","Choose a Book by its Cover · ⌘L","library","本棚 home shelf"),
        command("言語：システム設定に従う","Language: Use System Setting",system.0,system.1,"language-system","言語 システム language automatic default"),
        command("言語：日本語","Language: Japanese",ja.0,ja.1,"language-ja","言語 日本語 Japanese"),
        command("言語：英語","Language: English",en.0,en.1,"language-en","言語 英語 English"),
    ]
}

fn language_for_action(action: &str) -> Option<&'static str> {
    match action {"language-system"=>Some("system"),"language-ja"=>Some("ja"),"language-en"=>Some("en"),_=>None}
}
fn page_label(page: usize) -> String {if is_japanese(){format!("{page}ページ")}else{format!("Page {page}")}}
fn page_total(count: usize) -> String {if is_japanese(){format!("{count}ページ")}else{format!("{count} {}",if count==1{"page"}else{"pages"})}}
fn match_count(count: usize) -> String {if is_japanese(){format!("{count}件")}else{format!("{count} {}",if count==1{"match"}else{"matches"})}}
fn page_query(query: &str, count: usize) -> Option<usize> {
    let normalized=objc2::rc::autoreleasepool(|_|s(query).precomposedStringWithCompatibilityMapping().to_string());
    let query=normalized.trim().to_lowercase();
    let number=query.strip_prefix("page").or_else(||query.strip_prefix("p.")).or_else(||query.strip_prefix('p')).unwrap_or(&query).trim();
    number.strip_suffix("ページ").unwrap_or(number).trim().parse::<usize>().ok().filter(|page|*page>0&&*page<=count)
}
fn panel_labels(kind: Panel) -> (&'static str, &'static str) {
    match kind {
        Panel::Find=>(text("本文を検索","Find in Book"),text("この本の中を検索","Find in This Book")),
        Panel::Palette=>(text("検索","Search"),text("本・本文・操作を検索","Search Books, Text, or Commands")),
        Panel::ImportErrors=>(text("取り込み結果","Import Results"),""),
        Panel::Pages=>(text("ページ","Pages"),text("ページ番号","Page Number")),
        Panel::Outline=>(text("目次","Contents"),text("見出しを検索","Search Headings")),
        Panel::Bookmarks=>(text("しおり","Bookmarks"),text("ページ番号で絞り込む","Filter by Page Number")),
        Panel::None=>("",""),
    }
}

#[derive(Clone)]
struct OpenTab { id:String, title:String, password:Option<String>, zoom:f64, find:String }

#[derive(Clone)]
struct OpenTask {
    sequence: u64,
    id: String,
    page: Option<usize>,
    password: Option<String>,
    highlight: Option<String>,
}
struct OpenedBook { document: PreparedDocument, title: Option<String> }


#[cfg(debug_assertions)]
struct Smoke { directory:PathBuf,books:Vec<String>,book:usize,phase:usize,expected:usize,due:Instant,checks:Vec<serde_json::Value> }

struct Ui {
    #[cfg(debug_assertions)]
    smoke:Option<Smoke>,
    mtm:MainThreadMarker, delegate:Retained<Delegate>, window:Retained<ReaderWindow>, root:Retained<RootView>, reader:Reader,
    store:Arc<Store>, preferences:ReaderPreferences, current:Option<Document>, password:Option<String>,
    tabs:Vec<OpenTab>, active_tab:Option<String>, tab_strip:NativeTabStrip, status:Retained<NSTextField>, page_button:Retained<NSButton>, zoom_button:Retained<NSButton>, nav_button:Retained<NSButton>, command_button:Retained<NSButton>,
    top:Retained<NSView>, footer:Retained<NSView>, panel:Retained<NSView>, query:Retained<NSTextField>, panel_title:Retained<NSTextField>, panel_hint:Retained<NSTextField>, rows_scroll:Retained<NSScrollView>, rows_view:Retained<NSView>,
    thumbnails:ThumbnailGrid,
    screen:Screen, bookshelf:Bookshelf, empty_shelf:Retained<NSTextField>,
    panel_kind:Panel, rows:Vec<Row>, row_buttons:Vec<Retained<NSButton>>, selected:usize, search:SearchEngine, request_id:u64, search_due:Option<Instant>, search_running:bool, text_hits:Vec<SearchHit>,
    dirty:Option<Instant>, last_page:usize,last_zoom:usize,size:NSSize,focus:bool,toast_until:Option<Instant>,
    last_capture:Option<Retained<NSImage>>,capture_name:String,last_find:String,last_find_selected:Option<usize>,
    pending_open:Option<PathBuf>,
    opening:Option<OpenTask>, open_receiver:Option<(u64,mpsc::Receiver<Result<OpenedBook,String>>)>, open_sequence:u64,
    open_activity:Activity, import_activity:Activity, search_activity:Activity, cover_activity:Activity, thumbnail_activity:Activity,
    activity_indicator:Retained<NSProgressIndicator>, panel_indicator:Retained<NSProgressIndicator>,
    import_receiver:Option<mpsc::Receiver<Result<ImportReport,String>>>, import_busy:bool,
}

unsafe fn decorate(view:&NSView,color:&NSColor,radius:f64){view.setWantsLayer(true); let layer:Option<Retained<AnyObject>>=msg_send![view,layer]; if let Some(layer)=layer{let _:()=msg_send![&layer,setCornerRadius:radius];let cg:*const objc2_core_graphics::CGColor=msg_send![color,CGColor];let _:()=msg_send![&layer,setBackgroundColor:cg];}}
unsafe fn label(mtm:MainThreadMarker,text:&str,size:f64)->Retained<NSTextField>{let v=NSTextField::labelWithString(&s(text),mtm);v.setFont(Some(&NSFont::systemFontOfSize(size)));v.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingTail);v}
unsafe fn progress_indicator(mtm:MainThreadMarker)->Retained<NSProgressIndicator>{
    let indicator=NSProgressIndicator::initWithFrame(NSProgressIndicator::alloc(mtm),rect(0.,0.,16.,16.));
    indicator.setStyle(NSProgressIndicatorStyle::Spinning);
    indicator.setControlSize(NSControlSize::Small);
    indicator.setIndeterminate(true);
    indicator.setDisplayedWhenStopped(false);
    indicator.setHidden(true);
    indicator.setAccessibilityIdentifier(Some(&s("sumi-loading")));
    indicator
}
unsafe fn show_progress(indicator:&NSProgressIndicator, message:Option<&str>){
    if let Some(message)=message{
        indicator.setToolTip(Some(&s(message)));indicator.setAccessibilityLabel(Some(&s(message)));
        if indicator.isHidden(){indicator.setHidden(false);indicator.startAnimation(None);}
    }else if !indicator.isHidden(){indicator.stopAnimation(None);indicator.setHidden(true);}
}
unsafe fn button(delegate:&Delegate,text:&str,symbol:Option<&str>,tag:usize)->Retained<NSButton>{
    let b=NSButton::buttonWithTitle_target_action(&s(text),Some(delegate),Some(sel!(activateControl:)),delegate.mtm());
    b.setTag(tag as isize);b.setBezelStyle(NSBezelStyle::AccessoryBarAction);b.setBordered(false);b.setFont(Some(&NSFont::systemFontOfSize(12.)));
    if let Some(symbol)=symbol {if let Some(image)=crate::icons::icon(match symbol{"plus"=>"folder-input","magnifyingglass"=>"search","xmark"=>"x",other=>other}){b.setImage(Some(&image));b.setImagePosition(NSCellImagePosition::ImageOnly);}}
    b.setToolTip(Some(&s(text)));b.setAccessibilityLabel(Some(&s(text))); b
}
unsafe fn row_title(row:&Row,_mtm:MainThreadMarker)->Retained<NSMutableAttributedString>{
    let text=format!("{}\n{}",row.title,row.detail);let value=NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(),&s(&text));let full=NSRange::new(0,text.encode_utf16().count());let detail=NSRange::new(row.title.encode_utf16().count()+1,row.detail.encode_utf16().count());
    value.addAttribute_value_range(&s("NSFont"),&NSFont::systemFontOfSize(13.),full);value.addAttribute_value_range(&s("NSColor"),&NSColor::labelColor(),full);value.addAttribute_value_range(&s("NSFont"),&NSFont::systemFontOfSize(11.),detail);value.addAttribute_value_range(&s("NSColor"),&NSColor::secondaryLabelColor(),detail);
    let paragraph=NSMutableParagraphStyle::new();paragraph.setFirstLineHeadIndent(12.);paragraph.setHeadIndent(12.);paragraph.setTailIndent(-12.);paragraph.setLineSpacing(3.);paragraph.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);value.addAttribute_value_range(&s("NSParagraphStyle"),&paragraph,full);value
}
const CONTROLS:[&str;28]=["library","find","palette","tools","previous","next","pages","spread","zoom-in","fit","zoom-out","close","import","outline","bookmarks","bookmark","capture","white","paper","night","direction","focus","close-active-tab","previous-tab","next-tab","language-system","language-ja","language-en"];

impl Ui {
    unsafe fn new(delegate:&Delegate)->Result<Self,String>{
        let mtm=delegate.mtm();let store=Arc::new(Store::load_default()?);let session=store.session()?;let library=store.public_library()?;let preferences=library.preferences;set_language(&preferences.language);
        let window:Retained<ReaderWindow>=msg_send![ReaderWindow::alloc(mtm),initWithContentRect:rect(0.,0.,1160.,800.),styleMask:(NSWindowStyleMask::Titled|NSWindowStyleMask::Closable|NSWindowStyleMask::Miniaturizable|NSWindowStyleMask::Resizable|NSWindowStyleMask::FullSizeContentView),backing:NSBackingStoreType::Buffered,defer:false];
        window.setReleasedWhenClosed(false);window.setTitle(&s("Sumi"));window.setTitleVisibility(NSWindowTitleVisibility::Hidden);window.setTitlebarAppearsTransparent(true);window.setContentMinSize(NSSize::new(680.,480.));window.setDelegate(Some(ProtocolObject::from_ref(delegate)));
        let root:Retained<RootView>=msg_send![RootView::alloc(mtm),initWithFrame:rect(0.,0.,1160.,800.)];
        root.registerForDraggedTypes(&NSArray::from_slice(&[&*s("NSFilenamesPboardType"),&*s("public.file-url")]));
        window.setContentView(Some(&root));
        let reader=Reader::new(mtm,rect(0.,44.,1160.,714.)); root.addSubview(reader.view());
        let top=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,762.,1160.,38.));root.addSubview(&top);
        let tab_strip=NativeTabStrip::new(mtm,delegate);top.addSubview(tab_strip.view());
        let nav_button=button(delegate,text("本を追加","Add Books"),Some("plus"),12);top.addSubview(&nav_button);
        let command_button=button(delegate,text("検索・操作","Search and Commands"),Some("magnifyingglass"),2);top.addSubview(&command_button);
        let footer=NSView::initWithFrame(NSView::alloc(mtm),rect(500.,8.,160.,30.));root.addSubview(&footer);
        let page_button=button(delegate,text("ページを選ぶ","Choose a Page"),None,6);footer.addSubview(&page_button);
        let zoom_button=button(delegate,text("全体表示に戻す","Fit Page"),None,9);footer.addSubview(&zoom_button);
        let empty_shelf=label(mtm,text("PDFをドロップ","Drop PDFs Here"),14.);empty_shelf.setAlignment(NSTextAlignment::Center);empty_shelf.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let bookshelf=Bookshelf::new(mtm,delegate);root.addSubview(bookshelf.view());root.addSubview(&empty_shelf);
        let status=label(mtm,text("PDFやフォルダをドロップ、または⌘Oで本を取り込む","Drop PDFs or folders here, or press ⌘O to add books"),12.);status.setAlignment(NSTextAlignment::Center);root.addSubview(&status);status.setHidden(true);
        let panel=NSView::initWithFrame(NSView::alloc(mtm),rect(300.,300.,560.,390.));root.addSubview(&panel);panel.setHidden(true);
        let panel_title=label(mtm,text("すべて検索","Search All Books"),13.);panel.addSubview(&panel_title);
        let close=button(delegate,text("閉じる · Esc","Close · Esc"),Some("xmark"),11);panel.addSubview(&close);
        let query=NSTextField::initWithFrame(NSTextField::alloc(mtm),rect(16.,330.,528.,30.));query.setFont(Some(&NSFont::systemFontOfSize(16.)));query.setBezeled(false);query.setDrawsBackground(false);query.setDelegate(Some(ProtocolObject::from_ref(delegate)));query.setFocusRingType(NSFocusRingType::None);panel.addSubview(&query);
        let rows_scroll=NSScrollView::initWithFrame(NSScrollView::alloc(mtm),rect(8.,36.,544.,270.));rows_scroll.setDrawsBackground(false);rows_scroll.setHasVerticalScroller(true);rows_scroll.setAutohidesScrollers(true);panel.addSubview(&rows_scroll);
        let rows_view=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,0.,528.,270.));rows_scroll.setDocumentView(Some(&rows_view));
        let panel_hint=label(mtm,text("↑↓ 選択　↵ 開く　Esc 閉じる","↑↓ Select   ↵ Open   Esc Close"),11.);panel.addSubview(&panel_hint);
        let thumbnails=ThumbnailGrid::new(mtm,delegate);
        let activity_indicator=progress_indicator(mtm);root.addSubview(&activity_indicator);
        let panel_indicator=progress_indicator(mtm);panel.addSubview(&panel_indicator);
        let mut ui=Self{#[cfg(debug_assertions)]smoke:None,mtm,delegate:Retained::retain(delegate as *const Delegate as *mut Delegate).unwrap(),window,root,reader,store,preferences,current:None,password:None,tabs:vec![],active_tab:None,tab_strip,status,page_button,zoom_button,nav_button,command_button,top,footer,panel,query,panel_title,panel_hint,rows_scroll,rows_view,thumbnails,screen:Screen::Shelf,bookshelf,empty_shelf,panel_kind:Panel::None,rows:vec![],row_buttons:vec![],selected:0,search:SearchEngine::new(),request_id:0,search_due:None,search_running:false,text_hits:vec![],dirty:None,last_page:0,last_zoom:0,size:NSSize::new(0.,0.),focus:false,toast_until:None,last_capture:None,capture_name:String::new(),last_find:String::new(),last_find_selected:None,pending_open:None,opening:None,open_receiver:None,open_sequence:0,open_activity:Activity::default(),import_activity:Activity::default(),search_activity:Activity::default(),cover_activity:Activity::default(),thumbnail_activity:Activity::default(),activity_indicator,panel_indicator,import_receiver:None,import_busy:false};
        ui.apply_preferences();ui.layout();ui.window.center();ui.window.makeKeyAndOrderFront(None);
        let app=NSApplication::sharedApplication(mtm);app.setActivationPolicy(NSApplicationActivationPolicy::Regular);#[allow(deprecated)]app.activateIgnoringOtherApps(true);
        let timer:Retained<NSTimer>=msg_send![NSTimer::class(),scheduledTimerWithTimeInterval:0.0166666667,target:delegate,selector:sel!(tick:),userInfo:std::ptr::null::<AnyObject>(),repeats:true];NSRunLoop::mainRunLoop().addTimer_forMode(&timer,NSRunLoopCommonModes);
        ui.setup_menu();
        ui.restore_session(session);
        let args:Vec<PathBuf>=std::env::args_os().skip(1).filter(|arg| !arg.to_string_lossy().starts_with('-')).map(PathBuf::from).filter(|p|p.exists()).collect();if !args.is_empty(){ui.pending_open=args.first().filter(|p|p.is_file()).cloned();ui.import(args);}
        #[cfg(debug_assertions)]
        if let (Some(directory),Some(isolated))=(std::env::var_os("SUMI_NATIVE_SMOKE_DIR"),std::env::var_os("SUMI_LIBRARY_PATH")){
            if PathBuf::from(isolated).components().any(|part|part.as_os_str()=="test-artifacts"){
                let books=ui.store.snapshot()?.recent.into_iter().filter(|entry|entry.document.name.starts_with("oreilly-")).map(|entry|entry.document.id).collect();
                ui.smoke=Some(Smoke{directory:PathBuf::from(directory),books,book:0,phase:0,expected:1,due:Instant::now()+Duration::from_secs(1),checks:vec![]});
            }
        }
        Ok(ui)
    }
    unsafe fn restore_session(&mut self,session:Session){
        self.tabs=session.tabs.into_iter().filter_map(|tab|self.store.entry(&tab.book_id).ok().map(|entry|OpenTab{
            id:tab.book_id,title:entry.document.title,password:None,zoom:tab.zoom,find:tab.find,
        })).collect();
        self.active_tab=session.active_book_id.filter(|id|self.tabs.iter().any(|tab|&tab.id==id));
        self.refresh_shelf();self.layout();
        if let Some(id)=self.active_tab.clone(){self.open_book(&id,None);}
        else{self.window.setTitle(&s(text("本棚 — Sumi","Library — Sumi")));}
    }
    fn session(&self)->Session{
        Session{tabs:self.tabs.iter().map(|tab|SessionTab{book_id:tab.id.clone(),zoom:tab.zoom,find:tab.find.clone()}).collect(),active_book_id:self.active_tab.clone()}
    }
    fn persist_session(&mut self)->bool{
        if let Err(error)=self.store.save_session(&self.session()){
            self.dirty=Some(Instant::now()+Duration::from_secs(2));
            unsafe{self.toast(&format!("{}: {error}",text("タブを保存できません","Could not save your tabs")));}false
        }else{true}
    }
    unsafe fn update_activity(&mut self){
        self.open_activity.set_active(self.opening.is_some());
        self.import_activity.set_active(self.import_busy);
        self.search_activity.set_active(self.search_running&&matches!(self.panel_kind,Panel::Find|Panel::Palette));
        self.cover_activity.set_active(self.screen==Screen::Shelf&&self.panel_kind==Panel::None&&self.bookshelf.is_loading());
        self.thumbnail_activity.set_active(self.panel_kind==Panel::Pages&&self.thumbnails.is_loading());
        let global=if self.open_activity.visible(){Some(text("本を開いています… · Escで取消","Opening book… · Esc to cancel"))}
            else if self.import_activity.visible(){Some(text("本を取り込んでいます…","Adding books…"))}
            else if self.cover_activity.visible(){Some(text("表紙を読み込んでいます…","Loading covers…"))}else{None};
        let panel=if self.search_activity.visible(){Some(text("検索中…","Searching…"))}
            else if self.thumbnail_activity.visible(){Some(text("ページを読み込んでいます…","Loading pages…"))}else{None};
        show_progress(&self.activity_indicator,global);show_progress(&self.panel_indicator,panel);
    }
    unsafe fn setup_menu(&self){
        let bar=NSMenu::new(self.mtm);
        for (name,items) in [("Sumi",vec![(text("Sumiを終了","Quit Sumi"),"quit","q")]),(text("ファイル","File"),vec![(text("本を取り込む…","Add Books…"),"import","o"),(text("ホーム","Home"),"library","l"),(text("タブを閉じる","Close Tab"),"close-active-tab","w")]),(text("編集","Edit"),vec![(text("取り消す","Undo"),"undo","z"),(text("切り取り","Cut"),"cut","x"),(text("コピー","Copy"),"copy","c"),(text("ペースト","Paste"),"paste","v"),(text("すべて選択","Select All"),"select-all","a"),(text("本文を検索","Find in Book"),"find","f"),(text("検索・操作","Search and Commands"),"palette","k")]),(text("表示","View"),vec![(text("ページを選ぶ","Choose a Page"),"pages","g"),(text("全体表示","Fit Page"),"fit","0"),(text("拡大","Zoom In"),"zoom-in","+"),(text("縮小","Zoom Out"),"zoom-out","-"),(text("見開きを切り替える","Toggle Two Pages"),"spread",""),(text("前のタブ","Previous Tab"),"previous-tab",""),(text("次のタブ","Next Tab"),"next-tab","")])]{
            let menu=NSMenu::initWithTitle(NSMenu::alloc(self.mtm),&s(name));
            if name=="Sumi" {
                let title=text("言語 / Language","Language / 言語");
                let languages=NSMenu::initWithTitle(NSMenu::alloc(self.mtm),&s(title));
                for (title,action) in [(text("システム設定に従う","Use System Language"),"language-system"),("日本語","language-ja"),("English","language-en")] {
                    let item=NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm),&s(title),Some(sel!(activateControl:)),&s(""));
                    item.setTag(CONTROLS.iter().position(|value|*value==action).unwrap() as isize);item.setTarget(Some(&*self.delegate));
                    item.setState(if language_for_action(action)==Some(self.preferences.language.as_str()){1}else{0});
                    languages.addItem(&item);
                }
                let item=NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm),&s(title),None,&s(""));
                item.setSubmenu(Some(&languages));menu.addItem(&item);
            }
            for (title,action,key) in items {
                let item=NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm),&s(title),Some(match action{"quit"=>sel!(terminate:),"copy"=>sel!(copy:),"cut"=>sel!(cut:),"paste"=>sel!(paste:),"select-all"=>sel!(selectAll:),"undo"=>sel!(undo:),_=>sel!(activateControl:)}),&s(key));
                if let Some(tag)=CONTROLS.iter().position(|x|*x==action){item.setTag(tag as isize);item.setTarget(Some(&*self.delegate));}
                menu.addItem(&item);
            }
            let item=NSMenuItem::new(self.mtm);item.setSubmenu(Some(&menu));bar.addItem(&item);
        }
        NSApplication::sharedApplication(self.mtm).setMainMenu(Some(&bar));
    }
    unsafe fn layout(&mut self){
        let size=self.root.bounds().size;self.size=size;
        let shelf=self.screen==Screen::Shelf;
        let focused=self.focus&&!shelf;
        let top_h=if focused{0.}else{38.};let bottom_h=if focused{0.}else{40.};
        self.reader.resize(rect(8.,bottom_h,size.width-16.,size.height-top_h-bottom_h-5.));
        self.reader.view().setHidden(shelf);self.reader.set_interaction_enabled(!shelf&&self.panel_kind==Panel::None&&self.opening.is_none());
        self.top.setFrame(rect(0.,size.height-38.,size.width,38.));self.top.setHidden(focused);self.footer.setHidden(focused||shelf);
        self.tab_strip.view().setFrameOrigin(NSPoint::new(88.,0.));
        let tabs:Vec<_>=self.tabs.iter().map(|tab|(tab.id.clone(),tab.title.clone())).collect();
        self.tab_strip.show(&tabs,self.active_tab.as_deref(),size.width-208.,38.);
        self.activity_indicator.setFrame(rect(size.width-103.,size.height-27.,16.,16.));
        self.nav_button.setFrame(rect(size.width-80.,4.,30.,30.));self.command_button.setFrame(rect(size.width-42.,4.,30.,30.));
        self.nav_button.setEnabled(!self.import_busy);self.nav_button.setTag(12);self.nav_button.setImagePosition(NSCellImagePosition::ImageOnly);self.command_button.setImagePosition(NSCellImagePosition::ImageOnly);
        self.layout_footer();
        self.bookshelf.view().setHidden(!shelf);self.empty_shelf.setHidden(!shelf||self.bookshelf.id_at(0).is_some());
        self.empty_shelf.setFrame(rect(24.,size.height/2.-14.,size.width-48.,28.));
        self.bookshelf.view().setFrameOrigin(NSPoint::new(16.,16.));self.bookshelf.layout(size.width-32.,(size.height-64.).max(80.));
        self.status.setFrame(rect(16.,if shelf{10.}else{44.},size.width-32.,22.));
        let w=if self.panel_kind==Panel::Find{480.0_f64}else if self.panel_kind==Panel::Pages{660.0_f64}else{580.0_f64}.min(size.width-32.);
        let h=if self.panel_kind==Panel::Find{if self.query.stringValue().is_empty(){48.0_f64}else{340.0_f64}}else if self.panel_kind==Panel::Pages{684.0_f64}else{440.0_f64}.min(size.height-92.);
        let x=if self.panel_kind==Panel::Find{size.width-w-16.}else{(size.width-w)/2.};let y=size.height-h-52.;
        self.panel.setFrame(rect(x,y,w,h));self.panel_title.setFrame(rect(18.,h-30.,w-68.,20.));
        self.panel.subviews().objectAtIndex(1).setFrame(rect(w-42.,h-34.,28.,28.));
        let finding=self.panel_kind==Panel::Find;let compact=matches!(self.panel_kind,Panel::Find|Panel::Palette|Panel::Pages);let empty_find=finding&&self.query.stringValue().is_empty();self.panel_title.setHidden(compact);self.rows_scroll.setHidden(empty_find);self.panel_hint.setHidden(empty_find);
        if compact {self.panel.subviews().objectAtIndex(1).setFrame(rect(w-40.,h-38.,28.,28.));self.query.setFrame(rect(16.,h-37.,w-84.,26.));}else{self.query.setFrame(rect(18.,h-70.,w-36.,30.));}
        self.rows_scroll.setFrame(rect(8.,34.,w-16.,(h-if compact{86.}else{112.}).max(0.)));self.panel_hint.setFrame(rect(18.,10.,w-36.,18.));
        self.panel_indicator.setFrame(rect(w-65.,h-31.,16.,16.));
        self.render_rows();
    }
    unsafe fn layout_footer(&self){
        let enlarged=self.reader.zoom_factor()>1.01;
        let width=if enlarged{286.}else{150.};self.footer.setFrame(rect((self.size.width-width)/2.,5.,width,30.));
        self.page_button.setFrame(rect(6.,1.,138.,28.));self.zoom_button.setHidden(!enlarged);self.zoom_button.setFrame(rect(150.,1.,130.,28.));
        if enlarged{self.zoom_button.setTitle(&s(&format!("{}% · {}",(self.reader.zoom_factor()*100.).round()as usize,text("全体に戻す","Fit Page"))));}
    }
    unsafe fn show_shelf(&mut self){
        self.cancel_open();self.close_panel();self.reader.set_interaction_enabled(false);if !self.save(){self.reader.set_interaction_enabled(self.screen==Screen::Reader);return;}self.screen=Screen::Shelf;self.active_tab=None;self.persist_session();self.focus=false;self.close_panel();self.window.setTitle(&s(text("本棚 — Sumi","Library — Sumi")));self.refresh_shelf();self.layout();self.window.makeFirstResponder(Some(self.bookshelf.view()));
    }
    unsafe fn show_reader(&mut self){
        if let Some(book)=&self.current{self.active_tab=Some(book.id.clone());self.screen=Screen::Reader;self.window.setTitle(&s(&format!("{} — Sumi",book.title)));self.layout();self.reader.focus();self.persist_session();}
    }
    unsafe fn refresh_shelf(&mut self){
        if let Ok(library)=self.store.snapshot(){
            let current=self.current.as_ref().map(|b|b.id.as_str()).or(library.last_opened_id.as_deref());
            let cards:Vec<BookCard>=library.recent.into_iter().map(|entry|{let book=entry.document;BookCard{id:book.id.clone(),title:book.title,path:entry.path,revision:(book.size,book.modified_at.to_bits()),page:book.state["page"].as_u64().unwrap_or(1)as usize,current:current==Some(book.id.as_str())}}).collect();
            self.bookshelf.show(&cards,self.size.width-32.,(self.size.height-64.).max(80.));self.empty_shelf.setHidden(self.screen!=Screen::Shelf||!cards.is_empty());
        }
    }
    unsafe fn select_tab(&mut self,index:usize){
        if index==0{self.show_shelf();}else if let Some(id)=self.tabs.get(index-1).map(|tab|tab.id.clone()){self.open_book(&id,None);}
    }
    unsafe fn close_tab(&mut self,index:usize){
        let Some(tab)=self.tabs.get(index)else{return;};let loaded=self.current.as_ref().is_some_and(|book|book.id==tab.id);let active=self.active_tab.as_deref()==Some(tab.id.as_str());
        if loaded{self.close_panel();self.reader.set_interaction_enabled(false);if !self.save(){self.reader.set_interaction_enabled(self.screen==Screen::Reader);return;}self.reader.clear();self.thumbnails.clear();self.current=None;self.password=None;self.last_find.clear();}
        if self.opening.as_ref().is_some_and(|task|task.id==self.tabs[index].id){self.cancel_open();}
        self.tabs.remove(index);
        if self.active_tab.as_ref().is_some_and(|id|!self.tabs.iter().any(|tab|&tab.id==id)){self.active_tab=None;}
        if active{self.show_shelf();if !self.tabs.is_empty(){self.open_book(&self.tabs[index.min(self.tabs.len()-1)].id.clone(),None);}}else{self.layout();}self.persist_session();
    }
    unsafe fn apply_preferences(&mut self){
        let dark=self.preferences.theme=="night";let appearance=NSAppearance::appearanceNamed(&s(if dark{"NSAppearanceNameDarkAqua"}else{"NSAppearanceNameAqua"}));self.window.setAppearance(appearance.as_deref());
        let bg=if dark{NSColor::colorWithSRGBRed_green_blue_alpha(0.12,0.13,0.14,1.)}else{NSColor::colorWithSRGBRed_green_blue_alpha(0.92,0.92,0.90,1.)};self.window.setBackgroundColor(Some(&bg));
        let chrome=if dark{NSColor::colorWithSRGBRed_green_blue_alpha(0.18,0.19,0.20,1.)}else{NSColor::colorWithSRGBRed_green_blue_alpha(0.98,0.98,0.97,1.)};
        decorate(&self.top,&chrome,0.);decorate(&self.footer,&chrome,12.);decorate(&self.panel,&chrome,14.);
        self.reader.set_spread(self.preferences.mode=="spread");self.reader.set_rtl(self.preferences.direction=="rtl");self.reader.set_theme(&self.preferences.theme);
        self.bookshelf.set_background(&bg);
        self.last_page=0;self.last_zoom=0;
    }
    unsafe fn change_language(&mut self,language:&str){
        match self.store.save_preferences(&json!({"language":language})) {
            Ok(preferences)=>self.preferences=preferences,
            Err(error)=>{self.toast(&error);return;}
        }
        set_language(&self.preferences.language);
        self.refresh_language();
        self.toast(text("表示言語を変更しました","Display language updated"));
    }
    unsafe fn refresh_language(&mut self){
        for (button,title) in [(&self.nav_button,text("本を追加","Add Books")),(&self.command_button,text("検索・操作","Search and Commands")),(&self.page_button,text("ページを選ぶ","Choose a Page")),(&self.zoom_button,text("全体表示に戻す","Fit Page"))] {
            button.setTitle(&s(title));button.setToolTip(Some(&s(title)));button.setAccessibilityLabel(Some(&s(title)));
            if button.image().is_some(){button.setImagePosition(NSCellImagePosition::ImageOnly);}
        }
        if let Ok(close)=self.panel.subviews().objectAtIndex(1).downcast::<NSButton>() {
            let title=text("閉じる · Esc","Close · Esc");close.setTitle(&s(title));close.setToolTip(Some(&s(title)));close.setAccessibilityLabel(Some(&s(title)));
            close.setImagePosition(NSCellImagePosition::ImageOnly);
        }
        self.empty_shelf.setStringValue(&s(text("PDFをドロップ","Drop PDFs Here")));
        if self.screen==Screen::Shelf{self.window.setTitle(&s(text("本棚 — Sumi","Library — Sumi")));}
        self.bookshelf.refresh_language();self.tab_strip.refresh_language();self.thumbnails.refresh_language();
        self.setup_menu();self.refresh_chrome();
        let (title,placeholder)=panel_labels(self.panel_kind);
        self.panel_title.setStringValue(&s(title));self.query.setPlaceholderString(Some(&s(placeholder)));
        let selected=self.selected;
        if self.panel_kind==Panel::ImportErrors {
            self.panel_hint.setStringValue(&s(text("追加できた本は本棚に保存されています · Esc 閉じる","Successfully added books are in your library · Esc to close")));
        } else if self.panel_kind==Panel::Pages {
            // Rebuilding these rows would reveal the reading page and lose the user's scroll position.
            self.panel_hint.setStringValue(&s(&page_total(self.reader.page_count())));
        } else if self.panel_kind!=Panel::None {
            self.populate_rows();self.selected=selected.min(self.rows.len().saturating_sub(1));
            if matches!(self.panel_kind,Panel::Find|Panel::Palette)&&!self.query.stringValue().to_string().trim().is_empty(){
                self.search.cancel();self.search_running=true;self.search_due=Some(Instant::now()+Duration::from_millis(100));
                self.panel_hint.setStringValue(&s(text("検索中… · Escで閉じる","Searching… · Esc to close")));
            }else if self.panel_kind!=Panel::Pages{
                self.panel_hint.setStringValue(&s(text("↑↓ 選択　↵ 開く　Esc 閉じる","↑↓ Select   ↵ Open   Esc Close")));
            }
        }
        self.layout();
    }
    unsafe fn tick(&mut self){
        let commands=COMMANDS.with(|q|q.borrow_mut().drain(..).collect::<Vec<_>>());
        for command in commands{match command{Command::Action(name,value)=>self.action(&name,value),Command::Files(paths)=>{if self.import_busy{self.toast(text("取り込み中です","Adding books…"));}else{self.pending_open=if paths.len()==1{paths.first().filter(|p|p.is_file()).cloned()}else{None};self.import(paths);}},Command::BookAction(action,id)=>self.book_action(action,&id),Command::Outside(point)=>self.outside_click(point)}}
        self.poll_open();
        self.reader.tick();
        let covers=self.screen==Screen::Shelf&&self.panel_kind==Panel::None&&self.opening.is_none();
        self.bookshelf.set_active(covers);self.thumbnails.set_active(self.panel_kind==Panel::Pages);
        if covers{self.bookshelf.render_next();}if self.panel_kind==Panel::Pages{self.thumbnails.render_next();}
        if self.search_due.is_some_and(|t|Instant::now()>=t){self.search_due=None;self.begin_search();}
        while let Some(result)=self.search.try_recv(){if result.request_id==self.request_id && matches!(self.panel_kind,Panel::Find|Panel::Palette){self.search_running=!result.complete;self.text_hits=result.hits;let count=self.text_hits.len();self.panel_hint.setStringValue(&s(&if result.complete{if result.warnings.is_empty(){format!("{} · {}",match_count(count),text("↑↓ 選択　↵ 移動　Esc 閉じる","↑↓ Select   ↵ Go   Esc Close"))}else{format!("{} · {}",match_count(count),result.warnings[0])}}else{format!("{} {}",text("検索中…","Searching…"),match_count(count))}));self.populate_rows();}}
        let result=self.import_receiver.as_ref().and_then(|rx|rx.try_recv().ok());if let Some(result)=result{self.import_receiver=None;self.import_busy=false;self.layout();match result{Ok(report)=>{let limit=if report.limited{text(" · 一部は上限に達したため未処理"," · Some files were skipped at the import limit")}else{""};let summary=if is_japanese(){format!("{}冊を追加 · 重複{}冊 · 読めなかったファイル{}件{limit}",report.added.len(),report.duplicates,report.errors.len())}else{format!("Added {} · Already in library {} · Could not import {}{limit}",report.added.len(),report.duplicates,report.errors.len())};self.toast(&summary);if !report.errors.is_empty(){self.show_import_errors(&report);}if self.screen==Screen::Shelf{self.refresh_shelf();}if let Some(path)=self.pending_open.take(){let canonical=std::fs::canonicalize(path).ok();let entry=self.store.snapshot().ok().and_then(|library|library.recent.into_iter().find(|e|Some(&e.path)==canonical.as_ref()));if let Some(entry)=entry{self.open_book(&entry.document.id,None);}}},Err(error)=>{self.pending_open=None;self.toast(&error)}}}
        let page=self.reader.current_page();let zoom=(self.reader.zoom_factor()*100.).round() as usize;
        if page!=self.last_page || zoom!=self.last_zoom {let changed=page!=self.last_page;self.last_page=page;self.last_zoom=zoom;self.refresh_chrome();if changed&&self.current.is_some(){self.dirty=Some(Instant::now()+Duration::from_millis(400));}}
        if self.dirty.is_some_and(|t|Instant::now()>=t)&&!self.reader.is_turning(){self.save();}
        if self.toast_until.is_some_and(|t|Instant::now()>=t){self.toast_until=None;self.status.setHidden(true);}
        self.update_activity();
        #[cfg(debug_assertions)] self.smoke_tick();
    }
    unsafe fn outside_click(&mut self,point:NSPoint){
        if self.panel_kind!=Panel::None&&contains(self.panel.frame(),point){return;}
        if self.screen==Screen::Shelf{let hit=self.root.hitTest(point);if self.bookshelf.clear_selection_outside(hit.as_deref())&&point.y<self.size.height-38.{self.window.makeFirstResponder(Some(self.bookshelf.view()));}}
        if self.panel_kind!=Panel::None&&point.y<self.size.height-38.&&(self.panel_kind!=Panel::Find||self.query.stringValue().to_string().trim().is_empty()){self.close_panel();}
    }
    #[cfg(debug_assertions)]
    unsafe fn smoke_tick(&mut self){
        let Some(mut smoke)=self.smoke.take()else{return;};if Instant::now()<smoke.due||self.reader.is_turning()||self.opening.is_some(){self.smoke=Some(smoke);return;}
        if smoke.book>=smoke.books.len(){let failures=smoke.checks.iter().filter(|v|v["passed"]!=true).count();let report=json!({"books":smoke.books.len(),"checks":smoke.checks,"failures":failures,"external_ui_queries_during_animation":false});let _=std::fs::create_dir_all(&smoke.directory);let _=std::fs::write(smoke.directory.join("reader-smoke.json"),serde_json::to_vec_pretty(&report).unwrap());self.toast(&if is_japanese(){format!("検証完了 · {}冊 · 失敗{}件",smoke.books.len(),failures)}else{format!("Verification complete · {} books · {failures} failures",smoke.books.len())});return;}
        if smoke.phase==0&&!self.current.as_ref().is_some_and(|book|book.id==smoke.books[smoke.book]){
            self.open_book(&smoke.books[smoke.book],None);smoke.due=Instant::now()+Duration::from_millis(100);self.smoke=Some(smoke);return;
        }
        if smoke.phase>0&&smoke.phase!=16{let details=self.reader.diagnostics();let zoom_ok=if matches!(smoke.phase,6|17){self.reader.zoom_factor()>1.1}else{(self.reader.zoom_factor()-1.).abs()<0.02};let passed=self.reader.current_page()==smoke.expected&&zoom_ok&&(matches!(smoke.phase,6|17)||details["fitted"]==true);smoke.checks.push(json!({"book":smoke.book+1,"phase":smoke.phase,"expected_page":smoke.expected,"passed":passed,"reader":details}));}
        let delay=match smoke.phase{
            0=>{self.reader.set_spread(false);smoke.expected=80.min(self.reader.page_count());self.reader.go_to(smoke.expected,false);250},
            1=>{self.reader.step(1);smoke.expected=(smoke.expected+1).min(self.reader.page_count());700},
            2=>{self.reader.step(-1);smoke.expected=smoke.expected.saturating_sub(1).max(1);700},
            3=>{self.reader.set_spread(true);150},
            4=>{self.reader.step(1);smoke.expected=(smoke.expected-smoke.expected%2+2).min(self.reader.page_count());700},
            5=>{self.reader.zoom(1);150},6=>{self.reader.fit();150},7=>{self.reader.set_spread(false);150},
            8=>{self.reader.debug_scroll(-30.,0.,1,false,1.);self.reader.debug_scroll(-200.,0.,4,false,1.12);self.reader.debug_scroll(0.,0.,8,false,1.14);smoke.expected+=1;700},
            9=>{self.reader.debug_scroll(-900.,0.,0,true,1.3);self.reader.debug_scroll(2.,-40.,1,false,2.);self.reader.debug_scroll(-250.,3.,4,false,2.1);self.reader.debug_scroll(0.,0.,8,false,2.12);200},
            10=>{self.reader.debug_scroll(-30.,0.,1,false,3.);self.reader.debug_scroll(-200.,0.,4,false,3.1);self.reader.debug_scroll(0.,0.,16,false,3.12);700},
            11=>{self.reader.debug_scroll(-30.,0.,1,false,4.);self.reader.debug_scroll(-170.,0.,4,false,4.1);self.reader.debug_scroll(190.,0.,4,false,4.2);self.reader.debug_scroll(0.,0.,8,false,4.22);700},
            12=>{self.reader.zoom(1);self.reader.debug_scroll(-600.,0.,1,false,5.);self.reader.debug_scroll(0.,0.,8,false,5.1);self.reader.fit();200},
            13=>{self.show_shelf();self.reader.step(1);self.reader.debug_scroll(-600.,0.,1,false,6.);self.reader.debug_scroll(0.,0.,8,false,6.1);self.show_reader();200},
            14=>{let before=self.tabs.len();self.show_shelf();self.action("escape",0);smoke.checks.push(json!({"book":smoke.book+1,"case":"escape_keeps_home","passed":self.screen==Screen::Shelf&&self.tabs.len()==before}));self.open_book(&smoke.books[smoke.book],None);smoke.checks.push(json!({"book":smoke.book+1,"case":"reopen_reuses_tab","passed":self.tabs.len()==before&&self.screen==Screen::Reader}));200},
            15=>{self.reader.zoom(1);self.last_find="sumi-tab-check".into();self.open_book(&smoke.books[(smoke.book+1)%smoke.books.len()],None);200},
            16=>{self.open_book(&smoke.books[smoke.book],None);200},
            17=>{smoke.checks.push(json!({"book":smoke.book+1,"case":"tab_restores_position_zoom_find","passed":self.reader.current_page()==smoke.expected&&(self.reader.zoom_factor()-1.25).abs()<0.02&&self.last_find=="sumi-tab-check"}));self.reader.fit();200},
            18=>{self.toggle(Panel::Pages);let count=self.reader.page_count();self.query.setStringValue(&s(&count.to_string()));self.action("query",0);self.thumbnails.render_next();smoke.checks.push(json!({"book":smoke.book+1,"case":"page_grid_reaches_last","passed":self.rows.len()==count&&self.selected==count-1&&self.thumbnails.index_for_page(count)==Some(count-1)&&self.thumbnails.realized_range().contains(&(count-1))&&self.thumbnails.visible_range().contains(&(count-1))&&self.thumbnails.cached_count()<=sumi_native::thumbnails::CACHE_CAPACITY}));self.close_panel();200},
            _=>{let id=smoke.books[smoke.book].clone();if let Some(index)=self.tabs.iter().position(|tab|tab.id==id){let count=self.tabs.len();self.close_tab(index);smoke.checks.push(json!({"book":smoke.book+1,"case":"close_tab_keeps_library","passed":self.tabs.len()+1==count&&!self.tabs.iter().any(|tab|tab.id==id)&&self.store.entry(&id).is_ok()}));}smoke.book+=1;smoke.phase=usize::MAX;100}
        };smoke.phase=smoke.phase.wrapping_add(1);smoke.due=Instant::now()+Duration::from_millis(delay);self.smoke=Some(smoke);
    }
    unsafe fn action(&mut self,name:&str,value:usize){
        if self.screen==Screen::Shelf&&matches!(name,"previous"|"next"|"fit"|"zoom-in"|"zoom-out"|"bookmark"|"capture"|"copy-selection"|"copy-page"|"focus"){return;}
        match name {
            "control"=>{if let Some(action)=CONTROLS.get(value){self.action(action,0);}},
            "layout"=>self.layout(),"find"=>{self.toggle(if self.screen==Screen::Shelf{Panel::Palette}else{Panel::Find});},"palette"=>self.toggle(Panel::Palette),"library"=>self.show_shelf(),"tools"=>self.toggle(Panel::Palette),"pages"=>self.toggle(Panel::Pages),"outline"=>self.toggle(Panel::Outline),"bookmarks"=>self.toggle(Panel::Bookmarks),
            "escape"=>{if self.panel_kind!=Panel::None{self.close_panel();}else if self.opening.is_some()||self.pending_open.is_some(){self.cancel_open();self.layout();}else if self.screen==Screen::Reader{if self.reader.cancel_interaction(){}else if self.reader.zoom_factor()>1.01{self.reader.fit();}else if self.focus{self.focus=false;self.layout();}}},
            "select-tab"=>self.select_tab(value),"close-tab"=>self.close_tab(value),
            "close-active-tab"=>{if let Some(index)=self.active_tab.as_ref().and_then(|id|self.tabs.iter().position(|tab|&tab.id==id)){self.close_tab(index);}},
            "number-tab"=>{let index=if value==9{self.tabs.len()}else{value.saturating_sub(1)};if index<=self.tabs.len(){self.select_tab(index);}},
            "cycle-tab"|"previous-tab"|"next-tab"=>{let index=self.active_tab.as_ref().and_then(|id|self.tabs.iter().position(|tab|&tab.id==id)).map_or(0,|i|i+1);let count=self.tabs.len()+1;let backwards=name=="previous-tab"||(name=="cycle-tab"&&value==1);self.select_tab(if backwards{(index+count-1)%count}else{(index+1)%count});},
            "shelf-open"=>{self.bookshelf.select(value);let id=self.bookshelf.id_at(value).map(str::to_owned);if let Some(id)=id{self.open_book(&id,None);}},
            "close"=>self.close_panel(),"query"=>{self.search_activity.begin();self.search_due=None;self.last_find_selected=None;self.selected=0;self.text_hits.clear();self.search.cancel();self.search_running=matches!(self.panel_kind,Panel::Find|Panel::Palette)&&!self.query.stringValue().to_string().trim().is_empty();self.populate_rows();self.layout();if matches!(self.panel_kind,Panel::Find|Panel::Palette)&&!self.query.stringValue().to_string().trim().is_empty(){self.search_due=Some(Instant::now()+Duration::from_millis(180));}},
            "row-down"=>{if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.move_selection(0,1);return;}self.last_find_selected=None;if !self.rows.is_empty(){self.selected=(self.selected+if self.panel_kind==Panel::Pages{COLUMNS}else{1}).min(self.rows.len()-1);self.highlight_row();}},"row-up"=>{if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.move_selection(0,-1);return;}self.last_find_selected=None;self.selected=self.selected.saturating_sub(if self.panel_kind==Panel::Pages{COLUMNS}else{1});self.highlight_row();},"row"=>{self.selected=value;self.last_find_selected=None;self.accept();},"accept"|"accept-prev"=>{if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.activate_selected();return;}if self.panel_kind==Panel::Find&&self.rows.iter().any(|r|matches!(r.action,RowAction::Hit(_))) {if name=="accept-prev"{self.selected=(self.selected+self.rows.len()-1)%self.rows.len();}else if self.last_find_selected==Some(self.selected){self.selected=(self.selected+1)%self.rows.len();}self.highlight_row();}self.accept();},
            "left"=>{if self.panel_kind==Panel::Pages{self.selected=self.selected.saturating_sub(1);self.highlight_row();return;}if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.move_selection(-1,0);return;}if self.panel_kind==Panel::None{self.reader.step(if self.preferences.direction=="rtl"{1}else{-1});}},"right"=>{if self.panel_kind==Panel::Pages{self.selected=(self.selected+1).min(self.rows.len().saturating_sub(1));self.highlight_row();return;}if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.move_selection(1,0);return;}if self.panel_kind==Panel::None{self.reader.step(if self.preferences.direction=="rtl"{-1}else{1});}},"previous"=>self.reader.step(-1),"next"=>self.reader.step(1),
            "space"=>{if self.screen==Screen::Shelf&&self.panel_kind==Panel::None{self.bookshelf.activate_selected();}},"fit"=>{self.reader.fit();},"zoom-in"=>{self.reader.zoom(1);},"zoom-out"=>{self.reader.zoom(-1);},
            "language-system"|"language-ja"|"language-en"=>{if let Some(language)=language_for_action(name){self.change_language(language);}},
            "spread"=>{self.preferences.mode=if self.preferences.mode=="spread"{"single"}else{"spread"}.into();self.persist_preferences();},
            "white"|"paper"|"night"=>{self.preferences.theme=name.into();self.persist_preferences();},"direction"=>{self.preferences.direction=if self.preferences.direction=="rtl"{"ltr"}else{"rtl"}.into();self.persist_preferences();},
            "focus"=>{self.close_panel();self.focus=!self.focus;self.layout();},"bookmark"=>self.bookmark(),
            "capture"=>{self.close_panel();self.reader.begin_capture();self.toast(text("範囲をドラッグして画像をコピー · Escで取消","Drag to copy an image · Esc to cancel"));},
            "copy-selection"=>{match self.reader.copy_selection(){Ok(_)=>self.toast(text("選んだ文章を出典付きでコピーしました","Selection copied with its source")),Err(e)=>self.toast(&e)}},
            "copy-page"=>{match self.reader.copy_page_text(){Ok(_)=>self.toast(text("ページの文章をコピーしました","Page text copied")),Err(e)=>self.toast(&e)}},
            "reader-page"=>{self.refresh_chrome();self.dirty=Some(Instant::now()+Duration::from_millis(400));},"reader-zoom"=>self.refresh_chrome(),
            "capture-ready"=>{self.last_capture=self.reader.take_capture();self.capture_name=format!("{}-p{}.png",self.current.as_ref().map(|b|b.title.as_str()).unwrap_or("Sumi"),self.reader.current_page());self.toast(text("画像をコピーしました · ⌘Kから保存もできます","Image copied · Use ⌘K to save it"));},
            "save-capture"=>self.save_capture(),"capture-failed"=>self.toast(text("画像をコピーできませんでした。もう一度範囲を選んでください","Could not copy the image. Select the area again.")),
            "reader-click"=>{if self.panel_kind==Panel::Find&&self.query.stringValue().to_string().trim().is_empty(){self.close_panel();}},
            "import"=>self.choose_import(),_=>{}
        }
    }
    unsafe fn persist_preferences(&mut self){if let Err(e)=self.store.save_preferences(&json!({"theme":self.preferences.theme,"mode":self.preferences.mode,"direction":self.preferences.direction,"language":self.preferences.language})){self.toast(&e);}self.apply_preferences();if self.panel_kind==Panel::Palette{self.populate_rows();}}
    fn save_before_close(&mut self)->bool{
        self.reader.set_interaction_enabled(false);
        let saved=self.save();
        if !saved{self.reader.set_interaction_enabled(self.screen==Screen::Reader&&self.panel_kind==Panel::None&&self.opening.is_none());}
        saved
    }
    fn save(&mut self)->bool{
        self.dirty=None;
        if self.panel_kind==Panel::Find{self.last_find=self.query.stringValue().to_string();}
        if let Some(tab)=self.current.as_ref().and_then(|book|self.tabs.iter_mut().find(|tab|tab.id==book.id)){
            tab.zoom=self.reader.zoom_factor();tab.find=self.last_find.clone();
        }
        if let Some(book)=self.current.as_mut(){book.state["page"]=json!(self.reader.current_page().max(1));}
        let result=self.store.save_reading_session(self.current.as_ref().map(|book|book.id.as_str()),self.current.as_ref().map(|book|&book.state),&self.session());
        if let Err(error)=result{
            self.dirty=Some(Instant::now()+Duration::from_secs(2));
            unsafe{self.toast(&format!("{}: {error}",text("読書状態を保存できません","Could not save your reading state")));}false
        }else{true}
    }
    unsafe fn refresh_chrome(&mut self){let page=self.reader.current_page();let count=self.reader.page_count();self.page_button.setTitle(&s(&if count>0{format!("{page} / {count}")}else{text("ページ","Pages").into()}));self.page_button.setAccessibilityLabel(Some(&s(&format!("{} · {page} / {count}",text("ページを選ぶ","Choose a Page")))));let zoom=self.reader.zoom_factor();self.zoom_button.setTitle(&s(&if zoom>1.01{format!("{}%",(zoom*100.).round() as usize)}else{text("全体表示","Fit Page").into()}));self.layout_footer();}
    unsafe fn toast(&mut self,text:&str){self.status.setStringValue(&s(text));self.status.setHidden(false);self.toast_until=Some(Instant::now()+Duration::from_secs(4));}
    unsafe fn toggle(&mut self,kind:Panel){if self.screen==Screen::Shelf&&matches!(kind,Panel::Pages|Panel::Outline|Panel::Bookmarks|Panel::Find){return;}if self.panel_kind==kind{self.close_panel();return;}if self.panel_kind==Panel::Find{self.last_find=self.query.stringValue().to_string();}self.search.cancel();self.search_due=None;self.search_running=false;self.panel_kind=kind;self.text_hits.clear();self.selected=0;self.last_find_selected=None;self.query.setStringValue(&s(if kind==Panel::Find{&self.last_find}else{""}));self.panel.setHidden(false);
        let (title,placeholder)=panel_labels(kind);
        self.panel_title.setStringValue(&s(title));self.query.setPlaceholderString(Some(&s(placeholder)));self.panel_hint.setStringValue(&s(""));self.populate_rows();self.layout();if kind==Panel::Pages{self.thumbnails.set_selected(self.selected,true);}self.window.makeFirstResponder(Some(&*self.query));self.query.selectText(None);if kind==Panel::Find&&!self.last_find.is_empty(){self.search_running=true;self.search_due=Some(Instant::now()+Duration::from_millis(100));}
    }
    unsafe fn close_panel(&mut self){if self.panel_kind==Panel::Find{self.last_find=self.query.stringValue().to_string();}self.panel_kind=Panel::None;self.panel.setHidden(true);self.search.cancel();self.search_due=None;self.search_running=false;self.reader.set_interaction_enabled(self.screen==Screen::Reader&&self.opening.is_none());if self.screen==Screen::Reader{self.reader.focus();}else{self.window.makeFirstResponder(Some(self.bookshelf.view()));}}
    unsafe fn populate_rows(&mut self){
        let query=self.query.stringValue().to_string();let query=query.trim().to_lowercase();let mut rows=vec![];
        if self.panel_kind==Panel::Palette {
            for command in palette_commands(&self.preferences) {
                if self.screen==Screen::Shelf&&!matches!(command.action,"import"|"white"|"paper"|"night"|"spread"|"direction"|"language-system"|"language-ja"|"language-en"){continue;}
                if command.matches(&query){rows.push(command.row());}
            }
        }
        if self.panel_kind==Panel::Palette{if let Ok(library)=self.store.public_library(){let mut books=vec![];for book in library.recent{if (self.screen==Screen::Shelf||!query.is_empty())&&crate::search::matches_query(&format!("{} {}",book.title,book.name),&query){let page=book.state["page"].as_u64().unwrap_or(1);if books.len()<40{books.push(Row{title:book.title.clone(),detail:page_label(page as usize),action:RowAction::Book(book.id)});}}}books.append(&mut rows);rows=books;}}
        if self.screen==Screen::Reader&&matches!(self.panel_kind,Panel::Palette|Panel::Pages){let number=page_query(&query,self.reader.page_count());
            if self.panel_kind==Panel::Pages{let anchor=number.unwrap_or(self.reader.current_page().max(1));for page in 1..=self.reader.page_count(){rows.push(Row{title:page_label(page as usize),detail:String::new(),action:RowAction::Page(page)});}self.selected=anchor.saturating_sub(1);self.panel_hint.setStringValue(&s(&page_total(self.reader.page_count())));
            }else if let Some(page)=number{rows.insert(0,Row{title:if is_japanese(){format!("{page}ページへ移動")}else{format!("Go to Page {page}")},detail:text("この本","This book").into(),action:RowAction::Page(page)});}}
        if self.screen==Screen::Reader&&matches!(self.panel_kind,Panel::Outline|Panel::Palette){for(title,page)in self.reader.outline(){if query.is_empty()&&self.panel_kind==Panel::Outline||!query.is_empty()&&crate::search::matches_query(&title,&query){rows.push(Row{title,detail:format!("{} · {}",text("目次","Contents"),page_label(page)),action:RowAction::Page(page)});}}}
        if self.panel_kind==Panel::Bookmarks{if let Some(book)=&self.current{if let Some(pages)=book.state["bookmarks"].as_array(){for value in pages{if let Some(page)=value.as_u64(){if query.is_empty()||page.to_string().contains(&query){rows.push(Row{title:page_label(page as usize),detail:text("しおり","Bookmarks").into(),action:RowAction::Page(page as usize)});}}}}}}
        if matches!(self.panel_kind,Panel::Find|Panel::Palette){for hit in &self.text_hits{rows.push(Row{title:format!("{} · {}",hit.title,page_label(hit.page)),detail:hit.snippet.clone(),action:RowAction::Hit(hit.clone())});}}
        if rows.is_empty(){rows.push(Row{title:if query.is_empty(){if self.panel_kind==Panel::Find{text("言葉を入力して本文を検索","Enter a word or phrase to search this book")}else{text("まだ項目がありません","No items yet")}}else if self.search_running{text("検索しています…","Searching…")}else{text("一致する項目がありません","No matches found")}.into(),detail:String::new(),action:RowAction::None});}
        if self.panel_kind!=Panel::Pages{rows.truncate(160);}self.rows=rows;self.selected=self.selected.min(self.rows.len().saturating_sub(1));self.render_rows();if self.panel_kind==Panel::Pages{self.thumbnails.set_selected(self.selected,true);}
    }
    unsafe fn render_rows(&mut self){
        for view in self.rows_view.subviews().iter(){if self.panel_kind!=Panel::Pages||!std::ptr::eq::<NSView>(&*view,self.thumbnails.view()){view.removeFromSuperview();}}self.row_buttons.clear();let w=self.rows_scroll.contentSize().width.max(100.);
        if self.panel_kind==Panel::Pages {let pages:Vec<usize>=self.rows.iter().filter_map(|r|if let RowAction::Page(p)=r.action{Some(p)}else{None}).collect();let source=self.current.as_ref().and_then(|book|self.store.entry(&book.id).ok()).map(|entry|PreviewSource{path:entry.path,password:self.password.clone(),revision:(entry.document.size,entry.document.modified_at.to_bits())});self.thumbnails.show(source,&pages,self.reader.current_page(),w);let h=self.thumbnails.height().max(self.rows_scroll.contentSize().height);self.rows_view.setFrame(rect(0.,0.,w,h));self.thumbnails.view().setFrameOrigin(NSPoint::new(0.,h-self.thumbnails.height()));if self.thumbnails.view().superview().is_none(){self.rows_view.addSubview(self.thumbnails.view());}self.thumbnails.set_selected(self.selected,false);return;}
        let height=(self.rows.len() as f64*52.).max(self.rows_scroll.contentSize().height);self.rows_view.setFrame(rect(0.,0.,w,height));
        for(i,row)in self.rows.iter().enumerate(){let b=NSButton::buttonWithTitle_target_action(&s(&format!("{}\n{}",row.title,row.detail)),Some(&*self.delegate),Some(sel!(activateRow:)),self.mtm);b.setTag(i as isize);b.setBordered(false);b.setAlignment(NSTextAlignment::Left);b.setFont(Some(&NSFont::systemFontOfSize(13.)));let _:()=msg_send![&b,setLineBreakMode:NSLineBreakMode::ByTruncatingTail];b.setFrame(rect(4.,height-(i+1) as f64*52.,w-8.,50.));b.setAccessibilityLabel(Some(&s(&format!("{} {}",row.title,row.detail))));if let RowAction::Book(id)=&row.action {let menu=NSMenu::new(self.mtm);for (tag,name) in [text("Finderで表示","Show in Finder"),text("場所を指定し直す…","Locate File…"),text("本棚から外す…","Remove from Library…")].iter().enumerate(){let item=NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm),&s(name),Some(sel!(bookAction:)),&s(""));item.setTag(tag as isize);item.setTarget(Some(&*self.delegate));item.setRepresentedObject(Some(&*s(id)));menu.addItem(&item);}b.setMenu(Some(&menu));}self.rows_view.addSubview(&b);self.row_buttons.push(b);}
        self.highlight_row();
    }
    unsafe fn highlight_row(&mut self){if self.panel_kind==Panel::Pages{self.thumbnails.set_selected(self.selected,true);return;}for(i,b)in self.row_buttons.iter().enumerate(){let color=if i==self.selected{NSColor::controlAccentColor().colorWithAlphaComponent(0.13)}else{NSColor::clearColor()};decorate(b,&color,7.);let tint=NSColor::labelColor();b.setContentTintColor(Some(&tint));if self.panel_kind!=Panel::Pages{if let Some(row)=self.rows.get(i){b.setAttributedTitle(&row_title(row,self.mtm));}}}if let Some(b)=self.row_buttons.get(self.selected){let frame=b.convertRect_toView(b.bounds(),Some(&self.rows_view));self.rows_view.scrollRectToVisible(frame);}}
    unsafe fn accept(&mut self){let action=self.rows.get(self.selected).map(|r|r.action.clone());match action{Some(RowAction::Command(action))=>{self.close_panel();self.action(action,0);},Some(RowAction::Book(id))=>{self.close_panel();self.open_book(&id,None);},Some(RowAction::Page(page))=>{self.close_panel();self.reader.go_to(page,false);},Some(RowAction::Hit(hit))=>{let query=self.query.stringValue().to_string();let keep=self.panel_kind==Panel::Find;if !keep{self.close_panel();}if self.current.as_ref().is_some_and(|b|b.id==hit.book_id){self.cancel_open();self.reader.go_to(hit.page,false);if self.screen==Screen::Shelf{self.show_reader();}}else{self.open_book(&hit.book_id,Some(hit.page));}if let Some(task)=self.opening.as_mut(){task.highlight=Some(query);}else{self.reader.highlight(&query);}if keep{self.last_find_selected=Some(self.selected);}},_=>{}}}
    unsafe fn begin_search(&mut self){
        let query=self.query.stringValue().to_string();if query.trim().is_empty(){return;}
        let mut books=vec![];
        if let Ok(library)=self.store.snapshot(){for entry in library.recent{
            let book=entry.document;
            if self.panel_kind==Panel::Find&&!self.current.as_ref().is_some_and(|b|b.id==book.id){continue;}
            let password=self.tabs.iter().find(|tab|tab.id==book.id).and_then(|tab|tab.password.clone());
            books.push(SearchBook{id:book.id,title:book.title,path:entry.path,password});
        }}
        self.request_id=self.search.submit(SearchRequest{query,books,limit:100});
        self.panel_hint.setStringValue(&s(text("検索中… · Escで閉じる","Searching… · Esc to close")));
    }
    unsafe fn cancel_open(&mut self){
        self.pending_open=None;self.opening=None;self.open_activity.finish();
        if self.current.is_none(){self.active_tab=None;}
        self.reader.set_interaction_enabled(self.screen==Screen::Reader&&self.panel_kind==Panel::None);
    }
    unsafe fn open_book(&mut self,id:&str,page:Option<usize>){
        self.pending_open=None;self.close_panel();
        if self.current.as_ref().is_some_and(|book|book.id==id){
            self.cancel_open();if let Some(page)=page{self.reader.go_to(page,false);}self.show_reader();return;
        }
        self.reader.set_interaction_enabled(false);
        if !self.save(){self.reader.set_interaction_enabled(self.screen==Screen::Reader);return;}
        self.open_sequence=self.open_sequence.wrapping_add(1);
        self.opening=Some(OpenTask{sequence:self.open_sequence,id:id.into(),page,password:self.tabs.iter().find(|tab|tab.id==id).and_then(|tab|tab.password.clone()),highlight:None});
        self.open_activity.begin();self.start_open();
    }
    fn start_open(&mut self){
        if self.open_receiver.is_some(){return;}
        let Some(task)=self.opening.clone()else{return;};
        let store=self.store.clone();let(tx,rx)=mpsc::channel();self.open_receiver=Some((task.sequence,rx));
        std::thread::spawn(move||{
            #[cfg(debug_assertions)]
            if store.path.components().any(|part|part.as_os_str()=="test-artifacts"){
                if let Some(delay)=std::env::var("SUMI_NATIVE_OPEN_DELAY_MS").ok().and_then(|value|value.parse::<u64>().ok()){
                    std::thread::sleep(Duration::from_millis(delay.min(10_000)));
                }
            }
            let result=(||{
                let entry=store.entry(&task.id)?;let path=store.authorized_path(&task.id)?;
                let page=task.page.unwrap_or_else(||entry.document.state["page"].as_u64().unwrap_or(1)as usize);
                let document=PreparedDocument::open_at(&path,task.password.as_deref(),page)?;
                let title=if entry.document.title.is_empty()||entry.document.title==entry.document.name{
                    crate::search::derive_title(&path).or_else(||Some(document.info().title.clone()))
                }else{None};
                Ok(OpenedBook{document,title})
            })();let _=tx.send(result);
        });
    }
    unsafe fn poll_open(&mut self){
        let completed=self.open_receiver.as_ref().and_then(|(sequence,rx)|match rx.try_recv(){
            Ok(result)=>Some((*sequence,result)),
            Err(mpsc::TryRecvError::Disconnected)=>Some((*sequence,Err(text("本の読み込みが中断されました","Book loading was interrupted").into()))),
            Err(mpsc::TryRecvError::Empty)=>None,
        });
        let Some((sequence,result))=completed else{return;};self.open_receiver=None;
        if !self.opening.as_ref().is_some_and(|task|task.sequence==sequence){self.start_open();return;}
        let task=self.opening.take().unwrap();self.open_activity.finish();
        match result{
            Ok(loaded)=>{
                if !self.tabs.iter().any(|tab|tab.id==task.id)&&self.store.entry(&task.id).is_err(){self.layout();return;}
                if let Some(title)=loaded.title{let _=self.store.set_book_title(&task.id,&title);}
                let Ok(entry)=self.store.entry(&task.id)else{self.layout();return;};
                let saved=self.tabs.iter().find(|tab|tab.id==task.id).cloned();let book=entry.document;
                self.reader.attach(loaded.document);self.reader.set_title(&book.title);self.thumbnails.clear();
                self.current=Some(book.clone());self.password=task.password.clone();
                self.last_find=saved.as_ref().map_or(String::new(),|tab|tab.find.clone());
                if let Some(tab)=self.tabs.iter_mut().find(|tab|tab.id==task.id){tab.title=book.title.clone();tab.password=task.password;}
                else{self.tabs.push(OpenTab{id:task.id.clone(),title:book.title.clone(),password:task.password,zoom:1.,find:String::new()});}
                self.show_reader();self.reader.restore_zoom(saved.as_ref().map_or(1.,|tab|tab.zoom));
                if let Some(query)=task.highlight{self.reader.highlight(&query);}
                self.status.setHidden(true);self.refresh_chrome();
                if let Err(error)=self.store.open(&task.id){self.toast(&format!("{}: {error}",text("本は開けましたが、履歴を保存できません","The book opened, but its history could not be saved")));}
                self.save();
            },
            Err(error) if error=="PASSWORD_REQUIRED"||error=="PASSWORD_INCORRECT"=>{
                let alert=NSAlert::new(self.mtm);alert.setMessageText(&s(text("このPDFのパスワードを入力","Enter the PDF Password")));
                alert.setInformativeText(&s(if error=="PASSWORD_INCORRECT"{text("パスワードが違います。もう一度入力してください。","Incorrect password. Try again.")}else{text("パスワードはこの起動中だけ使い、保存しません。","The password is kept only while Sumi is open and is not saved.")}));
                let field=NSSecureTextField::initWithFrame(NSSecureTextField::alloc(self.mtm),rect(0.,0.,300.,28.));alert.setAccessoryView(Some(&field));alert.addButtonWithTitle(&s(text("開く","Open")));alert.addButtonWithTitle(&s(text("キャンセル","Cancel")));alert.window().setInitialFirstResponder(Some(&field));
                if alert.runModal()==1000{self.opening=Some(OpenTask{password:Some(field.stringValue().to_string()),..task});self.open_activity.begin();self.start_open();}
                else{self.cancel_open();self.layout();self.persist_session();}
            },
            Err(error)=>{if self.current.is_none(){self.active_tab=None;}self.toast(&error);self.layout();self.reader.focus();self.persist_session();},
        }
    }
    unsafe fn book_action(&mut self,action:usize,id:&str){
        match action {
            0=>{if let Ok(path)=self.store.authorized_path(id){let url=objc2_foundation::NSURL::fileURLWithPath(&s(&path.to_string_lossy()));NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));}},
            1=>{let panel=NSOpenPanel::openPanel(self.mtm);panel.setTitle(Some(&s(text("移動したPDFを選ぶ","Locate the PDF"))));panel.setAllowsMultipleSelection(false);panel.setCanChooseDirectories(false);#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("pdf")])));if panel.runModal()==1{if let Some(path)=panel.URL().and_then(|url|url.path()){match self.store.relink(id,&PathBuf::from(path.to_string())){Ok(_)=>{self.toast(text("本の場所を更新しました","File location updated"));self.refresh_shelf();self.populate_rows();},Err(e)=>self.toast(&e)}}}},
            2=>{let alert=NSAlert::new(self.mtm);alert.setMessageText(&s(text("この本を本棚から外しますか？","Remove this book from the library?")));alert.setInformativeText(&s(text("PDFファイルは残ります。読書位置としおりは本棚から削除されます。","The PDF file will stay in place. Its reading position and bookmarks will be removed from the library.")));alert.addButtonWithTitle(&s(text("キャンセル","Cancel")));alert.addButtonWithTitle(&s(text("本棚から外す","Remove from Library")));if alert.runModal()==1001{if self.screen==Screen::Reader&&self.current.as_ref().is_some_and(|b|b.id==id){self.toast(text("読んでいる本は、別の本を開いてから外せます","Open another book before removing this one."));}else{match self.store.remove(id){Ok(_)=>{if self.current.as_ref().is_some_and(|b|b.id==id){self.reader.clear();self.current=None;self.password=None;}self.tabs.retain(|tab|tab.id!=id);if self.active_tab.as_deref()==Some(id){self.active_tab=None;}self.persist_session();self.refresh_shelf();self.layout();self.populate_rows();self.toast(text("本棚から外しました","Book removed from the library"));},Err(e)=>self.toast(&e)}}}},_=>{}
        }
    }
    unsafe fn show_import_errors(&mut self,report:&ImportReport){
        if self.panel_kind!=Panel::ImportErrors{self.toggle(Panel::ImportErrors);}self.panel_title.setStringValue(&s(text("取り込み結果","Import Results")));self.rows=report.errors.iter().map(|e|Row{title:e.path.file_name().unwrap_or_default().to_string_lossy().into_owned(),detail:e.message.clone(),action:RowAction::None}).collect();self.panel_hint.setStringValue(&s(text("追加できた本は本棚に保存されています · Esc 閉じる","Successfully added books are in your library · Esc to close")));self.render_rows();
    }
    unsafe fn bookmark(&mut self){if let Some(book)=self.current.as_mut(){let page=self.reader.current_page().max(1);let mut marks=book.state["bookmarks"].as_array().cloned().unwrap_or_default();let exists=marks.iter().any(|v|v.as_u64()==Some(page as u64));marks.retain(|v|v.as_u64()!=Some(page as u64));if !exists{marks.push(json!(page));}marks.sort_by_key(|v|v.as_u64().unwrap_or(0));book.state["bookmarks"]=json!(marks);self.save();self.toast(if exists{text("しおりを外しました","Bookmark removed")}else{text("しおりを付けました","Bookmark added")});}}
    unsafe fn save_capture(&mut self){
        let Some(image)=self.last_capture.as_ref()else{self.toast(text("先に⌘⇧Xで範囲を選んでください","Select an area with ⌘⇧X first."));return;};
        let Some(tiff)=image.TIFFRepresentation()else{self.toast(text("画像を読み込めませんでした","Could not read the image."));return;};let class=objc2::runtime::AnyClass::get(c"NSBitmapImageRep").unwrap();let rep:Option<Retained<AnyObject>>=msg_send![class,imageRepWithData:&*tiff];let Some(rep)=rep else{self.toast(text("画像を読み込めませんでした","Could not read the image."));return;};
        let properties=objc2_foundation::NSDictionary::<NSString,AnyObject>::new();let png:Option<Retained<objc2_foundation::NSData>>=msg_send![&rep,representationUsingType:NSBitmapImageFileType::PNG,properties:&*properties];let Some(png)=png else{self.toast(text("PNGを作成できませんでした","Could not create the PNG."));return;};
        let panel=NSSavePanel::savePanel(self.mtm);panel.setTitle(Some(&s(text("切り抜いた画像を保存","Save Captured Image"))));panel.setNameFieldStringValue(&s(&self.capture_name.replace('/',"-")));#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("png")])));if panel.runModal()==1{if let Some(url)=panel.URL(){let ok:bool=msg_send![&png,writeToURL:&*url,atomically:true];self.toast(if ok{text("画像を保存しました","Image saved")}else{text("画像を保存できませんでした","Could not save the image.")});}}
    }
    unsafe fn choose_import(&mut self){if self.import_busy{self.toast(text("本を取り込み中です","Adding books…"));return;}let panel=NSOpenPanel::openPanel(self.mtm);panel.setCanChooseFiles(true);panel.setCanChooseDirectories(true);panel.setAllowsMultipleSelection(true);panel.setTitle(Some(&s(text("本を取り込む","Add Books"))));panel.setMessage(Some(&s(text("PDFまたはフォルダを選んでください。元のファイルは移動しません。","Choose PDFs or folders. The original files stay in place."))));panel.setPrompt(Some(&s(text("取り込む","Add"))));#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("pdf")])));let result=panel.runModal();if result==1{let paths=panel.URLs().iter().filter_map(|u|u.path().map(|p|PathBuf::from(p.to_string()))).collect();self.import(paths);}}
    unsafe fn import(&mut self,paths:Vec<PathBuf>){if paths.is_empty(){return;}if self.import_busy{self.toast(text("本を取り込み中です。完了してから追加してください","Books are being added. Wait for this import to finish."));return;}self.import_busy=true;self.import_activity.begin();self.layout();self.toast(text("本を取り込んでいます…","Adding books…"));let store=self.store.clone();let(tx,rx)=mpsc::channel();self.import_receiver=Some(rx);std::thread::spawn(move||{let result=store.import_paths(&paths).map(|report|{for entry in &report.added{if let Some(title)=crate::search::derive_title(&entry.path){let _=store.set_book_title(&entry.document.id,&title);}}report});let _=tx.send(result);});}
}

pub fn run(){let mtm=MainThreadMarker::new().expect("Sumi requires the macOS main thread");let app=NSApplication::sharedApplication(mtm);let delegate=Delegate::new(mtm);app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));app.run();}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn commands_remain_searchable_in_both_languages_in_either_layout() {
        for mode in ["single","spread"] {
            let mut preferences=ReaderPreferences::default();preferences.mode=mode.into();
            let commands=palette_commands(&preferences);
            for (query,action) in [("しおりを開く","bookmarks"),("open bookmarks","bookmarks"),("見開き","spread"),("two pages","spread"),("ＺＯＯＭ ＩＮ","zoom-in"),("save image","save-capture"),("英語","language-en"),("Japanese","language-ja"),("システム","language-system")] {
                assert!(commands.iter().any(|command|command.action==action&&command.matches(query)),"{mode}: {query}");
            }
            assert!(!commands.iter().any(|command|command.matches("not-a-command-937")));
        }
    }

    #[test]
    fn language_commands_and_menu_actions_use_the_saved_preference_values() {
        let commands=palette_commands(&ReaderPreferences::default());
        for (action,language) in [("language-system","system"),("language-ja","ja"),("language-en","en")] {
            assert_eq!(language_for_action(action),Some(language));
            assert!(CONTROLS.contains(&action));
            assert_eq!(commands.iter().filter(|command|command.action==action).count(),1);
        }
        assert_eq!(language_for_action("language-fr"),None);
        assert_eq!(language_for_action("paper"),None);
    }

    #[test]
    fn page_queries_accept_english_japanese_and_width_variants_with_bounds() {
        for query in ["42","42ページ","page 42","Page 42","p42","p. 42","Ｐ．４２","４２ページ"] {
            assert_eq!(page_query(query,80),Some(42),"{query}");
        }
        for query in ["0","81","page -1","paper 42","42.5","42 pages later",""] {
            assert_eq!(page_query(query,80),None,"{query}");
        }
        assert_eq!(page_query("1",0),None);
    }
}
