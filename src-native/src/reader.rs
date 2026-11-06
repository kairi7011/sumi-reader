use sumi_native::i18n::text as tr;
use std::{cell::RefCell, path::{Path, PathBuf}, ptr::NonNull, rc::Rc, time::{Duration, Instant}};
use block2::RcBlock;
use objc2::{define_class, msg_send, ClassType, MainThreadMarker, MainThreadOnly, AnyThread};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, AnyClass};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSColor, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSEventPhase, NSImage, NSPasteboard, NSPasteboardTypeString, NSScrollView, NSView, NSGraphicsContext, NSCursor, NSDeviceRGBColorSpace, NSWorkspace};
use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize, NSString, NSArray, NSObject, NSObjectProtocol, NSURL};
use objc2_core_graphics::CGContext;
#[path = "animation.rs"]
mod animation;
use animation::{Curl, Overlay, PageImage, Visual, EdgeHint};
use crate::pdf::{PDFDocument, PDFPage, PDFSelection, PDFView};

fn allowed_pdf_link(url: &NSURL) -> bool {
    let scheme = url.scheme().map(|value| value.to_string().to_ascii_lowercase());
    match scheme.as_deref() {
        Some("http" | "https") => url.host().is_some_and(|value| !value.is_empty()),
        Some("mailto") => true,
        _ => false,
    }
}

define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    struct LinkDelegate;
    unsafe impl NSObjectProtocol for LinkDelegate {}
    impl LinkDelegate {
        #[unsafe(method(PDFViewWillClickOnLink:withURL:))]
        fn click_link(&self, _view: &PDFView, url: &NSURL) {
            // PDFKit otherwise forwards URI and Launch actions to any installed URL handler.
            if allowed_pdf_link(url) {
                NSWorkspace::sharedWorkspace().openURL(url);
            } else {
                objc2_app_kit::NSBeep();
            }
        }
    }
);

#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub title: String,
    pub page_count: usize,
    pub path: PathBuf,
    pub allows_copying: bool,
}

struct Settle { start: Instant, duration: Duration, from: f64, to: f64, vertical: f64, tangent: f64 }
struct Turn { old: usize, target: usize, settle: Option<Settle>, start: NSPoint, last: NSPoint, sample: Instant, velocity: f64, excursion: f64 }

#[derive(Clone, Copy)]
struct EdgePress { side: i8, start: NSPoint, distance: f64 }
struct ScrollSession { gesture: PagingGesture, target: Option<usize>, animated: bool }
#[derive(Debug)]
struct PagingGesture { x: f64, y: f64, width: f64, direction: Option<i8>, vertical: bool, velocity: f64, last_time: f64, last_motion: f64, finished: bool }
impl PagingGesture {
    fn new(width: f64, time: f64) -> Self { Self { x: 0., y: 0., width: width.max(1.), direction: None, vertical: false, velocity: 0., last_time: time, last_motion: time, finished: false } }
    fn update(&mut self, dx: f64, dy: f64, time: f64, momentum: bool) -> Option<i8> {
        if momentum || self.finished {return None;}
        self.x += dx; self.y += dy;
        if dx.abs() > 0.01 { self.velocity = dx / self.width / (time-self.last_time).max(1./120.); self.last_motion = time; }
        self.last_time = time;
        if self.direction.is_none() && !self.vertical {
            if self.x.abs() >= 12. && self.x.abs() > self.y.abs()*1.35 { let direction=if self.x>0. {1}else{-1}; self.direction=Some(direction); return Some(direction); }
            if self.y.abs() >= 12. && self.y.abs() > self.x.abs()*1.15 { self.vertical=true; }
        }
        None
    }
    fn progress(&self) -> f64 { (self.x*self.direction.unwrap_or(0) as f64/self.width).clamp(0.,1.) }
    fn speed(&self, time: f64) -> f64 { if time-self.last_motion > 0.12 {0.} else {self.velocity*self.direction.unwrap_or(0) as f64} }
    fn finish(&mut self, cancelled: bool, time: f64) -> bool { let commit=!self.finished && !cancelled && self.direction.is_some() && (self.progress()>0.22 || self.progress()>=0.08 && self.speed(time)>0.8);self.finished=true;commit }
}

struct ReaderState {
    view: Retained<PDFView>,
    enabled: bool,
    edge_press: Option<EdgePress>,
    scroll: Option<ScrollSession>,
    last_scroll: Option<Instant>,
    document: Option<Retained<PDFDocument>>,
    title: String,
    current: usize,
    spread: bool,
    rtl: bool,
    fit_scale: f64,
    factor: f64,
    space: bool,
    pan: Option<(NSPoint, NSPoint)>,
    last_scale: f64,
    overlay: Retained<Overlay>,
    visual: Rc<RefCell<Visual>>,
    turn: Option<Turn>,
    capture_start: Option<NSPoint>,
    captured: Option<Retained<NSImage>>,
    #[cfg(debug_assertions)]
    debug_samples: u8,
}

pub struct Reader {
    container: Retained<NSView>,
    state: Rc<RefCell<ReaderState>>,
    monitor: Option<Retained<AnyObject>>,
    _link_delegate: Retained<LinkDelegate>,
}

impl Reader {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Self {
        let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let view: Retained<PDFView> = unsafe {
            let allocated: Allocated<PDFView> = msg_send![PDFView::class(), alloc];
            msg_send![allocated, initWithFrame: NSRect::new(NSPoint::new(0., 0.), frame.size)]
        };
        let link_delegate: Retained<LinkDelegate> = unsafe { msg_send![LinkDelegate::alloc(mtm), init] };
        unsafe {
            let _: () = msg_send![&*view, setDelegate: &*link_delegate];
            let _: () = msg_send![&*view, setDisplayMode: 0isize];
            let _: () = msg_send![&*view, setDisplaysAsBook: true];
            let _: () = msg_send![&*view, setMinScaleFactor: 0.000001f64];
            let _: () = msg_send![&*view, setMaxScaleFactor: 64f64];
            let _: () = msg_send![&*view, setAutoScales: true];
            let _: () = msg_send![&*view, setAcceptsDraggedFiles: false];
            let _: () = msg_send![&*view, setInterpolationQuality: 2isize];
            view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
            container.addSubview(&view);
        }
        let visual = Rc::new(RefCell::new(Visual::default()));
        let overlay = Overlay::new(mtm, NSRect::new(NSPoint::new(0., 0.), frame.size), visual.clone());
        container.addSubview(&overlay);
        let state = Rc::new(RefCell::new(ReaderState { view, enabled: true, edge_press: None, scroll: None, last_scroll: None, document: None, title: String::new(), current: 1, spread: false, rtl: false, fit_scale: 1., factor: 1., space: false, pan: None, last_scale: 1., overlay, visual, turn: None, capture_start: None, captured: None, #[cfg(debug_assertions)] debug_samples: 0 }));
        let weak = Rc::downgrade(&state);
        let block = RcBlock::new(move |pointer: NonNull<NSEvent>| -> *mut NSEvent {
            let Some(state) = weak.upgrade() else { return pointer.as_ptr(); };
            let event = unsafe { pointer.as_ref() };
            if handle_event(&state, event) { std::ptr::null_mut() } else { pointer.as_ptr() }
        });
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::Any, &block) };
        let reader = Self { container, state, monitor, _link_delegate: link_delegate };
        reader.set_theme("paper");
        reader
    }

