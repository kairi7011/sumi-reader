use crate::{Command, COMMANDS, enqueue, reader::Reader, search::{SearchEngine, SearchBook, SearchRequest, SearchHit}};
use objc2::{define_class, msg_send, sel, AnyThread, ClassType, MainThreadOnly, rc::Retained, runtime::{AnyObject, ProtocolObject, Sel}};
use objc2_app_kit::*;
use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSNotification, NSString, NSArray, NSPoint, NSRect, NSSize, NSTimer, NSMutableAttributedString, NSRange};
use serde_json::json;
use std::{cell::RefCell, path::PathBuf, sync::{Arc, mpsc}, time::{Duration, Instant}};
use sumi_native::library::{Store, Document, ReaderPreferences, ImportReport};
use sumi_native::thumbnails::{ThumbnailGrid,pages_near,COLUMNS};

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
        #[unsafe(method(applicationWillTerminate:))]
        fn terminating(&self,_note:&NSNotification) { UI.with(|state| {if let Some(ui)=state.borrow_mut().as_mut(){ui.save();}}); }
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowDidResize:))]
        fn resized(&self,_note:&NSNotification){ enqueue("layout",0); }
    }
    unsafe impl NSTextFieldDelegate for Delegate {}
    unsafe impl NSControlTextEditingDelegate for Delegate {
        #[unsafe(method(controlTextDidChange:))]
        fn changed(&self,_note:&NSNotification){enqueue("query",0);}
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn text_command(&self,_control:&NSControl,_view:&NSTextView,command:Sel)->bool {
            if command==sel!(cancelOperation:) {enqueue("escape",0);true}
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
        #[unsafe(method(activateControl:))]
        fn action(&self,sender:&AnyObject) {let tag:isize=unsafe{msg_send![sender,tag]}; enqueue("control",tag as usize);}
        #[unsafe(method(activateRow:))]
        fn row(&self,sender:&AnyObject) {let tag:isize=unsafe{msg_send![sender,tag]};enqueue("row",tag as usize);}
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
                    if mods.contains(NSEventModifierFlags::Command) {
                        let command=match key.as_str(){"f" if mods.contains(NSEventModifierFlags::Shift)=>Some("focus"),"f"=>Some("find"),"k"=>Some("palette"),"o"=>Some("import"),"g"=>Some("pages"),"0"=>Some("fit"),"+"|"="=>Some("zoom-in"),"-"=>Some("zoom-out"),"d"=>Some("bookmark"),"x" if mods.contains(NSEventModifierFlags::Shift)=>Some("capture"),_=>None};
                        if let Some(command)=command {enqueue(command,0);return;}
                    }
                    if event.keyCode()==53 {enqueue("escape",0);return;}
                    let editing=self.firstResponder().is_some_and(|r|r.isKindOfClass(NSTextView::class()));
                    if !editing && !mods.contains(NSEventModifierFlags::Command) && !mods.contains(NSEventModifierFlags::Option) {
                        match event.keyCode(){123=>{enqueue("left",0);return;},124=>{enqueue("right",0);return;},_=>{}}
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
                if let Some(names)=names {
                    let paths=names.iter().map(|p|PathBuf::from(p.to_string())).collect();
                    COMMANDS.with(|q|q.borrow_mut().push_back(Command::Files(paths)));true
                }else{false}
            }
        }
    }
);

#[derive(Clone,Copy,PartialEq,Eq)]
enum Panel {None,Find,Palette,Library,Pages,Tools,Outline,Bookmarks}
#[derive(Clone)]
enum RowAction {Command(&'static str),Book(String),Page(usize),Hit(SearchHit),None}
#[derive(Clone)]
struct Row {title:String,detail:String,action:RowAction}

struct Ui {
    mtm:MainThreadMarker, delegate:Retained<Delegate>, window:Retained<ReaderWindow>, root:Retained<RootView>, reader:Reader,
    store:Arc<Store>, preferences:ReaderPreferences, current:Option<Document>, password:Option<String>,
    title:Retained<NSTextField>, status:Retained<NSTextField>, page_button:Retained<NSButton>, zoom_button:Retained<NSButton>, spread_button:Retained<NSButton>,
    top:Retained<NSView>, footer:Retained<NSView>, panel:Retained<NSView>, query:Retained<NSTextField>, panel_title:Retained<NSTextField>, panel_hint:Retained<NSTextField>, rows_scroll:Retained<NSScrollView>, rows_view:Retained<NSView>,
    thumbnails:ThumbnailGrid,page_anchor:usize,page_controls:Retained<NSView>,
    panel_kind:Panel, rows:Vec<Row>, row_buttons:Vec<Retained<NSButton>>, selected:usize, search:SearchEngine, request_id:u64, search_due:Option<Instant>, search_running:bool, text_hits:Vec<SearchHit>,
    dirty:Option<Instant>, last_page:usize,last_zoom:usize,size:NSSize,focus:bool,toast_until:Option<Instant>,
    last_capture:Option<Retained<NSImage>>,capture_name:String,last_find:String,last_find_selected:Option<usize>,
    pending_open:Option<PathBuf>,
    import_receiver:Option<mpsc::Receiver<Result<ImportReport,String>>>, import_busy:bool,
}

unsafe fn decorate(view:&NSView,color:&NSColor,radius:f64){view.setWantsLayer(true); let layer:Option<Retained<AnyObject>>=msg_send![view,layer]; if let Some(layer)=layer{let _:()=msg_send![&layer,setCornerRadius:radius];let cg:*const objc2_core_graphics::CGColor=msg_send![color,CGColor];let _:()=msg_send![&layer,setBackgroundColor:cg];}}
unsafe fn label(mtm:MainThreadMarker,text:&str,size:f64)->Retained<NSTextField>{let v=NSTextField::labelWithString(&s(text),mtm);v.setFont(Some(&NSFont::systemFontOfSize(size)));v.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingTail);v}
unsafe fn button(delegate:&Delegate,text:&str,symbol:Option<&str>,tag:usize)->Retained<NSButton>{
    let b=NSButton::buttonWithTitle_target_action(&s(text),Some(delegate),Some(sel!(activateControl:)),delegate.mtm());
    b.setTag(tag as isize);b.setBezelStyle(NSBezelStyle::AccessoryBarAction);b.setBordered(false);b.setFont(Some(&NSFont::systemFontOfSize(12.)));
    if let Some(symbol)=symbol {if let Some(image)=NSImage::imageWithSystemSymbolName_accessibilityDescription(&s(symbol),Some(&s(text))){b.setImage(Some(&image));b.setImagePosition(NSCellImagePosition::ImageOnly);}}
    b.setToolTip(Some(&s(text)));b.setAccessibilityLabel(Some(&s(text))); b
}
unsafe fn row_title(row:&Row,_mtm:MainThreadMarker)->Retained<NSMutableAttributedString>{
    let text=format!("{}\n{}",row.title,row.detail);let value=NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(),&s(&text));let full=NSRange::new(0,text.encode_utf16().count());let detail=NSRange::new(row.title.encode_utf16().count()+1,row.detail.encode_utf16().count());
    value.addAttribute_value_range(&s("NSFont"),&NSFont::systemFontOfSize(13.),full);value.addAttribute_value_range(&s("NSColor"),&NSColor::labelColor(),full);value.addAttribute_value_range(&s("NSFont"),&NSFont::systemFontOfSize(11.),detail);value.addAttribute_value_range(&s("NSColor"),&NSColor::secondaryLabelColor(),detail);
    let paragraph=NSMutableParagraphStyle::new();paragraph.setFirstLineHeadIndent(12.);paragraph.setHeadIndent(12.);paragraph.setTailIndent(-12.);paragraph.setLineSpacing(3.);paragraph.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);value.addAttribute_value_range(&s("NSParagraphStyle"),&paragraph,full);value
}
const CONTROLS:[&str;24]=["library","find","palette","tools","previous","next","pages","spread","zoom-in","fit","zoom-out","close","import","outline","bookmarks","bookmark","capture","white","paper","night","direction","focus","page-back","page-forward"];

