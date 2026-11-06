use crate::i18n::{text as tr, is_japanese};
use crate::pdf::PDFDocument;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, AnyClass};
use objc2::{msg_send, sel, AnyThread, ClassType, MainThreadOnly};
use objc2_app_kit::{NSAccessibility, NSBezierPath, NSButton, NSCellImagePosition, NSColor, NSCompositingOperation, NSDeviceRGBColorSpace, NSFont, NSGraphicsContext, NSImage, NSImageScaling, NSView};
use objc2_foundation::{MainThreadMarker, NSData, NSDictionary, NSPoint, NSRect, NSSize, NSString};
use std::{collections::VecDeque, ops::Range, path::PathBuf, sync::{Arc, Condvar, Mutex}, thread};

pub const COLUMNS: usize = 6;
pub const CACHE_CAPACITY: usize = 36;
const ROW_HEIGHT: f64 = 140.;
const PADDING: f64 = 6.;
const IMAGE_SIZE: NSSize = NSSize::new(176., 248.);

#[derive(Clone, PartialEq, Eq)]
pub struct PreviewSource {
    pub path: PathBuf,
    pub password: Option<String>,
    pub revision: (u64, u64),
}

#[derive(Clone, PartialEq, Eq)]
pub struct PreviewRequest {
    pub source: PreviewSource,
    pub page: usize,
    pub width: usize,
    pub height: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PreviewStatus { Ready, Locked, Missing, Unavailable }

pub struct PreviewResult {
    pub request: PreviewRequest,
    pub png: Option<Vec<u8>>,
    pub pages: Option<usize>,
    pub status: PreviewStatus,
}

const QUEUE_CAPACITY: usize = 48;
const RESULT_CAPACITY: usize = 2;

#[derive(Default)]
struct PreviewQueue {
    generation: u64,
    wanted: Vec<PreviewRequest>,
    pending: VecDeque<PreviewRequest>,
    busy: Option<(u64, PreviewRequest)>,
    results: VecDeque<PreviewResult>,
    stopped: bool,
}
impl PreviewQueue {
    fn replace(&mut self, requests: &[PreviewRequest]) {
        self.wanted.clear();
        for request in requests.iter().take(QUEUE_CAPACITY) {
            if !self.wanted.contains(request) { self.wanted.push(request.clone()); }
        }
        self.results.retain(|result| self.wanted.contains(&result.request));
        self.pending = self.wanted.iter().filter(|request| {
            !self.busy.as_ref().is_some_and(|(generation, busy)| *generation == self.generation && busy == *request)
                && !self.results.iter().any(|result| result.request == **request)
        }).cloned().collect();
    }

    fn cancel(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.wanted.clear();self.pending.clear();self.results.clear();
    }

    fn start(&mut self) -> Option<(u64, PreviewRequest)> {
        if self.stopped || self.busy.is_some() || self.results.len() >= RESULT_CAPACITY { return None; }
        let request = self.pending.pop_front()?;
        self.busy = Some((self.generation, request.clone()));
        Some((self.generation, request))
    }

    fn finish(&mut self, generation: u64, result: PreviewResult) {
        self.busy = None;
        if !self.stopped && generation == self.generation && self.wanted.contains(&result.request) {
            self.results.push_back(result);
        }
    }
}

struct PreviewShared { queue: Mutex<PreviewQueue>, wake: Condvar }

pub struct PreviewWorker { shared: Arc<PreviewShared> }
impl Default for PreviewWorker { fn default() -> Self { Self::new() } }
impl PreviewWorker {
    pub fn new() -> Self {
        let shared = Arc::new(PreviewShared { queue: Mutex::new(PreviewQueue::default()), wake: Condvar::new() });
        let state = shared.clone();
        thread::Builder::new().name("sumi-previews".into()).spawn(move || {
            let mut document: Option<(PreviewSource, Retained<PDFDocument>)> = None;
            loop {
                let next = {
                    let mut queue = state.queue.lock().unwrap_or_else(|error|error.into_inner());
                    loop {
                        if queue.stopped { return; }
                        if let Some(next) = queue.start() { break Some(next); }
                        if queue.pending.is_empty() && document.is_some() { break None; }
                        queue = state.wake.wait(queue).unwrap_or_else(|error|error.into_inner());
                    }
                };
                let Some((generation, request)) = next else {
                    // PDFKit destruction can also be expensive; keep it outside the UI-facing lock.
                    document = None;continue;
                };
                let result = autoreleasepool(|_| render_preview(request, &mut document));
                state.queue.lock().unwrap_or_else(|error|error.into_inner()).finish(generation, result);
            }
        }).expect("Could not start the preview worker");
        Self { shared }
    }
    pub fn request(&self, requests: &[PreviewRequest]) {
        self.shared.queue.lock().unwrap_or_else(|error|error.into_inner()).replace(requests);
        self.shared.wake.notify_one();
    }
    pub fn poll(&self) -> Vec<PreviewResult> {
        let results = self.shared.queue.lock().unwrap_or_else(|error|error.into_inner()).results.drain(..).collect();
        self.shared.wake.notify_one();results
    }
    pub fn cancel(&self) {
        self.shared.queue.lock().unwrap_or_else(|error|error.into_inner()).cancel();
        self.shared.wake.notify_one();
    }
}
impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|error|error.into_inner());
        queue.stopped = true;queue.cancel();self.shared.wake.notify_one();
    }
}