    pub fn view(&self) -> &NSView { &self.container }
    pub fn set_interaction_enabled(&self, enabled: bool) {
        let mut state=self.state.borrow_mut(); if state.enabled==enabled {return;} state.enabled=enabled;
        if !enabled {
            state.cancel_scroll();
            let commit=state.turn.as_ref().and_then(|turn|turn.settle.as_ref()).is_some_and(|settle|settle.to>0.5);
            state.finish_turn(commit);state.edge_press=None;state.set_edge_hint(None);state.cancel_capture();state.pan=None;state.space=false;state.update_cursor();
        } else if let Some(window)=state.view.window() {window.setAcceptsMouseMovedEvents(true);}
    }
    pub fn clear(&self) {
        self.set_interaction_enabled(false);let mut state=self.state.borrow_mut();state.turn=None;state.scroll=None;state.edge_press=None;state.captured=None;
        {let mut visual=state.visual.borrow_mut();visual.curl=None;visual.edge=None;visual.capture=None;visual.capture_mode=false;}
        unsafe {let _:()=msg_send![&*state.view,setDocument:std::ptr::null::<PDFDocument>()];}
        state.document=None;state.title.clear();state.current=1;state.factor=1.;state.fit_scale=1.;state.last_scale=1.;state.overlay.repaint();
    }
    pub fn focus(&self) { let state = self.state.borrow(); if state.enabled {if let Some(window) = state.view.window() { window.setAcceptsMouseMovedEvents(true);window.makeFirstResponder(Some(&*state.view)); }} }
    pub fn resize(&self, frame: NSRect) {
        if self.container.frame() == frame { return; }
        { let mut state = self.state.borrow_mut(); state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn(); state.pan = None; state.update_cursor(); }
        self.container.setFrame(frame);
        if self.zoom_factor() <= 1.001 { self.fit(); }
    }
    pub fn set_frame(&self, frame: NSRect) { self.resize(frame); }
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<DocumentInfo, String> {
        let document = PDFDocument::from_path(path).ok_or(tr("PDFを開けませんでした。ファイルを確認してください。", "The PDF could not be opened. Check the file."))?;
        if document.is_locked() {
            let password = password.ok_or("PASSWORD_REQUIRED")?;
            if !document.unlock(password) { return Err("PASSWORD_INCORRECT".into()); }
        }
        if document.page_count() == 0 { return Err(tr("このPDFにはページがありません。", "This PDF has no pages.").into()); }
        let title = document.title().unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        let info = DocumentInfo { title: title.clone(), page_count: document.page_count(), path: path.to_owned(), allows_copying: document.allows_copying() };
        {
            let mut state = self.state.borrow_mut();
            state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn(); state.cancel_capture(); state.captured = None;
            state.current = 1; state.title = title; state.document = Some(document.clone()); state.factor = 1.; state.pan = None; state.space = false;
            unsafe { let _: () = msg_send![&*state.view, setDocument: &*document]; }
        }
        self.go_to(1, false); self.focus();
        Ok(info)
    }
    pub fn document(&self) -> Option<Retained<PDFDocument>> { self.state.borrow().document.clone() }
    pub fn current_page(&self) -> usize { self.state.borrow().current }
    pub fn is_turning(&self) -> bool { self.state.borrow().turn.is_some() }
    #[cfg(debug_assertions)]
    pub fn debug_scroll(&self, dx: f64, dy: f64, phase: usize, momentum: bool, time: f64) {
        let mut state=self.state.borrow_mut();if !state.enabled || state.view.isHiddenOrHasHiddenAncestor() || state.factor>1.001 {return;}
        let bounds=state.view.bounds();let point=NSPoint::new(bounds.size.width/2.,bounds.size.height*0.8);
        state.trackpad_sample(dx,dy,NSEventPhase::from_bits_retain(phase),momentum,time,point);
    }
    #[cfg(debug_assertions)]
    pub fn diagnostics(&self) -> serde_json::Value {
        let state=self.state.borrow();let bounds=state.view.bounds();let pages=state.visible();
        let fitted=state.factor<=1.001 && !pages.is_empty() && pages.iter().all(|(_,_,rect)|rect.origin.x>=-2. && rect.origin.y>=-2. && rect.origin.x+rect.size.width<=bounds.size.width+2. && rect.origin.y+rect.size.height<=bounds.size.height+2.);
        serde_json::json!({"page":state.current,"page_count":state.document.as_ref().map(|doc|doc.page_count()).unwrap_or(0),"zoom":state.factor,"visible_pages":pages.into_iter().map(|(page,_,rect)|serde_json::json!({"page":page,"rect":[rect.origin.x,rect.origin.y,rect.size.width,rect.size.height]})).collect::<Vec<_>>(),"bounds":[bounds.size.width,bounds.size.height],"fitted":fitted})
    }
    pub fn page_count(&self) -> usize { self.state.borrow().document.as_ref().map(|pdf| pdf.page_count()).unwrap_or(0) }
    pub fn zoom_factor(&self) -> f64 { self.state.borrow().factor }
    pub fn title(&self) -> String { self.state.borrow().title.clone() }
    pub fn set_title(&self, title: &str) { self.state.borrow_mut().title = title.to_owned(); }
    pub fn go_to(&self, page: usize, animated: bool) { self.state.borrow_mut().navigate(page, animated); }
    pub fn step(&self, delta: isize) { let mut state=self.state.borrow_mut();if state.enabled {state.step(delta);} }
    pub fn set_spread(&self, enabled: bool) {
        let mut state = self.state.borrow_mut(); if state.spread == enabled { return; } state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn(); state.spread = enabled;
        unsafe { let _: () = msg_send![&*state.view, setDisplayMode: if enabled { 2isize } else { 0isize }]; let _: () = msg_send![&*state.view, setDisplaysAsBook: true]; }
        let current = state.current; state.go_to(current);
    }
    pub fn set_rtl(&self, enabled: bool) { let mut state = self.state.borrow_mut(); if state.rtl == enabled { return; } state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn(); state.rtl = enabled; unsafe { let _: () = msg_send![&*state.view, setDisplaysRTL: enabled]; } state.fit(); }
    pub fn set_theme(&self, theme: &str) {
        let color = match theme { "night" => NSColor::colorWithSRGBRed_green_blue_alpha(0.09, 0.10, 0.10, 1.), "white" => NSColor::colorWithSRGBRed_green_blue_alpha(0.96, 0.96, 0.96, 1.), _ => NSColor::colorWithSRGBRed_green_blue_alpha(0.91, 0.91, 0.88, 1.) };
        let state=self.state.borrow();state.visual.borrow_mut().night=theme=="night";state.overlay.repaint();
        unsafe { let _: () = msg_send![&*state.view, setBackgroundColor: &*color]; }
    }
    pub fn zoom(&self, delta: i32) { let mut state = self.state.borrow_mut(); let next = if delta > 0 { state.factor * 1.25 } else { state.factor / 1.25 }; state.set_zoom(next); }
    pub fn restore_zoom(&self,factor:f64){self.state.borrow_mut().set_zoom(factor);}
    pub fn fit(&self) { let mut state = self.state.borrow_mut(); state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn(); state.fit(); state.update_cursor(); }
    pub fn outline(&self) -> Vec<(String, usize)> { self.document().map(|document| document.outline()).unwrap_or_default() }
    pub fn thumbnail(&self, page: usize, size: NSSize) -> Option<Retained<NSImage>> { self.document()?.page(page.checked_sub(1)?).map(|page| page.thumbnail(size)) }
    pub fn highlight(&self, query: &str) -> bool {
        let state = self.state.borrow();
        let Some(page) = state.document.as_ref().and_then(|pdf| pdf.page(state.current.saturating_sub(1))) else { return false; };
        let Some((location,length)) = crate::search::first_match_range(&page.text(),query) else { return false; };
        unsafe {
            let selection: Option<Retained<PDFSelection>> = msg_send![&*page, selectionForRange: NSRange::new(location,length)];
            if let Some(selection) = selection { let _: () = msg_send![&*state.view, setCurrentSelection: &*selection, animate: true]; return true; }
        }
        false
    }
    pub fn copy_selection(&self) -> Result<(), String> {
        let state = self.state.borrow();
        let document = state.document.as_ref().ok_or(tr("PDFを開いてください。", "Open a PDF first."))?;
        if !document.allows_copying() { return Err(tr("このPDFでは文章のコピーが許可されていません。", "This PDF does not allow text copying.").into()); }
        let selection: Option<Retained<PDFSelection>> = unsafe { msg_send![&*state.view, currentSelection] };
        let selection = selection.ok_or(tr("コピーする文章を選択してください。", "Select the text to copy."))?;
        let text = selection.text();
        if text.trim().is_empty() { return Err(tr("コピーする文章を選択してください。", "Select the text to copy.").into()); }
        let pages = selection.pages();
        let first: Option<Retained<PDFPage>> = unsafe { if pages.count() > 0 { msg_send![&*pages, objectAtIndex: 0usize] } else { None } };
        let page = first.map(|page| document.index_of(&page) + 1).unwrap_or(state.current);
        write_text(&format!("{} — p. {}\n\n{}", state.title, page, text.trim()))
    }
    pub fn copy_page_text(&self) -> Result<(), String> {
        let state = self.state.borrow();
        let document = state.document.as_ref().ok_or(tr("PDFを開いてください。", "Open a PDF first."))?;
        if !document.allows_copying() { return Err(tr("このPDFでは文章のコピーが許可されていません。", "This PDF does not allow text copying.").into()); }
        let text = document.page(state.current - 1).map(|page| page.text()).unwrap_or_default();
        if text.trim().is_empty() { return Err(tr("このページにはコピーできる文章がありません。", "This page has no text that can be copied.").into()); }
        write_text(&format!("{} — p. {}\n\n{}", state.title, state.current, text.trim()))
    }
    pub fn begin_capture(&self) {
        let mut state = self.state.borrow_mut(); state.cancel_scroll();state.edge_press=None;state.set_edge_hint(None);state.finish_intended_turn();
        if state.document.is_none() { return; }
        state.visual.borrow_mut().capture_mode = true; state.capture_start = None; state.pan = None; state.space = false;
        NSCursor::crosshairCursor().set(); state.overlay.repaint();
    }
    pub fn take_capture(&self) -> Option<Retained<NSImage>> { self.state.borrow_mut().captured.take() }
    pub fn cancel_interaction(&self) -> bool {
        let mut state = self.state.borrow_mut();
        if state.scroll.is_some() { state.cancel_scroll(); return true; }
        if state.edge_press.take().is_some() {state.set_edge_hint(None);state.update_cursor();return true;}
        if state.turn.is_some() { state.settle_turn(false); return true; }
        if state.visual.borrow().capture_mode { state.cancel_capture(); return true; }
        let active = state.pan.is_some() || state.space; state.pan = None; state.space = false; state.update_cursor(); active
    }
    pub fn tick(&self) {
        let mut state = self.state.borrow_mut();
        if !state.enabled {return;}
        #[cfg(debug_assertions)] animation::debug_tick(&mut state.visual.borrow_mut());
        state.advance_turn();
        if state.turn.is_none() {
            let actual: Option<Retained<PDFPage>> = unsafe { msg_send![&*state.view, currentPage] };
            if let (Some(document), Some(page)) = (&state.document, actual) {
                let page = document.index_of(&page) + 1;
                if page <= document.page_count() && !same_leaf(state.current, page, state.spread) { state.current = page; state.fit(); crate::enqueue("reader-page", page); }
            }
        }
        let scale: f64 = unsafe { msg_send![&*state.view, scaleFactor] };
        if (scale - state.last_scale).abs() > 0.0001 { state.last_scale = scale; state.factor = (scale / state.fit_scale.max(0.000001)).max(1.); crate::enqueue("reader-zoom", (state.factor * 100.).round() as usize); }
    }
}

impl Drop for Reader { fn drop(&mut self) { if let Some(monitor) = &self.monitor { unsafe { NSEvent::removeMonitor(monitor); } } } }

impl ReaderState {
    fn fit(&mut self) {
        if self.document.is_none() { return; }
        unsafe {
            let _: () = msg_send![&*self.view, setAutoScales: true];
            let _: () = msg_send![&*self.view, layoutDocumentView];
            let scale: f64 = msg_send![&*self.view, scaleFactorForSizeToFit];
            if scale.is_finite() && scale > 0. { self.fit_scale = scale; let _: () = msg_send![&*self.view, setScaleFactor: scale]; }
        }
        self.factor = 1.; self.pan = None; self.space = false;
        crate::enqueue("reader-zoom", 100);
    }
    fn go_to(&mut self, target: usize) {
        let Some(document) = &self.document else { return; };
        let target = target.clamp(1, document.page_count());
        if let Some(page) = document.page(target - 1) {
            unsafe { let _: () = msg_send![&*self.view, goToPage: &*page]; }
            self.current = target; self.fit(); self.update_cursor();
            crate::enqueue("reader-page", target);
        }
    }
    fn step(&mut self, delta: isize) {
        self.cancel_scroll();self.edge_press=None;self.set_edge_hint(None);self.finish_intended_turn();
        let target = step_target(self.current, self.document.as_ref().map(|doc|doc.page_count()).unwrap_or(1), self.spread, delta);
        self.navigate(target, true);
    }
    fn set_zoom(&mut self, factor: f64) {
        self.cancel_scroll();self.edge_press=None;self.set_edge_hint(None);self.finish_intended_turn(); self.pan = None;
        if self.document.is_none() { return; }
        self.factor = factor.clamp(1., 4.);
        if self.factor <= 1.001 { self.fit(); self.update_cursor(); return; }
        unsafe { let _: () = msg_send![&*self.view, setAutoScales: false]; let _: () = msg_send![&*self.view, setScaleFactor: self.fit_scale * self.factor]; }
        self.update_cursor();
        crate::enqueue("reader-zoom", (self.factor * 100.).round() as usize);
    }
    fn scroll_view(&self) -> Option<Retained<NSScrollView>> {
        unsafe { let document: Option<Retained<NSView>> = msg_send![&*self.view, documentView]; document.and_then(|view| view.enclosingScrollView()) }
    }
}

fn write_text(text: &str) -> Result<(), String> {
    if text.len() > 8 * 1024 * 1024 { return Err(tr("コピーする文章が長すぎます。", "The selected text is too long to copy.").into()); }
    let pasteboard = NSPasteboard::generalPasteboard(); pasteboard.clearContents();
    if pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }) { Ok(()) } else { Err(tr("コピーできませんでした。もう一度お試しください。", "Copying failed. Please try again.").into()) }
}

