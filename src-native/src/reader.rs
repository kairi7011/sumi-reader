use std::{cell::RefCell, path::{Path, PathBuf}, ptr::NonNull, rc::Rc};
use block2::RcBlock;
use objc2::{msg_send, ClassType, MainThreadMarker, MainThreadOnly};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSAutoresizingMaskOptions, NSColor, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSImage, NSPasteboard, NSPasteboardTypeString, NSScrollView, NSView};
use objc2_foundation::{NSPoint, NSRange, NSRect, NSSize, NSString};
use crate::pdf::{PDFDocument, PDFPage, PDFSelection, PDFView, CROP_BOX};

#[derive(Clone, Debug)]
pub struct DocumentInfo {
    pub title: String,
    pub page_count: usize,
    pub path: PathBuf,
    pub allows_copying: bool,
}

struct ReaderState {
    view: Retained<PDFView>,
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
}

pub struct Reader {
    container: Retained<NSView>,
    state: Rc<RefCell<ReaderState>>,
    monitor: Option<Retained<AnyObject>>,
}

impl Reader {
    pub fn new(mtm: MainThreadMarker, frame: NSRect) -> Self {
        let container = NSView::initWithFrame(NSView::alloc(mtm), frame);
        let view: Retained<PDFView> = unsafe {
            let allocated: Allocated<PDFView> = msg_send![PDFView::class(), alloc];
            msg_send![allocated, initWithFrame: NSRect::new(NSPoint::new(0., 0.), frame.size)]
        };
        unsafe {
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
        let state = Rc::new(RefCell::new(ReaderState { view, document: None, title: String::new(), current: 1, spread: false, rtl: false, fit_scale: 1., factor: 1., space: false, pan: None, last_scale: 1. }));
        let weak = Rc::downgrade(&state);
        let block = RcBlock::new(move |pointer: NonNull<NSEvent>| -> *mut NSEvent {
            let Some(state) = weak.upgrade() else { return pointer.as_ptr(); };
            let event = unsafe { pointer.as_ref() };
            if handle_event(&state, event) { std::ptr::null_mut() } else { pointer.as_ptr() }
        });
        let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(NSEventMask::Any, &block) };
        let reader = Self { container, state, monitor };
        reader.set_theme("paper");
        reader
    }