fn render_preview(request: PreviewRequest, document: &mut Option<(PreviewSource, Retained<PDFDocument>)>) -> PreviewResult {
    let mut result = PreviewResult { request, png: None, pages: None, status: PreviewStatus::Unavailable };
    let source = &result.request.source;
    if !document.as_ref().is_some_and(|(old,_)| old == source) {
        *document = None;
        if !source.path.is_file() { result.status = PreviewStatus::Missing;return result; }
        let Some(pdf) = PDFDocument::from_path(&source.path) else { return result; };
        if pdf.is_locked() && !source.password.as_deref().is_some_and(|password|pdf.unlock(password)) {
            result.status = PreviewStatus::Locked;return result;
        }
        *document = Some((source.clone(), pdf));
    }
    let pdf = &document.as_ref().unwrap().1;
    result.pages = Some(pdf.page_count());
    if let Some(page) = result.request.page.checked_sub(1).and_then(|index|pdf.page(index)) {
        let size = NSSize::new(result.request.width.clamp(1,512) as f64,result.request.height.clamp(1,512) as f64);
        let thumbnail = page.thumbnail(size);
        result.png = preview_png(&thumbnail, size);
        if result.png.is_some() { result.status = PreviewStatus::Ready; }
    }
    result
}

fn preview_png(source: &NSImage, size: NSSize) -> Option<Vec<u8>> {
    let original = source.size();
    if !original.width.is_finite() || !original.height.is_finite() || original.width <= 0. || original.height <= 0. { return None; }
    let scale = (size.width / original.width).min(size.height / original.height);
    unsafe {
        let class = AnyClass::get(c"NSBitmapImageRep")?;
        let allocated: objc2::rc::Allocated<AnyObject> = msg_send![class,alloc];
        let bitmap: Option<Retained<AnyObject>> = msg_send![allocated,initWithBitmapDataPlanes:std::ptr::null_mut::<*mut u8>(),pixelsWide:size.width as isize,pixelsHigh:size.height as isize,bitsPerSample:8isize,samplesPerPixel:4isize,hasAlpha:true,isPlanar:false,colorSpaceName:NSDeviceRGBColorSpace,bytesPerRow:0isize,bitsPerPixel:32isize];
        let bitmap = bitmap?;
        let context: Option<Retained<NSGraphicsContext>> = msg_send![NSGraphicsContext::class(),graphicsContextWithBitmapImageRep:&*bitmap];
        let context = context?;
        NSGraphicsContext::saveGraphicsState_class();NSGraphicsContext::setCurrentContext(Some(&context));
        let cg = context.CGContext();
        objc2_core_graphics::CGContext::clear_rect(Some(&cg), NSRect::new(NSPoint::new(0.,0.),size));
        source.drawInRect_fromRect_operation_fraction(NSRect::new(NSPoint::new((size.width-original.width*scale)/2.,(size.height-original.height*scale)/2.),NSSize::new(original.width*scale,original.height*scale)),NSRect::new(NSPoint::new(0.,0.),original),NSCompositingOperation::SourceOver,1.);
        NSGraphicsContext::restoreGraphicsState_class();
        let properties: Retained<NSDictionary<NSString,AnyObject>> = NSDictionary::new();
        let data: Option<Retained<NSData>> = msg_send![&*bitmap,representationUsingType:4usize,properties:&*properties];
        data.filter(|data|data.len() <= 2*1024*1024).map(|data|data.to_vec())
    }
}