fn handle_event(state: &Rc<RefCell<ReaderState>>, event: &NSEvent) -> bool {
    let Ok(mut state) = state.try_borrow_mut() else { return false; };
    if state.view.isHiddenOrHasHiddenAncestor() {return false;}
    let Some(window) = state.view.window() else { return false; };
    if event.window(state.view.mtm()).as_deref() != Some(&*window) { return false; }
    let point = state.view.convertPoint_fromView(event.locationInWindow(), None);
    let bounds = state.view.bounds();
    let geometric_inside = point.x >= 0. && point.y >= 0. && point.x <= bounds.size.width && point.y <= bounds.size.height;
    let reader_hit = window.contentView().and_then(|content| { let point = content.convertPoint_fromView(event.locationInWindow(), None); content.hitTest(point) }).is_some_and(|hit|hit.isDescendantOf(&state.view));
    let inside = geometric_inside && reader_hit;
    let event_type = event.r#type();
    let overlay_point = NSPoint::new(point.x, if state.view.isFlipped() { point.y } else { bounds.size.height - point.y });
    let responder = window.firstResponder();
    let reader_focused = responder.as_ref().and_then(|responder| responder.downcast_ref::<NSView>()).is_some_and(|view| view.isDescendantOf(&state.view));
    if !state.enabled {
        let command=event.modifierFlags().contains(NSEventModifierFlags::Command);
        let paging=event_type==NSEventType::KeyDown && reader_focused && if command {matches!(event.keyCode(),24|27|29)}else{matches!(event.keyCode(),49|115|116|119|121|123|124)};
        return paging || inside && (event_type==NSEventType::ScrollWheel || event_type==NSEventType::Magnify || event_type==NSEventType::Swipe);
    }
    if event_type == NSEventType::LeftMouseDown && inside { state.cancel_scroll();crate::enqueue("reader-click", 0); }
    if event_type == NSEventType::MouseMoved || event_type == NSEventType::CursorUpdate || event_type == NSEventType::MouseExited {
        if !state.space && !state.visual.borrow().capture_mode && state.edge_press.is_none() {
            let hint=if inside {state.edge_at(overlay_point).map(|(side,rect)|EdgeHint{side,rect,pressed:false})}else{None};
            state.set_edge_hint(hint);
            if hint.is_some() {state.update_cursor();return true;}
        }
    }
    if event_type == NSEventType::KeyUp && event.keyCode() == 49 { state.space = false; state.pan = None; state.update_cursor(); return false; }
    if inside && (event_type == NSEventType::MouseMoved || event_type == NSEventType::CursorUpdate) && (state.space || state.visual.borrow().capture_mode) { state.update_cursor(); return true; }
    if event_type == NSEventType::KeyDown && reader_focused {
        let modifiers = event.modifierFlags();
        let text = event.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
        if modifiers.contains(NSEventModifierFlags::Command) {
            if event.keyCode() == 24 { let factor = state.factor * 1.25; state.set_zoom(factor); return true; }
            if event.keyCode() == 27 { let factor = state.factor / 1.25; state.set_zoom(factor); return true; }
            if event.keyCode() == 29 { state.finish_intended_turn(); state.fit(); state.update_cursor(); return true; }
            match text.as_str() { "+" | "=" => { let factor = state.factor * 1.25; state.set_zoom(factor); return true; }, "-" => { let factor = state.factor / 1.25; state.set_zoom(factor); return true; }, "0" => { state.finish_intended_turn(); state.fit(); state.update_cursor(); return true; }, _ => return false }
        }
        if modifiers.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option) { return false; }
        match event.keyCode() {
            49 if state.factor > 1.001 => { state.space = true; state.update_cursor(); return true; },
            49 | 124 | 121 => { let delta = if event.keyCode() == 124 && state.rtl || event.keyCode() == 49 && modifiers.contains(NSEventModifierFlags::Shift) { -1 } else { 1 }; state.step(delta); return true; },
            123 | 116 => { let delta = if event.keyCode() == 123 && state.rtl { 1 } else { -1 }; state.step(delta); return true; },
            115 => { state.go_to(1); return true; },
            119 => { let end = state.document.as_ref().map(|d| d.page_count()).unwrap_or(1); state.go_to(end); return true; },
            _ => {}
        }
    }
    if state.visual.borrow().capture_mode {
        if inside { NSCursor::crosshairCursor().set(); }
        if event_type == NSEventType::LeftMouseDown && inside { state.capture_start = Some(overlay_point); return true; }
        if event_type == NSEventType::LeftMouseDragged && state.capture_start.is_some() { state.update_capture(overlay_point); return true; }
        if event_type == NSEventType::LeftMouseUp && state.capture_start.is_some() { state.update_capture(overlay_point); state.complete_capture(); return true; }
    }
    if event_type == NSEventType::ScrollWheel && (inside || state.scroll.is_some()) && state.factor<=1.001 {
        if !inside {state.cancel_scroll();return false;}
        return state.trackpad_event(event,overlay_point);
    }
    if event_type == NSEventType::Swipe && inside && state.factor<=1.001 {
        if state.scroll.is_none() && state.last_scroll.is_none_or(|last|last.elapsed().as_secs_f64()>0.7) && event.deltaX().abs()>event.deltaY().abs() {
            let side=if event.deltaX()>0. {1}else{-1};let delta=if (side>0)!=state.rtl {1}else{-1};state.step(delta);
        }
        return true;
    }
    if event_type == NSEventType::LeftMouseDown && inside && !state.space && state.factor <= 1.001 && !state.page_content_at(overlay_point) {
        if state.corner_turn(overlay_point) {return true;}
        if let Some((side,rect))=state.edge_at(overlay_point) {state.edge_press=Some(EdgePress{side,start:overlay_point,distance:0.});state.set_edge_hint(Some(EdgeHint{side,rect,pressed:true}));state.update_cursor();return true;}
    }
    if event_type == NSEventType::LeftMouseDragged && state.edge_press.is_some() {
        let mut press=state.edge_press.unwrap();press.distance=press.distance.max((overlay_point.x-press.start.x).hypot(overlay_point.y-press.start.y));state.edge_press=Some(press);
        let hint=if inside {state.edge_at(overlay_point).filter(|(side,_)|*side==press.side).map(|(side,rect)|EdgeHint{side,rect,pressed:press.distance<5.})}else{None};state.set_edge_hint(hint);return true;
    }
    if event_type == NSEventType::LeftMouseUp && state.edge_press.is_some() {
        let press=state.edge_press.take().unwrap();let zone=if inside {state.edge_at(overlay_point)}else{None};
        let commit=edge_click_commits(press,overlay_point,zone.map(|(side,_)|side));state.set_edge_hint(None);state.update_cursor();
        if commit {let delta=if (press.side>0)!=state.rtl {1}else{-1};state.step(delta);}return true;
    }
    if event_type == NSEventType::LeftMouseDragged && state.scroll.is_none() && state.turn.as_ref().is_some_and(|turn|turn.settle.is_none()) { state.drag_turn(overlay_point); return true; }
    if event_type == NSEventType::LeftMouseUp && state.scroll.is_none() && state.turn.as_ref().is_some_and(|turn|turn.settle.is_none()) {
        let progress = state.visual.borrow().curl.as_ref().map(|curl|curl.progress).unwrap_or(0.);
        let turn = state.turn.as_ref().unwrap();
        let speed = if turn.sample.elapsed().as_secs_f64() < 0.12 { turn.velocity } else { 0. };
        let commit = should_commit(progress, turn.excursion, speed);
        state.settle_turn(commit); return true;
    }
    if event_type == NSEventType::Magnify && inside { let factor = state.factor * (1. + event.magnification()); state.set_zoom(factor); return true; }
    if event_type == NSEventType::LeftMouseDown && inside && state.space && state.factor > 1. {
        if let Some(scroll) = state.scroll_view() { let clip=scroll.contentView(); state.pan = Some((event.locationInWindow(), clip.bounds().origin)); state.update_cursor(); return true; }
    }
    if event_type == NSEventType::LeftMouseDragged {
        if let Some((start, origin)) = state.pan { if let Some(scroll) = state.scroll_view() { let clip = scroll.contentView(); let position=event.locationInWindow(); let next = NSPoint::new(origin.x - position.x + start.x, origin.y + (position.y - start.y) * if clip.isFlipped() { 1. } else { -1. }); let next = clip.constrainBoundsRect(NSRect::new(next, clip.bounds().size)).origin; clip.scrollToPoint(next); scroll.reflectScrolledClipView(&clip); } return true; }
    }
    if event_type == NSEventType::LeftMouseUp && state.pan.take().is_some() { state.update_cursor(); return true; }
    if event_type == NSEventType::ScrollWheel && inside && state.factor <= 1.001 { return true; }
    false
}