    pub fn view(&self) -> &NSView { &self.container }
    pub fn resize(&self, frame: NSRect) {
        unsafe { self.container.setFrame(frame); }
        if self.zoom_factor() <= 1.001 { self.fit(); }
    }
    pub fn set_frame(&self, frame: NSRect) { self.resize(frame); }
    pub fn open(&self, path: &Path, password: Option<&str>) -> Result<DocumentInfo, String> {
        let document = PDFDocument::from_path(path).ok_or("PDFを開けませんでした。ファイルを確認してください。")?;
        if document.is_locked() {
            let password = password.ok_or("PASSWORD_REQUIRED")?;
            if !document.unlock(password) { return Err("PASSWORD_INCORRECT".into()); }
        }
        if document.page_count() == 0 { return Err("このPDFにはページがありません。".into()); }
        let title = document.title().unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
        let info = DocumentInfo { title: title.clone(), page_count: document.page_count(), path: path.to_owned(), allows_copying: document.allows_copying() };
        {
            let mut state = self.state.borrow_mut();
            state.current = 1; state.title = title; state.document = Some(document.clone()); state.factor = 1.; state.pan = None; state.space = false;
            unsafe { let _: () = msg_send![&*state.view, setDocument: &*document]; }
        }
        self.go_to(1, false);
        Ok(info)
    }
    pub fn document(&self) -> Option<Retained<PDFDocument>> { self.state.borrow().document.clone() }
    pub fn current_page(&self) -> usize { self.state.borrow().current }
    pub fn page_count(&self) -> usize { self.state.borrow().document.as_ref().map(|pdf| pdf.page_count()).unwrap_or(0) }
    pub fn zoom_factor(&self) -> f64 { self.state.borrow().factor }
    pub fn title(&self) -> String { self.state.borrow().title.clone() }
    pub fn go_to(&self, page: usize, _animated: bool) { self.state.borrow_mut().go_to(page); }
    pub fn step(&self, delta: isize) { self.state.borrow_mut().step(delta); }
    pub fn set_spread(&self, enabled: bool) {
        let mut state = self.state.borrow_mut(); state.spread = enabled;
        unsafe { let _: () = msg_send![&*state.view, setDisplayMode: if enabled { 2isize } else { 0isize }]; let _: () = msg_send![&*state.view, setDisplaysAsBook: true]; }
        state.fit();
    }
    pub fn set_rtl(&self, enabled: bool) { let mut state = self.state.borrow_mut(); state.rtl = enabled; unsafe { let _: () = msg_send![&*state.view, setDisplaysRTL: enabled]; } state.fit(); }
    pub fn set_theme(&self, theme: &str) {
        let color = match theme { "night" => NSColor::colorWithSRGBRed_green_blue_alpha(0.09, 0.10, 0.10, 1.), "white" => NSColor::colorWithSRGBRed_green_blue_alpha(0.96, 0.96, 0.96, 1.), _ => NSColor::colorWithSRGBRed_green_blue_alpha(0.91, 0.91, 0.88, 1.) };
        unsafe { let _: () = msg_send![&*self.state.borrow().view, setBackgroundColor: &*color]; }
    }
    pub fn zoom(&self, delta: i32) { let mut state = self.state.borrow_mut(); let next = if delta > 0 { state.factor * 1.25 } else { state.factor / 1.25 }; state.set_zoom(next); }
    pub fn fit(&self) { self.state.borrow_mut().fit(); }
    pub fn outline(&self) -> Vec<(String, usize)> { self.document().map(|document| document.outline()).unwrap_or_default() }
    pub fn thumbnail(&self, page: usize, size: NSSize) -> Option<Retained<NSImage>> { self.document()?.page(page.checked_sub(1)?).map(|page| page.thumbnail(size)) }
    pub fn highlight(&self, query: &str) -> bool {
        let state = self.state.borrow();
        let Some(page) = state.document.as_ref().and_then(|pdf| pdf.page(state.current.saturating_sub(1))) else { return false; };
        let text = NSString::from_str(&page.text());
        unsafe {
            let range: NSRange = msg_send![&*text, rangeOfString: &*NSString::from_str(query), options: 1usize];
            if range.length == 0 || range.location == usize::MAX { return false; }
            let selection: Option<Retained<PDFSelection>> = msg_send![&*page, selectionForRange: range];
            if let Some(selection) = selection { let _: () = msg_send![&*state.view, setCurrentSelection: &*selection, animate: true]; return true; }
        }
        false
    }
    pub fn copy_selection(&self) -> Result<(), String> {
        let state = self.state.borrow();
        let document = state.document.as_ref().ok_or("PDFを開いてください。")?;
        if !document.allows_copying() { return Err("このPDFでは文章のコピーが許可されていません。".into()); }
        let selection: Option<Retained<PDFSelection>> = unsafe { msg_send![&*state.view, currentSelection] };
        let selection = selection.ok_or("コピーする文章を選択してください。")?;
        let text = selection.text();
        if text.trim().is_empty() { return Err("コピーする文章を選択してください。".into()); }
        let pages = selection.pages();
        let first: Option<Retained<PDFPage>> = unsafe { if pages.count() > 0 { msg_send![&*pages, objectAtIndex: 0usize] } else { None } };
        let page = first.map(|page| document.index_of(&page) + 1).unwrap_or(state.current);
        write_text(&format!("{} — p. {}\n\n{}", state.title, page, text.trim()))
    }
    pub fn copy_page_text(&self) -> Result<(), String> {
        let state = self.state.borrow();
        let document = state.document.as_ref().ok_or("PDFを開いてください。")?;
        if !document.allows_copying() { return Err("このPDFでは文章のコピーが許可されていません。".into()); }
        let text = document.page(state.current - 1).map(|page| page.text()).unwrap_or_default();
        if text.trim().is_empty() { return Err("このページにはコピーできる文章がありません。".into()); }
        write_text(&format!("{} — p. {}\n\n{}", state.title, state.current, text.trim()))
    }
    pub fn begin_capture(&self) { crate::enqueue("reader-capture-unavailable", 0); }
    pub fn take_capture(&self) -> Option<Retained<NSImage>> { None }
    pub fn cancel_interaction(&self) -> bool { let mut state = self.state.borrow_mut(); let active = state.pan.is_some() || state.space; state.pan = None; state.space = false; active }
    pub fn tick(&self) {
        let mut state = self.state.borrow_mut();
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
            self.current = target; self.fit();
            crate::enqueue("reader-page", target);
        }
    }
    fn step(&mut self, delta: isize) {
        let target = if self.spread {
            if delta > 0 { if self.current == 1 { 2 } else { self.current - self.current % 2 + 2 } }
            else if self.current <= 3 { 1 } else { self.current - self.current % 2 - 2 }
        } else { self.current.saturating_add_signed(delta.signum()).max(1) };
        self.go_to(target);
    }
    fn set_zoom(&mut self, factor: f64) {
        if self.document.is_none() { return; }
        self.factor = factor.clamp(1., 4.);
        if self.factor <= 1.001 { self.fit(); return; }
        unsafe { let _: () = msg_send![&*self.view, setAutoScales: false]; let _: () = msg_send![&*self.view, setScaleFactor: self.fit_scale * self.factor]; }
        crate::enqueue("reader-zoom", (self.factor * 100.).round() as usize);
    }
    fn scroll_view(&self) -> Option<Retained<NSScrollView>> {
        unsafe { let document: Option<Retained<NSView>> = msg_send![&*self.view, documentView]; document.and_then(|view| view.enclosingScrollView()) }
    }
}