pub fn preview_image(png: &[u8], size: NSSize) -> Option<Retained<NSImage>> {
    let image = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(png))?;
    image.setSize(size);Some(image)
}

#[derive(Clone, Copy)]
struct GridGeometry {
    count: usize,
    width: f64,
}
impl GridGeometry {
    fn height(self) -> f64 { self.count.div_ceil(COLUMNS).max(1) as f64 * ROW_HEIGHT + PADDING * 2. }

    fn frame(self, index: usize) -> NSRect {
        let cell = (self.width - PADDING * 2.) / COLUMNS as f64;
        NSRect::new(NSPoint::new(PADDING + (index % COLUMNS) as f64 * cell, self.height() - PADDING - (index / COLUMNS + 1) as f64 * ROW_HEIGHT), NSSize::new(cell - PADDING, ROW_HEIGHT - PADDING))
    }

    fn visible(self, viewport: NSRect) -> Range<usize> {
        if self.count == 0 || viewport.size.width <= 0. || viewport.size.height <= 0. { return 0..0; }
        let rows = self.count.div_ceil(COLUMNS);
        let top = (self.height() - PADDING - viewport.origin.y - viewport.size.height).clamp(0., rows as f64 * ROW_HEIGHT);
        let bottom = (self.height() - PADDING - viewport.origin.y).clamp(0., rows as f64 * ROW_HEIGHT);
        if bottom <= top { return 0..0; }
        let first = (top / ROW_HEIGHT).floor() as usize;
        let last = ((bottom / ROW_HEIGHT).ceil() as usize).min(rows);
        (first * COLUMNS).min(self.count)..(last * COLUMNS).min(self.count)
    }

    fn realized(self, viewport: NSRect) -> Range<usize> {
        let visible = self.visible(viewport);
        if visible.is_empty() { return visible; }
        visible.start.saturating_sub(COLUMNS)..visible.end.saturating_add(COLUMNS).min(self.count)
    }
}

pub struct ThumbnailGrid {
    mtm: MainThreadMarker,
    target: Retained<AnyObject>,
    view: Retained<NSView>,
    buttons: Vec<Retained<NSButton>>,
    realized: Range<usize>,
    pages: Vec<usize>,
    current: usize,
    selected: Option<usize>,
    priority: Option<usize>,
    source: Option<PreviewSource>,
    worker: PreviewWorker,
    active: bool,
    cache: VecDeque<(usize, Option<Retained<NSImage>>)>,
    placeholder: Retained<NSImage>,
}