impl ReaderState {
    fn page_rect(&self, page: &PDFPage) -> NSRect {
        let rect: NSRect = unsafe { msg_send![&*self.view, convertRect: page.bounds(), fromPage: page] };
        NSRect::new(NSPoint::new(rect.origin.x, if self.view.isFlipped() { rect.origin.y } else { self.view.bounds().size.height - rect.origin.y - rect.size.height }), rect.size)
    }
    fn visible(&self) -> Vec<(usize, Retained<PDFPage>, NSRect)> {
        let Some(document) = &self.document else { return Vec::new(); };
        let pages: Retained<NSArray<PDFPage>> = unsafe { msg_send![&*self.view, visiblePages] };
        pages.iter().take(2).map(|page| (document.index_of(&page) + 1, page.clone(), self.page_rect(&page))).collect()
    }
    fn snapshot(&self, page: &PDFPage, rect: NSRect) -> PageImage {
        let scale = (1_800_000. / (rect.size.width * rect.size.height).max(1.)).sqrt().min(1.5);
        PageImage { image: page.thumbnail(NSSize::new(rect.size.width * scale, rect.size.height * scale)), rect }
    }
    fn navigate(&mut self, target: usize, animated: bool) {
        self.cancel_scroll();self.edge_press=None;self.set_edge_hint(None);self.finish_intended_turn(); self.cancel_capture();
        let count = self.document.as_ref().map(|doc|doc.page_count()).unwrap_or(0); if count == 0 { return; }
        let target = target.clamp(1,count); if target == self.current { self.go_to(target); return; }
        let reduce: bool = unsafe { msg_send![&*NSWorkspace::sharedWorkspace(), accessibilityDisplayShouldReduceMotion] };
        if !animated || reduce || self.factor > 1.001 || same_leaf(self.current,target,self.spread) || !self.begin_turn(target, 0.88, NSPoint::new(0.,0.)) { self.go_to(target); return; }
        self.settle_turn(true);
    }
    fn begin_turn(&mut self, target: usize, corner: f64, start: NSPoint) -> bool {
        let Some(document) = self.document.clone() else { return false; };
        if target == self.current || target == 0 || target > document.page_count() { return false; }
        self.set_edge_hint(None);
        let direction = if (target > self.current) != self.rtl { 1. } else { -1. };
        let mut old = self.visible(); old.sort_by(|a,b|a.2.origin.x.total_cmp(&b.2.origin.x));
        if old.is_empty() { return false; }
        let moving = old.remove(if direction > 0. { old.len()-1 } else {0});
        if moving.2.size.width < 2. || moving.2.size.height < 2. { return false; }
        #[cfg(debug_assertions)]
        animation::debug_begin(&mut self.visual.borrow_mut());
        let front = self.snapshot(&moving.1,moving.2);
        #[cfg(debug_assertions)] animation::debug_phase(&mut self.visual.borrow_mut(), "front_snapshot");
        let companion = old.first().map(|(_,page,rect)|self.snapshot(page,*rect));
        #[cfg(debug_assertions)] animation::debug_phase(&mut self.visual.borrow_mut(), "companion_snapshot");
        let gutter = companion.as_ref().map(|other| (front.rect.origin.x-other.rect.origin.x).abs()-front.rect.size.width).unwrap_or(0.).max(0.);
        let old_page = self.current;
        self.visual.borrow_mut().curl = Some(Curl { front, companion, back: None, direction, corner, progress: 0., vertical: 0., spread: self.spread, gutter });
        self.overlay.repaint();
        if let Some(page)=document.page(target-1) { unsafe { let _:()=msg_send![&*self.view,goToPage:&*page]; } }
        self.fit();
        #[cfg(debug_assertions)] animation::debug_phase(&mut self.visual.borrow_mut(), "destination_layout");
        let mut incoming=self.visible();incoming.sort_by(|a,b|a.2.origin.x.total_cmp(&b.2.origin.x));
        if self.spread && incoming.len() > 1 {
            let reverse=&incoming[if direction > 0. {0}else{incoming.len()-1}];
            let image=self.snapshot(&reverse.1,reverse.2).image;
            self.visual.borrow_mut().curl.as_mut().unwrap().back=Some(image);
        }
        #[cfg(debug_assertions)] animation::debug_phase(&mut self.visual.borrow_mut(), "back_snapshot");
        self.view.displayIfNeeded();
        #[cfg(debug_assertions)] animation::debug_phase(&mut self.visual.borrow_mut(), "destination_display");
        #[cfg(debug_assertions)] { self.debug_samples = 0; animation::debug_prepared(&mut self.visual.borrow_mut()); }
        self.turn=Some(Turn {old:old_page,target,settle:None,start,last:start,sample:Instant::now(),velocity:0.,excursion:0.});
        self.overlay.repaint(); true
    }
    fn corner_turn(&mut self, point: NSPoint) -> bool {
        if self.turn.is_some() { self.finish_intended_turn(); }
        let mut pages=self.visible();pages.sort_by(|a,b|a.2.origin.x.total_cmp(&b.2.origin.x));
        let Some(first)=pages.first() else{return false;};let last=pages.last().unwrap();
        let left=(point.x-first.2.origin.x).abs()<38.;let right=(point.x-last.2.origin.x-last.2.size.width).abs()<38.;
        let rect=if left{first.2}else{last.2};
        let top=(point.y-rect.origin.y).abs()<44.;let bottom=(point.y-rect.origin.y-rect.size.height).abs()<44.;
        if !(left||right)||!(top||bottom){return false;}
        let forward=right!=self.rtl;
        let target=step_target(self.current,self.document.as_ref().map(|doc|doc.page_count()).unwrap_or(1),self.spread,if forward{1}else{-1});
        let reduce:bool=unsafe{msg_send![&*NSWorkspace::sharedWorkspace(),accessibilityDisplayShouldReduceMotion]};
        if reduce { self.navigate(target,false); return true; }
        NSCursor::closedHandCursor().set();
        if !self.begin_turn(target,if top{0.1}else{0.9},point){self.update_cursor();return false;}
        self.visual.borrow_mut().curl.as_mut().unwrap().progress=0.015;self.overlay.repaint();true
    }
    fn drag_turn(&mut self, point: NSPoint) {
        let Some(turn)=self.turn.as_mut() else{return;};let mut visual=self.visual.borrow_mut();let Some(curl)=visual.curl.as_mut()else{return;};
        let distance=(turn.start.x-point.x)*curl.direction;
        let elapsed=turn.sample.elapsed().as_secs_f64().max(0.001);
        turn.velocity=(turn.last.x-point.x)*curl.direction/curl.front.rect.size.width/elapsed;
        turn.sample=Instant::now();turn.last=point;turn.excursion=turn.excursion.max((point.x-turn.start.x).hypot(point.y-turn.start.y));
        curl.progress=(distance/curl.front.rect.size.width).clamp(0.,1.);curl.vertical=point.y-turn.start.y;self.overlay.repaint();
    }
    fn settle_turn(&mut self, commit: bool) {
        let Some(turn)=self.turn.as_mut()else{return;};let visual=self.visual.borrow();let Some(curl)=visual.curl.as_ref()else{return;};
        let target=if commit{1.}else{0.};let travel=target-curl.progress;
        let duration=(travel.abs()*0.5/(1.+turn.velocity.abs()*0.12)).clamp(0.16,0.54);
        turn.settle=Some(Settle{start:Instant::now(),duration:Duration::from_secs_f64(duration),from:curl.progress,to:target,vertical:curl.vertical,tangent:if travel.abs()<0.001{0.}else{(turn.velocity*duration/travel).clamp(0.,2.4)}});
        drop(visual); self.update_cursor();
    }
    fn advance_turn(&mut self) {
        let Some(turn)=self.turn.as_ref()else{return;};let Some(settle)=turn.settle.as_ref()else{self.overlay.displayIfNeeded();return;};
        let t=(settle.start.elapsed().as_secs_f64()/settle.duration.as_secs_f64()).clamp(0.,1.);let smooth=t*t*(3.-2.*t);let eased=smooth+settle.tangent*t*(1.-t)*(1.-t);
        {let mut visual=self.visual.borrow_mut();if let Some(curl)=visual.curl.as_mut(){curl.progress=settle.from+(settle.to-settle.from)*eased;curl.vertical=settle.vertical*(1.-smooth);}}
        let commit=settle.to>0.5;
        #[cfg(debug_assertions)]
        if commit && animation::debug_pngs_enabled() { if let Some(directory)=std::env::var_os("SUMI_CAPTURE_TURNS") {
            for (index,threshold) in [0.25,0.5,0.75].into_iter().enumerate() {
                let mask=1<<index;
                if self.debug_samples & mask == 0 && self.visual.borrow().curl.as_ref().is_some_and(|curl|curl.progress>=threshold) {
                    self.debug_samples |= mask;
                    animation::debug_frame(&self.view, &self.visual.borrow(), &std::path::PathBuf::from(&directory).join(format!("turn-{}-{}.png", self.current,index+1)));
                }
            }
        } }
        if t>=1. {self.finish_turn(commit);}else{self.overlay.repaint();self.overlay.displayIfNeeded();}
    }
    fn finish_intended_turn(&mut self) {
        let commit=self.turn.as_ref().and_then(|turn|turn.settle.as_ref()).map(|settle|settle.to>0.5).unwrap_or(true);
        self.finish_turn(commit);
    }
    fn finish_turn(&mut self, commit: bool) {
        let Some(turn)=self.turn.take()else{return;};
        #[cfg(debug_assertions)] let endpoint = Instant::now();
        let target=if commit{turn.target}else{turn.old};
        if !commit {if let Some(page)=self.document.as_ref().and_then(|doc|doc.page(target-1)){unsafe{let _:()=msg_send![&*self.view,goToPage:&*page];}self.fit();}}
        self.view.displayIfNeeded();
        #[cfg(debug_assertions)] animation::debug_finish(&mut self.visual.borrow_mut(), turn.old, turn.target, commit, endpoint.elapsed().as_secs_f64()*1000.);
        self.current=target;self.visual.borrow_mut().curl=None;self.update_cursor();self.overlay.repaint();crate::enqueue("reader-page",target);
    }
    fn update_cursor(&self) {
        if self.visual.borrow().capture_mode { NSCursor::crosshairCursor().set(); }
        else if self.pan.is_some() || self.turn.as_ref().is_some_and(|turn|turn.settle.is_none()) { NSCursor::closedHandCursor().set(); }
        else if self.space && self.factor > 1.001 { NSCursor::openHandCursor().set(); }
        else if self.visual.borrow().edge.is_some() {NSCursor::pointingHandCursor().set();}
        else { NSCursor::arrowCursor().set(); }
    }
    fn cancel_capture(&mut self) {
        let mut visual=self.visual.borrow_mut();if visual.capture_mode {NSCursor::arrowCursor().set();}
        visual.capture_mode=false;visual.capture=None;self.capture_start=None;self.overlay.repaint();
    }
    fn update_capture(&mut self, point: NSPoint) {
        let Some(start)=self.capture_start else{return;};let bounds=self.view.bounds();let x=point.x.clamp(0.,bounds.size.width);let y=point.y.clamp(0.,bounds.size.height);
        self.visual.borrow_mut().capture=Some(NSRect::new(NSPoint::new(start.x.min(x),start.y.min(y)),NSSize::new((start.x-x).abs(),(start.y-y).abs())));self.overlay.repaint();
    }
    fn complete_capture(&mut self) {
        let rect=self.visual.borrow().capture;self.cancel_capture();let Some(rect)=rect else{return;};if rect.size.width<4.||rect.size.height<4.{return;}
        let scale=(6_000_000./(rect.size.width*rect.size.height)).sqrt().min(2.);
        let width=(rect.size.width*scale).floor().max(1.)as isize;let height=(rect.size.height*scale).floor().max(1.)as isize;
        let bitmap:Option<Retained<AnyObject>>=unsafe { let Some(class)=AnyClass::get(c"NSBitmapImageRep") else{return;}; let allocated:Allocated<AnyObject>=msg_send![class,alloc]; msg_send![allocated,initWithBitmapDataPlanes:std::ptr::null_mut::<*mut u8>(),pixelsWide:width,pixelsHigh:height,bitsPerSample:8isize,samplesPerPixel:4isize,hasAlpha:true,isPlanar:false,colorSpaceName:NSDeviceRGBColorSpace,bytesPerRow:0isize,bitsPerPixel:32isize] };
        let Some(bitmap)=bitmap else{return;};let context:Option<Retained<NSGraphicsContext>>=unsafe{msg_send![NSGraphicsContext::class(),graphicsContextWithBitmapImageRep:&*bitmap]};let Some(context)=context else{return;};
        NSGraphicsContext::saveGraphicsState_class();NSGraphicsContext::setCurrentContext(Some(&context));let cg=context.CGContext();
        CGContext::scale_ctm(Some(&cg),scale,scale);
        let source=NSRect::new(NSPoint::new(rect.origin.x,if self.view.isFlipped(){rect.origin.y}else{self.view.bounds().size.height-rect.origin.y-rect.size.height}),rect.size);
        CGContext::translate_ctm(Some(&cg),-source.origin.x,-source.origin.y);
        self.view.displayRectIgnoringOpacity_inContext(source,&context);NSGraphicsContext::restoreGraphicsState_class();
        let image=NSImage::initWithSize(NSImage::alloc(),rect.size);unsafe{let _:()=msg_send![&*image,addRepresentation:&*bitmap];}
        let pasteboard=NSPasteboard::generalPasteboard();pasteboard.clearContents();let objects=NSArray::from_slice(&[&*image]);
        let copied:bool=unsafe{msg_send![&*pasteboard,writeObjects:&*objects]};
        self.captured=Some(image);if copied{crate::enqueue("capture-ready",self.current);}else{crate::enqueue("capture-failed",self.current);}
    }
}