fn write_text(text: &str) -> Result<(), String> {
    if text.len() > 8 * 1024 * 1024 { return Err("コピーする文章が長すぎます。".into()); }
    let pasteboard = NSPasteboard::generalPasteboard(); pasteboard.clearContents();
    if pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString }) { Ok(()) } else { Err("コピーできませんでした。もう一度お試しください。".into()) }
}

fn handle_event(state: &Rc<RefCell<ReaderState>>, event: &NSEvent) -> bool {
    let Ok(mut state) = state.try_borrow_mut() else { return false; };
    let Some(window) = state.view.window() else { return false; };
    if event.window(state.view.mtm()).as_deref() != Some(&*window) { return false; }
    let point = unsafe { state.view.convertPoint_fromView(event.locationInWindow(), None) };
    let bounds = state.view.bounds();
    let inside = point.x >= 0. && point.y >= 0. && point.x <= bounds.size.width && point.y <= bounds.size.height;
    let event_type = event.r#type();
    if event_type == NSEventType::LeftMouseDown && inside { crate::enqueue("reader-click", 0); }
    let responder = window.firstResponder();
    let reader_focused = responder.as_ref().and_then(|responder| responder.downcast_ref::<NSView>()).is_some_and(|view| view.isDescendantOf(&state.view));
    if event_type == NSEventType::KeyUp && event.keyCode() == 49 { state.space = false; state.pan = None; return false; }
    if event_type == NSEventType::KeyDown && reader_focused {
        let modifiers = event.modifierFlags();
        let text = event.charactersIgnoringModifiers().map(|s| s.to_string()).unwrap_or_default();
        if modifiers.contains(NSEventModifierFlags::Command) {
            match text.as_str() { "+" | "=" => { let factor = state.factor * 1.25; state.set_zoom(factor); return true; }, "-" => { let factor = state.factor / 1.25; state.set_zoom(factor); return true; }, "0" => { state.fit(); return true; }, _ => return false }
        }
        if modifiers.intersects(NSEventModifierFlags::Control | NSEventModifierFlags::Option) { return false; }
        match event.keyCode() {
            49 if state.factor > 1.001 => { state.space = true; return true; },
            49 | 124 | 121 => { let delta = if event.keyCode() == 124 && state.rtl || event.keyCode() == 49 && modifiers.contains(NSEventModifierFlags::Shift) { -1 } else { 1 }; state.step(delta); return true; },
            123 | 116 => { let delta = if event.keyCode() == 123 && state.rtl { 1 } else { -1 }; state.step(delta); return true; },
            115 => { state.go_to(1); return true; },
            119 => { let end = state.document.as_ref().map(|d| d.page_count()).unwrap_or(1); state.go_to(end); return true; },
            _ => {}
        }
    }
    if event_type == NSEventType::Magnify && inside { let factor = state.factor * (1. + event.magnification()); state.set_zoom(factor); return true; }
    if event_type == NSEventType::LeftMouseDown && inside && state.space && state.factor > 1. {
        if let Some(scroll) = state.scroll_view() { state.pan = Some((point, scroll.contentView().bounds().origin)); return true; }
    }
    if event_type == NSEventType::LeftMouseDragged {
        if let Some((start, origin)) = state.pan { if let Some(scroll) = state.scroll_view() { let clip = scroll.contentView(); let next = NSPoint::new(origin.x - point.x + start.x, origin.y - point.y + start.y); unsafe { clip.scrollToPoint(next); scroll.reflectScrolledClipView(&clip); } } return true; }
    }
    if event_type == NSEventType::LeftMouseUp && state.pan.take().is_some() { return true; }
    if event_type == NSEventType::ScrollWheel && inside && state.factor <= 1.001 { return true; }
    false
}