impl ThumbnailGrid {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0., 0.), NSSize::new(564., ROW_HEIGHT)));
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live thumbnail action target") };
        Self { mtm, target, view, buttons: Vec::new(), realized: 0..0, pages: Vec::new(), current: 0, selected: None, priority: None, source: None, worker: PreviewWorker::new(), active: true, cache: VecDeque::new(), placeholder: paper_placeholder() }
    }

    pub fn view(&self) -> &NSView { &self.view }
    pub fn buttons(&self) -> &[Retained<NSButton>] { &self.buttons }
    pub fn height(&self) -> f64 { self.view.frame().size.height }
    pub fn page_at(&self, index: usize) -> Option<usize> { self.pages.get(index).copied() }
    pub fn index_for_page(&self, page: usize) -> Option<usize> { self.pages.iter().position(|number| *number == page) }
    pub fn realized_range(&self) -> Range<usize> { self.realized.clone() }
    pub fn visible_range(&self) -> Range<usize> { self.geometry().visible(self.view.visibleRect()) }
    pub fn cached_count(&self) -> usize { self.cache.len() }
    pub fn refresh_language(&mut self) { self.refresh_buttons(); self.update_selection(); }
    pub fn remaining(&self) -> usize { self.active_indices().into_iter().filter(|index| !self.is_cached(self.pages[*index])).count() }
    pub fn is_loading(&self) -> bool { self.active && self.source.is_some() && self.visible_range().any(|index|!self.is_cached(self.pages[index])) }
    pub fn set_active(&mut self, active: bool) { if self.active != active { self.active = active;if !active {self.worker.cancel();} } }

    pub fn prioritize(&mut self, page: usize) { self.priority = Some(page); }

    pub fn set_selected(&mut self, index: usize, reveal: bool) {
        self.selected = (!self.pages.is_empty()).then(|| index.min(self.pages.len() - 1));
        self.priority = self.selected.and_then(|index| self.page_at(index));
        if reveal {
            if let Some(index) = self.selected { self.view.scrollRectToVisible(self.geometry().frame(index)); }
            self.sync_visible();
        }
        self.update_selection();
    }

    pub fn scroll_to_page(&mut self, page: usize) {
        if let Some(index) = self.index_for_page(page) { self.set_selected(index, true); }
    }

    pub fn clear(&mut self) {
        self.remove_buttons();
        self.pages.clear();
        self.cache.clear();
        self.selected = None;
        self.priority = None;
        self.source = None;
        self.worker.cancel();
    }

    pub fn show(&mut self, source: Option<PreviewSource>, pages: &[usize], current: usize, width: f64) {
        let same_document = self.source == source;
        let pages: Vec<usize> = pages.iter().copied().filter(|page| *page > 0).collect();
        let same_pages = same_document && self.pages == pages;
        if !same_document { self.cache.clear();self.worker.cancel(); }
        if !same_pages { self.remove_buttons(); }
        self.source = source;
        self.current = current;
        self.pages = pages;
        if !same_pages {
            self.selected = self.index_for_page(current).or_else(|| (!self.pages.is_empty()).then_some(0));
            self.priority = self.selected.and_then(|index| self.page_at(index));
        }
        let geometry = GridGeometry { count: self.pages.len(), width: width.max(240.) };
        self.view.setFrameSize(NSSize::new(geometry.width, geometry.height()));
        for (offset, button) in self.buttons.iter().enumerate() { button.setFrame(geometry.frame(self.realized.start + offset)); }
        self.refresh_buttons();
    }

    /// The UI tick only installs completed images and updates the bounded background queue.
    pub fn render_next(&mut self) -> bool {
        if !self.active { return false; }
        let mut changed = self.sync_visible();
        for result in self.worker.poll() {
            if self.source.as_ref()!=Some(&result.request.source) || !self.pages.contains(&result.request.page) {continue;}
            let image = result.png.as_deref().and_then(|png|preview_image(png,IMAGE_SIZE));
            self.cache.retain(|(page,_)|*page!=result.request.page);
            self.cache.push_back((result.request.page,image));
            while self.cache.len()>CACHE_CAPACITY {self.cache.pop_front();}
            changed = true;
        }
        let requests = self.source.as_ref().map(|source|self.active_indices().into_iter()
            .filter(|index|!self.is_cached(self.pages[*index]))
            .map(|index|PreviewRequest { source:source.clone(),page:self.pages[index],width:IMAGE_SIZE.width as usize,height:IMAGE_SIZE.height as usize }).collect::<Vec<_>>()).unwrap_or_default();
        self.worker.request(&requests);
        // Buttons retain their NSImage, so eviction must also replace any visible reference.
        if changed {self.refresh_buttons();}
        changed
    }

    fn geometry(&self) -> GridGeometry { GridGeometry { count: self.pages.len(), width: self.view.frame().size.width } }
    fn is_cached(&self, page: usize) -> bool { self.cache.iter().any(|(number, _)| *number == page) }

    fn remove_buttons(&mut self) {
        for button in self.buttons.drain(..) { button.removeFromSuperview(); }
        self.realized = 0..0;
    }

    fn active_indices(&self) -> Vec<usize> {
        let visible = self.visible_range();
        let center = (visible.start as f64 + visible.end as f64) / 2.;
        let mut indices: Vec<usize> = self.realized.clone().collect();
        indices.sort_by(|a, b| {
            let priority = |index: usize| if visible.contains(&index) && self.page_at(index) == self.priority { 0 } else if visible.contains(&index) { 1 } else { 2 };
            priority(*a).cmp(&priority(*b)).then_with(|| ((*a as f64 - center).abs()).total_cmp(&(*b as f64 - center).abs())).then(a.cmp(b))
        });
        // The active render set must fit the cache to avoid repeatedly evicting its own thumbnails.
        indices.truncate(CACHE_CAPACITY);
        indices
    }

    fn sync_visible(&mut self) -> bool {
        let geometry = self.geometry();
        let next = geometry.realized(self.view.visibleRect());
        if next == self.realized { return false; }
        let previous = self.realized.clone();
        let buttons = std::mem::take(&mut self.buttons);
        for (offset, button) in buttons.iter().enumerate() {
            if !next.contains(&(previous.start + offset)) { button.removeFromSuperview(); }
        }
        for index in next.clone() {
            let button = if previous.contains(&index) { buttons[index - previous.start].clone() } else {
                let button = unsafe { NSButton::buttonWithTitle_target_action(&NSString::from_str(""), Some(&*self.target), Some(sel!(activateRow:)), self.mtm) };
                button.setTag(index as isize);
                button.setBordered(false);
                button.setFont(Some(&NSFont::systemFontOfSize(11.)));
                button.setImagePosition(NSCellImagePosition::ImageAbove);
                button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
                self.view.addSubview(&button);
                button
            };
            button.setFrame(geometry.frame(index));
            self.buttons.push(button);
        }
        self.realized = next;
        self.refresh_buttons();
        self.update_selection();
        true
    }

    fn refresh_buttons(&self) {
        for (offset, button) in self.buttons.iter().enumerate() {
            let page = self.pages[self.realized.start + offset];
            let cached = self.cache.iter().find(|(number, _)| *number == page);
            let image = cached.and_then(|(_, image)| image.as_deref());
            button.setImage(Some(image.unwrap_or(&self.placeholder)));
            button.setTitle(&NSString::from_str(&caption(page, self.current, cached.is_none())));
            let action = if is_japanese() { format!("{page}ページを開く") } else { format!("Open page {page}") };
            button.setToolTip(Some(&NSString::from_str(&format!("{action}{}", if cached.is_some() && image.is_none() { tr(" · プレビューを表示できません", " · Preview unavailable") } else { "" }))));
        }
    }

    fn update_selection(&self) {
        for (offset, button) in self.buttons.iter().enumerate() {
            let index = self.realized.start + offset;
            let page = self.pages[index];
            let selected = self.selected == Some(index);
            let action = if is_japanese() { format!("{page}ページを開く") } else { format!("Open page {page}") };
            button.setAccessibilityLabel(Some(&NSString::from_str(&format!("{action}{}{}", if page == self.current { tr(" · 読書中", " · Reading") } else { "" }, if selected { tr(" · 選択中", " · Selected") } else { "" }))));
            button.setWantsLayer(true);
            let color = if selected { NSColor::controlAccentColor().colorWithAlphaComponent(0.13) } else { NSColor::clearColor() };
            unsafe {
                let layer: Option<Retained<AnyObject>> = msg_send![&**button, layer];
                if let Some(layer) = layer {
                    let _: () = msg_send![&*layer, setCornerRadius: 7.0f64];
                    let cg: *const objc2_core_graphics::CGColor = msg_send![&*color, CGColor];
                    let _: () = msg_send![&*layer, setBackgroundColor: cg];
                }
            }
        }
    }
}