fn same_leaf(a: usize, b: usize, spread: bool) -> bool { a == b || spread && a > 1 && b > 1 && a / 2 == b / 2 }
fn step_target(current: usize, count: usize, spread: bool, delta: isize) -> usize {
    let target = if spread { if delta > 0 { if current == 1 { 2 } else { current - current % 2 + 2 } } else if current <= 3 { 1 } else { current - current % 2 - 2 } } else { current.saturating_add_signed(delta.signum()).max(1) };
    if target > count { current } else { target.max(1) }
}
fn should_commit(progress: f64, excursion: f64, velocity: f64) -> bool { excursion < 5. || progress > 0.32 || progress >= 0.08 && velocity > 0.8 }

#[cfg(test)]
mod tests {
    use super::{same_leaf, step_target, should_commit};
    #[test]
    fn cover_and_companion_navigation_preserve_the_selected_page() {
        assert_eq!(step_target(1, 9, true, 1), 2);
        assert_eq!(step_target(3, 9, true, 1), 4);
        assert_eq!(step_target(3, 9, true, -1), 1);
        assert_eq!(step_target(8, 9, true, 1), 8);
        assert_eq!(step_target(9, 9, true, 1), 9);
        assert_eq!(step_target(115, 325, false, 1), 116);
        assert!(same_leaf(114, 115, true));
        assert!(!same_leaf(114, 115, false));
        assert!(!same_leaf(1, 2, true));
        assert!(!same_leaf(115, 116, true));
    }
    #[test]
    fn returning_a_drag_to_its_origin_cancels_while_a_corner_click_turns() {
        assert!(!should_commit(0.02, 160., -0.3));
        assert!(!should_commit(0.05, 160., 0.));
        assert!(!should_commit(0.025, 12., 1.2));
        assert!(should_commit(0.01, 2., 0.));
        assert!(should_commit(0.7, 230., -0.2));
        assert!(should_commit(0.2, 80., 1.1));
    }
}