impl Ui {
    unsafe fn new(delegate:&Delegate)->Result<Self,String>{
        let mtm=delegate.mtm();let store=Arc::new(Store::load_default()?);let library=store.public_library()?;let preferences=library.preferences;
        let window:Retained<ReaderWindow>=msg_send![ReaderWindow::alloc(mtm),initWithContentRect:rect(0.,0.,1160.,800.),styleMask:(NSWindowStyleMask::Titled|NSWindowStyleMask::Closable|NSWindowStyleMask::Miniaturizable|NSWindowStyleMask::Resizable|NSWindowStyleMask::FullSizeContentView),backing:NSBackingStoreType::Buffered,defer:false];
        window.setReleasedWhenClosed(false);window.setTitle(&s("Sumi"));window.setTitleVisibility(NSWindowTitleVisibility::Hidden);window.setTitlebarAppearsTransparent(true);window.setContentMinSize(NSSize::new(680.,480.));window.setDelegate(Some(ProtocolObject::from_ref(delegate)));
        let root:Retained<RootView>=msg_send![RootView::alloc(mtm),initWithFrame:rect(0.,0.,1160.,800.)];
        root.registerForDraggedTypes(&NSArray::from_slice(&[&*s("NSFilenamesPboardType"),&*s("public.file-url")]));
        window.setContentView(Some(&root));
        let reader=Reader::new(mtm,rect(0.,44.,1160.,714.)); root.addSubview(reader.view());
        let top=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,762.,1160.,38.));root.addSubview(&top);
        let title=label(mtm,"Sumi",12.);title.setAlignment(NSTextAlignment::Center);top.addSubview(&title);
        for (tag,x) in [(0,88.),(1,1020.),(2,1056.),(3,1092.)]{let b=button(delegate,match tag {0=>"本棚",1=>"本文を検索 · ⌘F",2=>"すべて検索 · ⌘K",_=>"表示と操作"},Some(match tag{0=>"sidebar.left",1=>"magnifyingglass",2=>"command",_=>"slider.horizontal.3"}),tag);b.setFrame(rect(x,4.,32.,30.));top.addSubview(&b);}
        let footer=NSView::initWithFrame(NSView::alloc(mtm),rect(400.,8.,360.,34.));root.addSubview(&footer);
        let prev=button(delegate,"前のページ",Some("chevron.left"),4);prev.setFrame(rect(8.,3.,30.,28.));footer.addSubview(&prev);
        let page_button=button(delegate,"ページを選ぶ",None,6);page_button.setFrame(rect(40.,3.,144.,28.));footer.addSubview(&page_button);
        let next=button(delegate,"次のページ",Some("chevron.right"),5);next.setFrame(rect(184.,3.,30.,28.));footer.addSubview(&next);
        let spread_button=button(delegate,"見開きにする",Some("book"),7);spread_button.setFrame(rect(224.,3.,30.,28.));footer.addSubview(&spread_button);
        let zoom_button=button(delegate,"全体表示",None,9);zoom_button.setFrame(rect(265.,3.,76.,28.));footer.addSubview(&zoom_button);
        let status=label(mtm,"PDFやフォルダをドロップ、または⌘Oで本を取り込む",12.);status.setAlignment(NSTextAlignment::Center);root.addSubview(&status);
        let panel=NSView::initWithFrame(NSView::alloc(mtm),rect(300.,300.,560.,390.));root.addSubview(&panel);panel.setHidden(true);
        let panel_title=label(mtm,"すべて検索",13.);panel.addSubview(&panel_title);
        let close=button(delegate,"閉じる · Esc",Some("xmark"),11);panel.addSubview(&close);
        let query=NSTextField::initWithFrame(NSTextField::alloc(mtm),rect(16.,330.,528.,30.));query.setFont(Some(&NSFont::systemFontOfSize(16.)));query.setBezeled(false);query.setDrawsBackground(false);query.setDelegate(Some(ProtocolObject::from_ref(delegate)));query.setFocusRingType(NSFocusRingType::None);panel.addSubview(&query);
        let rows_scroll=NSScrollView::initWithFrame(NSScrollView::alloc(mtm),rect(8.,36.,544.,270.));rows_scroll.setDrawsBackground(false);rows_scroll.setHasVerticalScroller(true);rows_scroll.setAutohidesScrollers(true);panel.addSubview(&rows_scroll);
        let rows_view=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,0.,528.,270.));rows_scroll.setDocumentView(Some(&rows_view));
        let panel_hint=label(mtm,"↑↓ 選択　↵ 開く　Esc 閉じる",11.);panel.addSubview(&panel_hint);
        let page_controls=NSView::initWithFrame(NSView::alloc(mtm),rect(0.,0.,100.,28.));panel.addSubview(&page_controls);page_controls.setHidden(true);
        let earlier=button(delegate,"前の24ページ",Some("chevron.left"),22);earlier.setFrame(rect(0.,0.,32.,26.));page_controls.addSubview(&earlier);
        let later=button(delegate,"次の24ページ",Some("chevron.right"),23);later.setFrame(rect(40.,0.,32.,26.));page_controls.addSubview(&later);
        let thumbnails=ThumbnailGrid::new(mtm,delegate);
        let mut ui=Self{mtm,delegate:Retained::retain(delegate as *const Delegate as *mut Delegate).unwrap(),window,root,reader,store,preferences,current:None,password:None,title,status,page_button,zoom_button,spread_button,top,footer,panel,query,panel_title,panel_hint,rows_scroll,rows_view,thumbnails,page_anchor:1,page_controls,panel_kind:Panel::None,rows:vec![],row_buttons:vec![],selected:0,search:SearchEngine::new(),request_id:0,search_due:None,search_running:false,text_hits:vec![],dirty:None,last_page:0,last_zoom:0,size:NSSize::new(0.,0.),focus:false,toast_until:None,last_capture:None,capture_name:String::new(),last_find:String::new(),last_find_selected:None,pending_open:None,import_receiver:None,import_busy:false};
        ui.apply_preferences();ui.layout();ui.window.center();ui.window.makeKeyAndOrderFront(None);
        let app=NSApplication::sharedApplication(mtm);app.setActivationPolicy(NSApplicationActivationPolicy::Regular);#[allow(deprecated)]app.activateIgnoringOtherApps(true);
        let _:Retained<NSTimer>=msg_send![NSTimer::class(),scheduledTimerWithTimeInterval:0.0166666667,target:delegate,selector:sel!(tick:),userInfo:std::ptr::null::<AnyObject>(),repeats:true];
        ui.setup_menu();
        if let Some(id)=library.last_opened_id {ui.open_book(&id,None);}
        let args:Vec<PathBuf>=std::env::args_os().skip(1).filter(|arg| !arg.to_string_lossy().starts_with('-')).map(PathBuf::from).filter(|p|p.exists()).collect();if !args.is_empty(){ui.pending_open=args.first().filter(|p|p.is_file()).cloned();ui.import(args);}
        Ok(ui)
    }
    unsafe fn setup_menu(&self){
        let bar=NSMenu::new(self.mtm);
        for (name,items) in [("Sumi",vec![("Sumiを終了","quit","q")]),("ファイル",vec![("本を取り込む…","import","o"),("本棚","library","l")]),("編集",vec![("取り消す","undo","z"),("切り取り","cut","x"),("コピー","copy","c"),("ペースト","paste","v"),("すべて選択","select-all","a"),("本文を検索","find","f"),("すべて検索","palette","k")]),("表示",vec![("ページを選ぶ","pages","g"),("全体表示","fit","0"),("拡大","zoom-in","+"),("縮小","zoom-out","-"),("見開きを切り替える","spread","")])]{
            let menu=NSMenu::initWithTitle(NSMenu::alloc(self.mtm),&s(name));
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
        let top_h=if self.focus{0.}else{38.};let bottom_h=if self.focus{0.}else{44.};
        self.reader.resize(rect(8.,bottom_h,size.width-16.,size.height-top_h-bottom_h-5.));
        self.top.setFrame(rect(0.,size.height-38.,size.width,38.));self.top.setHidden(self.focus);self.footer.setHidden(self.focus);
        self.title.setFrame(rect(160.,9.,size.width-320.,20.));
        let children=self.top.subviews();for(i,v)in children.iter().enumerate(){if i>=2{v.setFrame(rect(size.width-124.+((i-2) as f64)*36.,4.,32.,30.));}}
        self.footer.setFrame(rect((size.width-350.)/2.,7.,350.,34.));
        self.status.setFrame(rect(16.,if self.current.is_none(){size.height/2.}else{48.},size.width-32.,22.));
        let w=if self.panel_kind==Panel::Find{480.0_f64}else if self.panel_kind==Panel::Pages{660.0_f64}else{580.0_f64}.min(size.width-32.);
        let h=if self.panel_kind==Panel::Find{if self.query.stringValue().is_empty(){48.0_f64}else{340.0_f64}}else if self.panel_kind==Panel::Pages{684.0_f64}else{440.0_f64}.min(size.height-92.);
        let x=if self.panel_kind==Panel::Find{size.width-w-16.}else{(size.width-w)/2.};let y=size.height-h-52.;
        self.panel.setFrame(rect(x,y,w,h));self.panel_title.setFrame(rect(18.,h-30.,w-68.,20.));
        self.panel.subviews().objectAtIndex(1).setFrame(rect(w-42.,h-34.,28.,28.));
        let finding=self.panel_kind==Panel::Find;let empty_find=finding&&self.query.stringValue().is_empty();self.panel_title.setHidden(finding);self.rows_scroll.setHidden(empty_find);self.panel_hint.setHidden(empty_find);
        if finding {self.panel.subviews().objectAtIndex(1).setFrame(rect(w-40.,h-38.,28.,28.));self.query.setFrame(rect(16.,h-37.,w-60.,26.));}else{self.query.setFrame(rect(18.,h-70.,w-36.,30.));}
        self.rows_scroll.setFrame(rect(8.,34.,w-16.,(h-if finding{86.}else{112.}).max(0.)));self.panel_hint.setFrame(rect(18.,10.,w-36.,18.));
        self.page_controls.setHidden(self.panel_kind!=Panel::Pages);self.page_controls.setFrame(rect(w-90.,6.,78.,26.));
        self.render_rows();
    }
    unsafe fn apply_preferences(&mut self){
        let dark=self.preferences.theme=="night";let appearance=NSAppearance::appearanceNamed(&s(if dark{"NSAppearanceNameDarkAqua"}else{"NSAppearanceNameAqua"}));self.window.setAppearance(appearance.as_deref());
        let bg=if dark{NSColor::colorWithSRGBRed_green_blue_alpha(0.12,0.13,0.14,1.)}else{NSColor::colorWithSRGBRed_green_blue_alpha(0.92,0.92,0.90,1.)};self.window.setBackgroundColor(Some(&bg));
        let chrome=if dark{NSColor::colorWithSRGBRed_green_blue_alpha(0.18,0.19,0.20,1.)}else{NSColor::colorWithSRGBRed_green_blue_alpha(0.98,0.98,0.97,1.)};
        decorate(&self.top,&chrome,0.);decorate(&self.footer,&chrome,12.);decorate(&self.panel,&chrome,14.);
        self.reader.set_spread(self.preferences.mode=="spread");self.reader.set_rtl(self.preferences.direction=="rtl");self.reader.set_theme(&self.preferences.theme);
        self.spread_button.setToolTip(Some(&s(if self.preferences.mode=="spread"{"1ページにする"}else{"見開きにする"})));
        self.spread_button.setState(if self.preferences.mode=="spread"{1}else{0});self.spread_button.setAccessibilityLabel(Some(&s(if self.preferences.mode=="spread"{"1ページにする"}else{"見開きにする"})));
        self.last_page=0;self.last_zoom=0;
    }
    unsafe fn tick(&mut self){
        let commands=COMMANDS.with(|q|q.borrow_mut().drain(..).collect::<Vec<_>>());
        for command in commands{match command{Command::Action(name,value)=>self.action(&name,value),Command::Files(paths)=>{self.pending_open=paths.first().filter(|p|p.is_file()).cloned();self.import(paths);},Command::BookAction(action,id)=>self.book_action(action,&id),Command::Outside(point)=>{if self.panel_kind!=Panel::None && !contains(self.panel.frame(),point) && point.y<self.size.height-38. && (self.panel_kind!=Panel::Find||self.query.stringValue().to_string().trim().is_empty()){self.close_panel();}}}}
        self.reader.tick();if self.panel_kind==Panel::Pages{self.thumbnails.render_next();}
        if self.search_due.is_some_and(|t|Instant::now()>=t){self.search_due=None;self.begin_search();}
        while let Some(result)=self.search.try_recv(){if result.request_id==self.request_id && matches!(self.panel_kind,Panel::Find|Panel::Palette){self.search_running=!result.complete;self.text_hits=result.hits;let count=self.text_hits.len();self.panel_hint.setStringValue(&s(&if result.complete{if result.warnings.is_empty(){format!("{count}件　↑↓ 選択　↵ 移動　Esc 閉じる")}else{format!("{count}件 · {}",result.warnings[0])}}else{format!("検索中… {count}件")}));self.populate_rows();}}
        let result=self.import_receiver.as_ref().and_then(|rx|rx.try_recv().ok());if let Some(result)=result{self.import_receiver=None;self.import_busy=false;match result{Ok(report)=>{let summary=format!("{}冊を追加 · 重複{}冊 · 読めなかったファイル{}件{}",report.added.len(),report.duplicates,report.errors.len(),if report.limited{" · 一部は上限に達したため未処理"}else{""});self.toast(&summary);if !report.errors.is_empty(){self.show_import_errors(&report);}else if self.panel_kind==Panel::Library{self.populate_rows();}if let Some(path)=self.pending_open.take(){let canonical=std::fs::canonicalize(path).ok();let entry=self.store.snapshot().ok().and_then(|library|library.recent.into_iter().find(|e|Some(&e.path)==canonical.as_ref()));if let Some(entry)=entry{self.open_book(&entry.document.id,None);}}else if self.current.is_none(){if let Some(entry)=report.added.first(){self.open_book(&entry.document.id,None);}}},Err(error)=>{self.pending_open=None;self.toast(&error)}}}
        let page=self.reader.current_page();let zoom=(self.reader.zoom_factor()*100.).round() as usize;
        if page!=self.last_page || zoom!=self.last_zoom {let changed=page!=self.last_page;self.last_page=page;self.last_zoom=zoom;self.refresh_chrome();if changed&&self.current.is_some(){self.dirty=Some(Instant::now()+Duration::from_millis(400));}}
        if self.dirty.is_some_and(|t|Instant::now()>=t){self.save();}
        if self.toast_until.is_some_and(|t|Instant::now()>=t){self.toast_until=None;self.status.setHidden(self.current.is_some());}
    }
    unsafe fn action(&mut self,name:&str,value:usize){
        match name {
            "control"=>{if let Some(action)=CONTROLS.get(value){self.action(action,0);}},
            "layout"=>self.layout(),"find"=>self.toggle(Panel::Find),"palette"=>self.toggle(Panel::Palette),"library"=>self.toggle(Panel::Library),"tools"=>self.toggle(Panel::Tools),"pages"=>self.toggle(Panel::Pages),"outline"=>self.toggle(Panel::Outline),"bookmarks"=>self.toggle(Panel::Bookmarks),
            "escape"=>{if self.panel_kind!=Panel::None{self.close_panel();}else if self.reader.cancel_interaction(){}else if self.reader.zoom_factor()>1.01{self.reader.fit();}else if self.focus{self.focus=false;self.layout();}else{self.reader.cancel_interaction();}},
            "close"=>self.close_panel(),"query"=>{self.last_find_selected=None;self.selected=0;self.text_hits.clear();self.search.cancel();self.search_running=matches!(self.panel_kind,Panel::Find|Panel::Palette)&&!self.query.stringValue().to_string().trim().is_empty();self.populate_rows();self.layout();if matches!(self.panel_kind,Panel::Find|Panel::Palette)&&!self.query.stringValue().to_string().trim().is_empty(){self.search_due=Some(Instant::now()+Duration::from_millis(180));}},
            "row-down"=>{self.last_find_selected=None;if !self.rows.is_empty(){self.selected=(self.selected+if self.panel_kind==Panel::Pages{COLUMNS}else{1}).min(self.rows.len()-1);self.highlight_row();}},"row-up"=>{self.last_find_selected=None;self.selected=self.selected.saturating_sub(if self.panel_kind==Panel::Pages{COLUMNS}else{1});self.highlight_row();},"row"=>{self.selected=value;self.last_find_selected=None;self.accept();},"accept"|"accept-prev"=>{if self.panel_kind==Panel::Find&&self.rows.iter().any(|r|matches!(r.action,RowAction::Hit(_))) {if name=="accept-prev"{self.selected=(self.selected+self.rows.len()-1)%self.rows.len();}else if self.last_find_selected==Some(self.selected){self.selected=(self.selected+1)%self.rows.len();}self.highlight_row();}self.accept();},
            "left"=>{if self.panel_kind==Panel::None{self.reader.step(if self.preferences.direction=="rtl"{1}else{-1});}},"right"=>{if self.panel_kind==Panel::None{self.reader.step(if self.preferences.direction=="rtl"{-1}else{1});}},"previous"=>self.reader.step(-1),"next"=>self.reader.step(1),
            "fit"=>{self.reader.fit();},"zoom-in"=>{self.reader.zoom(1);},"zoom-out"=>{self.reader.zoom(-1);},
            "spread"=>{self.preferences.mode=if self.preferences.mode=="spread"{"single"}else{"spread"}.into();self.persist_preferences();},
            "white"|"paper"|"night"=>{self.preferences.theme=name.into();self.persist_preferences();},"direction"=>{self.preferences.direction=if self.preferences.direction=="rtl"{"ltr"}else{"rtl"}.into();self.persist_preferences();},
            "focus"=>{self.close_panel();self.focus=!self.focus;self.layout();},"bookmark"=>self.bookmark(),
            "capture"=>{self.close_panel();self.reader.begin_capture();self.toast("範囲をドラッグして画像をコピー · Escで取消");},
            "copy-selection"=>{match self.reader.copy_selection(){Ok(_)=>self.toast("選んだ文章を出典付きでコピーしました"),Err(e)=>self.toast(&e)}},
            "copy-page"=>{match self.reader.copy_page_text(){Ok(_)=>self.toast("ページの文章をコピーしました"),Err(e)=>self.toast(&e)}},
            "reader-page"=>{self.refresh_chrome();self.dirty=Some(Instant::now()+Duration::from_millis(400));},"reader-zoom"=>self.refresh_chrome(),
            "capture-ready"=>{self.last_capture=self.reader.take_capture();self.capture_name=format!("{}-p{}.png",self.current.as_ref().map(|b|b.title.as_str()).unwrap_or("Sumi"),self.reader.current_page());self.toast("画像をコピーしました · ⌘Kから保存もできます");},
            "save-capture"=>self.save_capture(),
            "reader-click"=>{if self.panel_kind==Panel::Find&&self.query.stringValue().to_string().trim().is_empty(){self.close_panel();}},
            "page-back"|"page-forward"=>{self.page_anchor=if name=="page-back"{self.page_anchor.saturating_sub(24).max(1)}else{(self.page_anchor+24).min(self.reader.page_count())};self.query.setStringValue(&s(""));self.populate_rows();},
            "import"=>self.choose_import(),_=>{}
        }
    }
    unsafe fn persist_preferences(&mut self){if let Err(e)=self.store.save_preferences(&json!({"theme":self.preferences.theme,"mode":self.preferences.mode,"direction":self.preferences.direction})){self.toast(&e);}self.apply_preferences();if self.panel_kind==Panel::Tools{self.populate_rows();}}
    fn save(&mut self){self.dirty=None;let result=if let Some(book)=self.current.as_mut(){book.state["page"]=json!(self.reader.current_page().max(1));self.store.save_state(&book.id,&book.state)}else{Ok(())};if let Err(error)=result{self.dirty=Some(Instant::now()+Duration::from_secs(2));unsafe{self.toast(&format!("読書位置を保存できません: {error}"));}}}
    unsafe fn refresh_chrome(&mut self){let page=self.reader.current_page();let count=self.reader.page_count();self.page_button.setTitle(&s(&if count>0{format!("{page} / {count}")}else{"ページ".into()}));self.page_button.setAccessibilityLabel(Some(&s(&format!("ページを選ぶ · {page} / {count}"))));let zoom=self.reader.zoom_factor();self.zoom_button.setTitle(&s(&if zoom>1.01{format!("{}%",(zoom*100.).round() as usize)}else{"全体表示".into()}));}
    unsafe fn toast(&mut self,text:&str){self.status.setStringValue(&s(text));self.status.setHidden(false);self.toast_until=Some(Instant::now()+Duration::from_secs(4));}
    unsafe fn toggle(&mut self,kind:Panel){if self.panel_kind==kind{self.close_panel();return;}self.search.cancel();self.search_due=None;self.search_running=false;self.panel_kind=kind;self.text_hits.clear();self.selected=0;self.page_anchor=self.reader.current_page().max(1);self.last_find_selected=None;self.query.setStringValue(&s(if kind==Panel::Find{&self.last_find}else{""}));self.panel.setHidden(false);
        let (title,placeholder)=match kind{Panel::Find=>("本文を検索","この本の中を検索"),Panel::Palette=>("すべて検索","本・本文・ページ・操作を検索"),Panel::Library=>("本棚","タイトルで絞り込む"),Panel::Pages=>("ページを選ぶ","ページ番号を入力"),Panel::Tools=>("表示と操作","操作を絞り込む"),Panel::Outline=>("目次","見出しを検索"),Panel::Bookmarks=>("しおり","ページ番号で絞り込む"),Panel::None=>("","")};
        self.panel_title.setStringValue(&s(title));self.query.setPlaceholderString(Some(&s(placeholder)));self.panel_hint.setStringValue(&s("↑↓ 選択　↵ 開く　Esc 閉じる"));self.populate_rows();self.layout();self.window.makeFirstResponder(Some(&*self.query));self.query.selectText(None);if kind==Panel::Find&&!self.last_find.is_empty(){self.search_running=true;self.search_due=Some(Instant::now()+Duration::from_millis(100));}
    }
    unsafe fn close_panel(&mut self){if self.panel_kind==Panel::Find{self.last_find=self.query.stringValue().to_string();}self.panel_kind=Panel::None;self.panel.setHidden(true);self.search.cancel();self.search_due=None;self.search_running=false;self.reader.focus();}
    unsafe fn populate_rows(&mut self){
        let query=self.query.stringValue().to_string();let query=query.trim().to_lowercase();let mut rows=vec![];
        if matches!(self.panel_kind,Panel::Palette|Panel::Tools) {
            let actions=[("本を取り込む…","PDF・フォルダをまとめて追加","import"),("本棚を開く","取り込んだ本から選ぶ","library"),("本文を検索","⌘F","find"),("ページを選ぶ","G / ⌘G","pages"),("目次を開く","章や見出しに移動","outline"),("しおりを開く","保存したページに移動","bookmarks"),("しおりを切り替える","⌘D","bookmark"),("1ページ / 見開き","アプリ全体に適用","spread"),("全体表示に戻す","⌘0","fit"),("拡大","⌘+","zoom-in"),("縮小","⌘−","zoom-out"),("範囲を画像でコピー","⌘⇧X","capture"),("切り抜いた画像を保存…","最後にコピーした範囲をPNGで保存","save-capture"),("選んだ文章をコピー","書名・ページ付き","copy-selection"),("ページの文章をコピー","書名・ページ付き","copy-page"),("ライト","外観","white"),("セピア","外観","paper"),("ダーク","外観","night"),("読み進める方向を切り替える","左右のページ送り","direction"),("読書に集中","⌘⇧F","focus")];
            for(title,detail,action)in actions{if query.is_empty()||format!("{title} {detail}").to_lowercase().contains(&query){rows.push(Row{title:title.into(),detail:detail.into(),action:RowAction::Command(action)});}}
        }
        if matches!(self.panel_kind,Panel::Palette|Panel::Library){if let Ok(library)=self.store.public_library(){for book in library.recent{if query.is_empty()||format!("{} {}",book.title,book.name).to_lowercase().contains(&query){let page=book.state["page"].as_u64().unwrap_or(1);rows.push(Row{title:book.title.clone(),detail:format!("本 · {page}ページから"),action:RowAction::Book(book.id)});}}}if self.panel_kind==Panel::Library{rows.insert(0,Row{title:if self.import_busy{"取り込み中…"}else{"本を取り込む…"}.into(),detail:"複数PDF・フォルダを選択　⌘O".into(),action:RowAction::Command("import")});}}
        if matches!(self.panel_kind,Panel::Palette|Panel::Pages){let number=query.trim_start_matches('p').trim_start_matches('.').trim_end_matches("ページ").trim().parse::<usize>().ok().filter(|p|*p>0&&*p<=self.reader.page_count());
            if self.panel_kind==Panel::Pages{let anchor=number.unwrap_or(self.page_anchor);for page in pages_near(anchor,self.reader.page_count()){rows.push(Row{title:format!("{page}ページ"),detail:if page==self.reader.current_page(){"読書中"}else{""}.into(),action:RowAction::Page(page)});}self.selected=rows.iter().position(|row|matches!(row.action,RowAction::Page(page) if page==anchor)).unwrap_or(0);self.panel_hint.setStringValue(&s(&format!("全{}ページ · 番号入力で移動",self.reader.page_count())));
            }else if let Some(page)=number{rows.insert(0,Row{title:format!("{page}ページへ移動"),detail:"この本".into(),action:RowAction::Page(page)});}}
        if matches!(self.panel_kind,Panel::Outline|Panel::Palette){for(title,page)in self.reader.outline(){if query.is_empty()&&self.panel_kind==Panel::Outline||!query.is_empty()&&title.to_lowercase().contains(&query){rows.push(Row{title,detail:format!("目次 · {page}ページ"),action:RowAction::Page(page)});}}}
        if self.panel_kind==Panel::Bookmarks{if let Some(book)=&self.current{if let Some(pages)=book.state["bookmarks"].as_array(){for value in pages{if let Some(page)=value.as_u64(){if query.is_empty()||page.to_string().contains(&query){rows.push(Row{title:format!("{page}ページ"),detail:"しおり".into(),action:RowAction::Page(page as usize)});}}}}}}
        if matches!(self.panel_kind,Panel::Find|Panel::Palette){for hit in &self.text_hits{rows.push(Row{title:format!("{} · {}ページ",hit.title,hit.page),detail:hit.snippet.clone(),action:RowAction::Hit(hit.clone())});}}
        if rows.is_empty(){rows.push(Row{title:if query.is_empty(){if self.panel_kind==Panel::Find{"言葉を入力して本文を検索"}else{"まだ項目がありません"}}else if self.search_running{"検索しています…"}else{"一致する項目がありません"}.into(),detail:String::new(),action:RowAction::None});}
        if self.panel_kind==Panel::Palette&&query.is_empty(){rows.sort_by_key(|r|if matches!(r.action,RowAction::Book(_)){0}else{1});}
        rows.truncate(160);self.rows=rows;self.selected=self.selected.min(self.rows.len().saturating_sub(1));self.render_rows();
    }
    unsafe fn render_rows(&mut self){
        for view in self.rows_view.subviews().iter(){view.removeFromSuperview();}self.row_buttons.clear();let w=self.rows_scroll.contentSize().width.max(100.);
        if self.panel_kind==Panel::Pages {let pages:Vec<usize>=self.rows.iter().filter_map(|r|if let RowAction::Page(p)=r.action{Some(p)}else{None}).collect();self.thumbnails.show(self.reader.document(),&pages,self.reader.current_page(),w);let h=self.thumbnails.height().max(self.rows_scroll.contentSize().height);self.rows_view.setFrame(rect(0.,0.,w,h));self.thumbnails.view().setFrameOrigin(NSPoint::new(0.,h-self.thumbnails.height()));self.rows_view.addSubview(self.thumbnails.view());self.row_buttons=self.thumbnails.buttons().to_vec();if let Some(Row{action:RowAction::Page(page),..})=self.rows.get(self.selected){self.thumbnails.prioritize(*page);}self.highlight_row();return;}
        let height=(self.rows.len() as f64*52.).max(self.rows_scroll.contentSize().height);self.rows_view.setFrame(rect(0.,0.,w,height));
        for(i,row)in self.rows.iter().enumerate(){let b=NSButton::buttonWithTitle_target_action(&s(&format!("{}\n{}",row.title,row.detail)),Some(&*self.delegate),Some(sel!(activateRow:)),self.mtm);b.setTag(i as isize);b.setBordered(false);b.setAlignment(NSTextAlignment::Left);b.setFont(Some(&NSFont::systemFontOfSize(13.)));let _:()=msg_send![&b,setLineBreakMode:NSLineBreakMode::ByTruncatingTail];b.setFrame(rect(4.,height-(i+1) as f64*52.,w-8.,50.));b.setAccessibilityLabel(Some(&s(&format!("{} {}",row.title,row.detail))));if let RowAction::Book(id)=&row.action {let menu=NSMenu::new(self.mtm);for (tag,name) in ["Finderで表示","場所を指定し直す…","本棚から外す…"].iter().enumerate(){let item=NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm),&s(name),Some(sel!(bookAction:)),&s(""));item.setTag(tag as isize);item.setTarget(Some(&*self.delegate));item.setRepresentedObject(Some(&*s(id)));menu.addItem(&item);}b.setMenu(Some(&menu));}self.rows_view.addSubview(&b);self.row_buttons.push(b);}
        self.highlight_row();
    }
    unsafe fn highlight_row(&self){for(i,b)in self.row_buttons.iter().enumerate(){let color=if i==self.selected{NSColor::controlAccentColor().colorWithAlphaComponent(0.13)}else{NSColor::clearColor()};decorate(b,&color,7.);let tint=NSColor::labelColor();b.setContentTintColor(Some(&tint));if self.panel_kind!=Panel::Pages{if let Some(row)=self.rows.get(i){b.setAttributedTitle(&row_title(row,self.mtm));}}}if let Some(b)=self.row_buttons.get(self.selected){let frame=b.convertRect_toView(b.bounds(),Some(&self.rows_view));self.rows_view.scrollRectToVisible(frame);}}
    unsafe fn accept(&mut self){let action=self.rows.get(self.selected).map(|r|r.action.clone());match action{Some(RowAction::Command(action))=>{self.close_panel();self.action(action,0);},Some(RowAction::Book(id))=>{self.close_panel();self.open_book(&id,None);},Some(RowAction::Page(page))=>{self.close_panel();self.reader.go_to(page,false);},Some(RowAction::Hit(hit))=>{let query=self.query.stringValue().to_string();let keep=self.panel_kind==Panel::Find;if !keep{self.close_panel();}if self.current.as_ref().is_some_and(|b|b.id==hit.book_id){self.reader.go_to(hit.page,false);}else{self.open_book(&hit.book_id,Some(hit.page));}self.reader.highlight(&query);if keep{self.last_find_selected=Some(self.selected);}},_=>{}}}
    unsafe fn begin_search(&mut self){let query=self.query.stringValue().to_string();if query.trim().is_empty(){return;}let mut books=vec![];if let Ok(library)=self.store.public_library(){for book in library.recent{if self.panel_kind==Panel::Find&&!self.current.as_ref().is_some_and(|b|b.id==book.id){continue;}if let Ok(path)=self.store.authorized_path(&book.id){let password=if self.current.as_ref().is_some_and(|b|b.id==book.id){self.password.clone()}else{None};books.push(SearchBook{id:book.id,title:book.title,path,password});}}}self.request_id=self.search.submit(SearchRequest{query,books,limit:100});self.panel_hint.setStringValue(&s("検索中… · Escで閉じる"));}
    unsafe fn open_book(&mut self,id:&str,page:Option<usize>){
        self.save();let path=match self.store.authorized_path(id){Ok(path)=>path,Err(e)=>{self.toast(&e);return;}};
        let mut password=None;let mut result=self.reader.open(&path,None);
        while matches!(&result,Err(error) if error=="PASSWORD_REQUIRED"||error=="PASSWORD_INCORRECT") {
            let alert=NSAlert::new(self.mtm);alert.setMessageText(&s("このPDFのパスワードを入力"));alert.setInformativeText(&s(if matches!(&result,Err(e) if e=="PASSWORD_INCORRECT"){ "パスワードが違います。もう一度入力してください。" }else{"パスワードはこの起動中だけ使い、保存しません。"}));
            let field=NSSecureTextField::initWithFrame(NSSecureTextField::alloc(self.mtm),rect(0.,0.,300.,28.));alert.setAccessoryView(Some(&field));alert.addButtonWithTitle(&s("開く"));alert.addButtonWithTitle(&s("キャンセル"));alert.window().setInitialFirstResponder(Some(&field));if alert.runModal()!=1000{return;}
            password=Some(field.stringValue().to_string());result=self.reader.open(&path,password.as_deref());
        }
        match result {
            Ok(info)=>{if let Some(title)=crate::search::derive_title(&path).or_else(||(!info.title.trim().is_empty()).then_some(info.title)){let _=self.store.set_book_title(id,&title);}
                let entry=self.store.entry(id);if let Ok(entry)=entry{let book=entry.document;let position=page.unwrap_or_else(||book.state["page"].as_u64().unwrap_or(1) as usize);self.title.setStringValue(&s(&book.title));self.window.setTitle(&s(&format!("{} — Sumi",book.title)));self.reader.set_title(&book.title);self.thumbnails.clear();self.current=Some(book);self.password=password;self.reader.go_to(position,false);self.status.setHidden(true);self.refresh_chrome();if let Err(error)=self.store.open(id){self.toast(&format!("本は開けましたが、履歴を保存できません: {error}"));}}
            },Err(e)=>self.toast(&e)
        }
    }
    unsafe fn book_action(&mut self,action:usize,id:&str){
        match action {
            0=>{if let Ok(path)=self.store.authorized_path(id){let url=objc2_foundation::NSURL::fileURLWithPath(&s(&path.to_string_lossy()));NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));}},
            1=>{let panel=NSOpenPanel::openPanel(self.mtm);panel.setTitle(Some(&s("移動したPDFを選ぶ")));panel.setAllowsMultipleSelection(false);panel.setCanChooseDirectories(false);#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("pdf")])));if panel.runModal()==1{if let Some(path)=panel.URL().and_then(|url|url.path()){match self.store.relink(id,&PathBuf::from(path.to_string())){Ok(_)=>{self.toast("本の場所を更新しました");self.populate_rows();},Err(e)=>self.toast(&e)}}}},
            2=>{let alert=NSAlert::new(self.mtm);alert.setMessageText(&s("この本を本棚から外しますか？"));alert.setInformativeText(&s("PDFファイルは残ります。読書位置としおりは本棚から削除されます。"));alert.addButtonWithTitle(&s("キャンセル"));alert.addButtonWithTitle(&s("本棚から外す"));if alert.runModal()==1001{if self.current.as_ref().is_some_and(|b|b.id==id){self.toast("読んでいる本は、別の本を開いてから外せます");}else{match self.store.remove(id){Ok(_)=>{self.populate_rows();self.toast("本棚から外しました");},Err(e)=>self.toast(&e)}}}},_=>{}
        }
    }
    unsafe fn show_import_errors(&mut self,report:&ImportReport){
        if self.panel_kind!=Panel::Library{self.toggle(Panel::Library);}self.panel_title.setStringValue(&s("取り込み結果"));self.rows=report.errors.iter().map(|e|Row{title:e.path.file_name().unwrap_or_default().to_string_lossy().into_owned(),detail:e.message.clone(),action:RowAction::None}).collect();self.panel_hint.setStringValue(&s("追加できた本は本棚に保存されています · Esc 閉じる"));self.render_rows();
    }
    unsafe fn bookmark(&mut self){if let Some(book)=self.current.as_mut(){let page=self.reader.current_page().max(1);let mut marks=book.state["bookmarks"].as_array().cloned().unwrap_or_default();let exists=marks.iter().any(|v|v.as_u64()==Some(page as u64));marks.retain(|v|v.as_u64()!=Some(page as u64));if !exists{marks.push(json!(page));}marks.sort_by_key(|v|v.as_u64().unwrap_or(0));book.state["bookmarks"]=json!(marks);self.save();self.toast(if exists{"しおりを外しました"}else{"しおりを付けました"});}}
    unsafe fn save_capture(&mut self){
        let Some(image)=self.last_capture.as_ref()else{self.toast("先に⌘⇧Xで範囲を選んでください");return;};
        let Some(tiff)=image.TIFFRepresentation()else{self.toast("画像を読み込めませんでした");return;};let class=objc2::runtime::AnyClass::get(c"NSBitmapImageRep").unwrap();let rep:Option<Retained<AnyObject>>=msg_send![class,imageRepWithData:&*tiff];let Some(rep)=rep else{self.toast("画像を読み込めませんでした");return;};
        let properties=objc2_foundation::NSDictionary::<NSString,AnyObject>::new();let png:Option<Retained<objc2_foundation::NSData>>=msg_send![&rep,representationUsingType:NSBitmapImageFileType::PNG,properties:&*properties];let Some(png)=png else{self.toast("PNGを作成できませんでした");return;};
        let panel=NSSavePanel::savePanel(self.mtm);panel.setTitle(Some(&s("切り抜いた画像を保存")));panel.setNameFieldStringValue(&s(&self.capture_name.replace('/',"-")));#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("png")])));if panel.runModal()==1{if let Some(url)=panel.URL(){let ok:bool=msg_send![&png,writeToURL:&*url,atomically:true];self.toast(if ok{"画像を保存しました"}else{"画像を保存できませんでした"});}}
    }
    unsafe fn choose_import(&mut self){if self.import_busy{self.toast("本を取り込み中です");return;}let panel=NSOpenPanel::openPanel(self.mtm);panel.setCanChooseFiles(true);panel.setCanChooseDirectories(true);panel.setAllowsMultipleSelection(true);panel.setTitle(Some(&s("本を取り込む")));panel.setMessage(Some(&s("PDFまたはフォルダを選んでください。元のファイルは移動しません。")));panel.setPrompt(Some(&s("取り込む")));#[allow(deprecated)]panel.setAllowedFileTypes(Some(&NSArray::from_slice(&[&*s("pdf")])));let result=panel.runModal();if result==1{let paths=panel.URLs().iter().filter_map(|u|u.path().map(|p|PathBuf::from(p.to_string()))).collect();self.import(paths);}}
    unsafe fn import(&mut self,paths:Vec<PathBuf>){if paths.is_empty(){return;}if self.import_busy{self.toast("本を取り込み中です。完了してから追加してください");return;}self.import_busy=true;self.toast("本を取り込んでいます…");let store=self.store.clone();let(tx,rx)=mpsc::channel();self.import_receiver=Some(rx);std::thread::spawn(move||{let result=store.import_paths(&paths).map(|report|{for entry in &report.added{if let Some(title)=crate::search::derive_title(&entry.path){let _=store.set_book_title(&entry.document.id,&title);}}report});let _=tx.send(result);});}
}

pub fn run(){let mtm=MainThreadMarker::new().expect("Sumi requires the macOS main thread");let app=NSApplication::sharedApplication(mtm);let delegate=Delegate::new(mtm);app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));app.run();}