fn caption(page: usize, current: usize, loading: bool) -> String {
    if page == current { format!("{page} · {}", tr("現在", "Current")) }
    else if loading { format!("{page} …") }
    else { page.to_string() }
}

#[allow(deprecated)]
fn paper_placeholder() -> Retained<NSImage> {
    let image = NSImage::initWithSize(NSImage::alloc(), IMAGE_SIZE);
    image.lockFocus();
    NSColor::colorWithSRGBRed_green_blue_alpha(0.97, 0.97, 0.96, 1.).setFill();
    NSBezierPath::fillRect(NSRect::new(NSPoint::new(0., 0.), IMAGE_SIZE));
    NSColor::colorWithSRGBRed_green_blue_alpha(0.84, 0.85, 0.82, 1.).setStroke();
    NSBezierPath::strokeRect(NSRect::new(NSPoint::new(1., 1.), NSSize::new(IMAGE_SIZE.width - 2., IMAGE_SIZE.height - 2.)));
    image.unlockFocus();
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport(y: f64, height: f64) -> NSRect { NSRect::new(NSPoint::new(0., y), NSSize::new(640., height)) }
    fn request(page: usize, revision: u64) -> PreviewRequest {
        PreviewRequest {source:PreviewSource{path:PathBuf::from("/synthetic/book.pdf"),password:None,revision:(100,revision)},page,width:176,height:248}
    }
    fn completed(request: PreviewRequest) -> PreviewResult {
        PreviewResult {request,png:Some(vec![1,2,3]),pages:Some(200),status:PreviewStatus::Ready}
    }

    #[test]
    fn changing_books_or_revisions_discards_a_decode_already_in_progress() {
        let mut queue=PreviewQueue::default();queue.replace(&[request(1,1)]);
        let (generation,old)=queue.start().unwrap();
        queue.cancel();queue.replace(&[request(1,2)]);
        assert!(queue.start().is_none());
        queue.finish(generation,completed(old));
        assert!(queue.results.is_empty());
        let (generation,new)=queue.start().unwrap();assert_eq!(new.source.revision,(100,2));
        queue.finish(generation,completed(new));assert_eq!(queue.results.len(),1);
        queue.cancel();assert!(queue.results.is_empty());assert!(queue.pending.is_empty());
    }

    #[test]
    fn rapid_scrolling_replaces_pending_work_and_rejects_offscreen_results() {
        let mut queue=PreviewQueue::default();queue.replace(&[request(1,1),request(2,1)]);
        let (generation,old)=queue.start().unwrap();
        queue.replace(&[request(180,1),request(181,1)]);
        queue.finish(generation,completed(old));assert!(queue.results.is_empty());
        let (_,next)=queue.start().unwrap();assert_eq!(next.page,180);
        assert_eq!(queue.pending.iter().map(|request|request.page).collect::<Vec<_>>(),vec![181]);
    }

    #[test]
    fn background_backlog_and_completed_images_stay_bounded() {
        let mut queue=PreviewQueue::default();
        queue.replace(&(1..=200).map(|page|request(page,1)).collect::<Vec<_>>());
        assert_eq!(queue.pending.len(),QUEUE_CAPACITY);
        for _ in 0..RESULT_CAPACITY {let (generation,next)=queue.start().unwrap();queue.finish(generation,completed(next));}
        assert!(queue.start().is_none());assert_eq!(queue.results.len(),RESULT_CAPACITY);
        queue.results.clear();assert!(queue.start().is_some());
    }

    #[test]
    fn queued_failures_are_delivered_once_instead_of_retried_while_waiting_for_the_ui() {
        let mut queue=PreviewQueue::default();queue.replace(&[request(1,1),request(1,1)]);
        assert_eq!(queue.pending.len(),1);
        let (generation,next)=queue.start().unwrap();
        queue.finish(generation,PreviewResult{request:next,png:None,pages:None,status:PreviewStatus::Missing});
        queue.replace(&[request(1,1)]);
        assert!(queue.pending.is_empty());assert_eq!(queue.results.len(),1);
    }

    #[test]
    fn a_thousand_page_book_reaches_the_final_partial_row_by_scrolling() {
        let grid = GridGeometry { count: 1000, width: 640. };
        let first = grid.realized(viewport(grid.height() - 572., 572.));
        let last = grid.realized(viewport(0., 572.));
        assert_eq!(first.start, 0);
        assert_eq!(last.end, 1000);
        assert!(last.contains(&999));
        assert!(first.end < last.start);
        assert!(first.len() <= 48 && last.len() <= 48);
        let final_page = grid.frame(999);
        assert!(final_page.origin.y >= 0.);
        assert!(final_page.origin.y + final_page.size.height <= 572.);
    }

    #[test]
    fn distant_jumps_realize_the_target_with_only_a_viewport_of_neighbors() {
        let grid = GridGeometry { count: 20_000, width: 640. };
        let target = grid.frame(16_789);
        let visible = grid.visible(viewport(target.origin.y, 572.));
        let realized = grid.realized(viewport(target.origin.y, 572.));
        assert!(visible.contains(&16_789));
        assert!(realized.start <= visible.start && realized.end >= visible.end);
        assert!(realized.len() <= 48);
        assert!(realized.start > 16_000);
    }

    #[test]
    fn empty_short_and_elastically_scrolled_views_stay_within_document_bounds() {
        let empty = GridGeometry { count: 0, width: 640. };
        assert!(empty.realized(viewport(0., 572.)).is_empty());
        let short = GridGeometry { count: 3, width: 640. };
        assert_eq!(short.realized(viewport(0., 572.)), 0..3);
        assert_eq!(short.realized(viewport(-100., 572.)), 0..3);
        assert!(short.realized(viewport(short.height() + 1., 572.)).is_empty());
        assert!(short.realized(viewport(0., 0.)).is_empty());
        for count in [1, 6, 7, 999, 1000] {
            let grid = GridGeometry { count, width: 640. };
            for y in [-100., 0., grid.height() / 2., grid.height() - 572., grid.height()] {
                let range = grid.realized(viewport(y, 572.));
                assert!(range.start <= range.end && range.end <= count);
                assert!(range.len() <= 48);
            }
        }
    }
}