impl ReaderState {
    fn set_edge_hint(&mut self, hint: Option<EdgeHint>) {
        let mut visual=self.visual.borrow_mut();if visual.edge!=hint {visual.edge=hint;self.overlay.repaint();}
    }
    fn page_content_at(&self, point: NSPoint) -> bool {
        let point=NSPoint::new(point.x,if self.view.isFlipped(){point.y}else{self.view.bounds().size.height-point.y});
        unsafe {
            let page:Option<Retained<PDFPage>>=msg_send![&*self.view,pageForPoint:point,nearest:false];let Some(page)=page else{return false;};
            let point:NSPoint=msg_send![&*self.view,convertPoint:point,toPage:&*page];
            let annotation:Option<Retained<AnyObject>>=msg_send![&*page,annotationAtPoint:point];if annotation.is_some(){return true;}
            let character:isize=msg_send![&*page,characterIndexAtPoint:point];let count:usize=msg_send![&*page,numberOfCharacters];character>=0 && (character as usize)<count
        }
    }
    fn edge_at(&self, point: NSPoint) -> Option<(i8,NSRect)> {
        if self.factor>1.001 || self.turn.is_some() || self.scroll.is_some() || self.visual.borrow().capture_mode {return None;}
        let mut pages=self.visible();pages.sort_by(|a,b|a.2.origin.x.total_cmp(&b.2.origin.x));let first=pages.first()?.2;let last=pages.last()?.2;
        let side=edge_side(point,self.view.bounds().size.width,first,last)?;
        let delta=if (side>0)!=self.rtl{1}else{-1};let count=self.document.as_ref()?.page_count();
        if step_target(self.current,count,self.spread,delta)==self.current || self.page_content_at(point){return None;}
        let rect=if side<0 {NSRect::new(NSPoint::new(0.,first.origin.y),NSSize::new((first.origin.x+edge_band(first)).max(28.),first.size.height))}else{let x=(last.origin.x+last.size.width-edge_band(last)).max(0.);NSRect::new(NSPoint::new(x,last.origin.y),NSSize::new((self.view.bounds().size.width-x).max(28.),last.size.height))};
        Some((side,rect))
    }
    fn cancel_scroll(&mut self) {
        if let Some(scroll)=self.scroll.take() {if scroll.animated {self.finish_turn(false);}if scroll.gesture.direction.is_some(){self.last_scroll=Some(Instant::now());}self.set_edge_hint(None);}
    }
    fn trackpad_event(&mut self, event: &NSEvent, point: NSPoint) -> bool {
        self.trackpad_sample(event.scrollingDeltaX(),event.scrollingDeltaY(),event.phase(),event.momentumPhase()!=NSEventPhase::None,event.timestamp(),point)
    }
    fn trackpad_sample(&mut self, dx: f64, dy: f64, phase: NSEventPhase, momentum: bool, time: f64, point: NSPoint) -> bool {
        if momentum {self.last_scroll=Some(Instant::now());return true;}
        if phase.intersects(NSEventPhase::Began|NSEventPhase::MayBegin) {
            self.cancel_scroll();self.finish_intended_turn();self.edge_press=None;self.set_edge_hint(None);
            let width=self.visible().first().map(|(_,_,rect)|rect.size.width).unwrap_or(self.view.bounds().size.width);
            self.scroll=Some(ScrollSession{gesture:PagingGesture::new(width,time),target:None,animated:false});
        }
        let Some(mut scroll)=self.scroll.take()else{return true;};
        if let Some(side)=scroll.gesture.update(-dx,dy,time,false) {
            let delta=if (side>0)!=self.rtl{1}else{-1};let target=step_target(self.current,self.document.as_ref().map(|doc|doc.page_count()).unwrap_or(1),self.spread,delta);
            if target!=self.current {
                scroll.target=Some(target);
                let reduce:bool=unsafe{msg_send![&*NSWorkspace::sharedWorkspace(),accessibilityDisplayShouldReduceMotion]};
                if !reduce {scroll.animated=self.begin_turn(target,if point.y<self.view.bounds().size.height/2.{0.18}else{0.82},point);}
            }
        }
        if scroll.gesture.direction.is_some(){self.last_scroll=Some(Instant::now());}
        if scroll.animated {
            {let mut visual=self.visual.borrow_mut();if let Some(curl)=visual.curl.as_mut(){curl.progress=scroll.gesture.progress();curl.vertical=scroll.gesture.y.clamp(-curl.front.rect.size.height*0.3,curl.front.rect.size.height*0.3);}}
            if let Some(turn)=self.turn.as_mut(){turn.velocity=scroll.gesture.speed(time);turn.sample=Instant::now();}self.overlay.repaint();
        }
        if phase.intersects(NSEventPhase::Ended|NSEventPhase::Cancelled) {
            let commit=scroll.gesture.finish(phase.contains(NSEventPhase::Cancelled),time);
            if scroll.animated {self.settle_turn(commit);}else if commit {if let Some(target)=scroll.target{self.go_to(target);}}
        } else {self.scroll=Some(scroll);}
        true
    }
}
fn edge_band(rect:NSRect)->f64 {(rect.size.width*0.035).clamp(10.,16.)}
fn edge_side(point:NSPoint,width:f64,left:NSRect,right:NSRect)->Option<i8> {
    if point.x<0.||point.x>width{return None;}
    if point.x<=left.origin.x+edge_band(left) && point.y>=left.origin.y && point.y<=left.origin.y+left.size.height {return Some(-1);}
    if point.x>=right.origin.x+right.size.width-edge_band(right) && point.y>=right.origin.y && point.y<=right.origin.y+right.size.height {return Some(1);}
    None
}
fn edge_click_commits(press:EdgePress,point:NSPoint,side:Option<i8>)->bool {side==Some(press.side)&&press.distance<5.&&(point.x-press.start.x).hypot(point.y-press.start.y)<5.}

#[cfg(test)]
mod input_tests {
    use super::{PagingGesture,EdgePress,edge_side,edge_click_commits,allowed_pdf_link};
    use objc2_foundation::{NSPoint,NSRect,NSSize,NSString,NSURL};
    #[test]
    fn pdf_links_cannot_launch_files_or_custom_application_handlers() {
        for value in ["https://example.com/book#chapter", "HTTP://example.com/", "mailto:reader@example.com"] {
            let url=NSURL::URLWithString(&NSString::from_str(value)).unwrap();
            assert!(allowed_pdf_link(&url), "{value}");
        }
        for value in ["file:///Applications/Calculator.app", "smb://server/share", "javascript:alert(1)", "x-apple.systempreferences:com.apple.preference.security", "sumi://open?path=/tmp/book.pdf", "../other.pdf", "https:/missing-host"] {
            let url=NSURL::URLWithString(&NSString::from_str(value)).unwrap();
            assert!(!allowed_pdf_link(&url), "{value}");
        }
    }
    #[test]
    fn horizontal_trackpad_gesture_can_return_to_its_origin_and_cancel() {
        let mut gesture=PagingGesture::new(500.,0.);
        assert_eq!(gesture.update(30.,2.,0.02,false),Some(1));
        gesture.update(170.,4.,0.08,false);assert!((gesture.progress()-0.4).abs()<0.001);
        gesture.update(-185.,0.,0.14,false);assert!(gesture.progress()<0.04);
        assert!(!gesture.finish(false,0.15));
    }
    #[test]
    fn trackpad_commit_is_once_and_momentum_cannot_advance_another_page() {
        let mut gesture=PagingGesture::new(500.,0.);
        gesture.update(150.,4.,0.1,false);assert!(gesture.finish(false,0.12));
        gesture.update(800.,0.,0.14,true);assert!(!gesture.finish(false,0.16));
        let mut noise=PagingGesture::new(500.,0.);noise.update(900.,0.,0.1,true);assert!(!noise.finish(false,0.12));
    }
    #[test]
    fn vertical_scroll_and_short_horizontal_flicks_do_not_turn() {
        let mut vertical=PagingGesture::new(500.,0.);vertical.update(2.,40.,0.02,false);vertical.update(250.,3.,0.1,false);assert!(!vertical.finish(false,0.11));
        let mut tiny=PagingGesture::new(500.,0.);tiny.update(13.,0.,0.008,false);assert!(!tiny.finish(false,0.015));
        let mut cancelled=PagingGesture::new(500.,0.);cancelled.update(280.,0.,0.1,false);assert!(!cancelled.finish(true,0.12));
    }
    #[test]
    fn page_center_stays_native_and_edge_click_requires_a_release_in_the_same_zone() {
        let page=NSRect::new(NSPoint::new(220.,20.),NSSize::new(500.,700.));
        assert_eq!(edge_side(NSPoint::new(80.,300.),940.,page,page),Some(-1));
        assert_eq!(edge_side(NSPoint::new(710.,300.),940.,page,page),Some(1));
        assert_eq!(edge_side(NSPoint::new(470.,300.),940.,page,page),None);
        assert_eq!(edge_side(NSPoint::new(80.,5.),940.,page,page),None);
        let press=EdgePress{side:1,start:NSPoint::new(800.,300.),distance:0.};
        assert!(edge_click_commits(press,NSPoint::new(802.,301.),Some(1)));
        assert!(!edge_click_commits(press,NSPoint::new(802.,301.),None));
        assert!(!edge_click_commits(EdgePress{distance:30.,..press},press.start,Some(1)));
    }
}
