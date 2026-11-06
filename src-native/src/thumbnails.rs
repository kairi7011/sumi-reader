use crate::i18n::{text as tr, is_japanese};
use crate::pdf::PDFDocument;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, AnyThread, MainThreadOnly};
use objc2_app_kit::{NSAccessibility, NSBezierPath, NSButton, NSCellImagePosition, NSColor, NSFont, NSImage, NSImageScaling, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
use std::{collections::VecDeque, ops::Range};

pub const COLUMNS: usize = 6;
pub const CACHE_CAPACITY: usize = 36;
const ROW_HEIGHT: f64 = 140.;
const PADDING: f64 = 6.;
const IMAGE_SIZE: NSSize = NSSize::new(176., 248.);

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
    document: Option<Retained<PDFDocument>>,
    cache: VecDeque<(usize, Option<Retained<NSImage>>)>,
    placeholder: Retained<NSImage>,
}

impl ThumbnailGrid {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0., 0.), NSSize::new(564., ROW_HEIGHT)));
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live thumbnail action target") };
        Self { mtm, target, view, buttons: Vec::new(), realized: 0..0, pages: Vec::new(), current: 0, selected: None, priority: None, document: None, cache: VecDeque::new(), placeholder: paper_placeholder() }
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
        self.document = None;
    }

    pub fn show(&mut self, document: Option<Retained<PDFDocument>>, pages: &[usize], current: usize, width: f64) {
        let same_document = match (&self.document, &document) {
            (Some(old), Some(new)) => std::ptr::eq::<PDFDocument>(&**old, &**new),
            (None, None) => true,
            _ => false,
        };
        let total = document.as_ref().map_or(0, |pdf| pdf.page_count());
        let pages: Vec<usize> = pages.iter().copied().filter(|page| *page > 0 && *page <= total).collect();
        let same_pages = same_document && self.pages == pages;
        if !same_document { self.cache.clear(); }
        if !same_pages { self.remove_buttons(); }
        self.document = document;
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

    /// Called once per UI tick while the chooser is visible; it never renders more than one page.
    pub fn render_next(&mut self) -> bool {
        let changed = self.sync_visible();
        let candidate = self.active_indices().into_iter().find(|index| !self.is_cached(self.pages[*index]));
        let Some(index) = candidate else { return changed; };
        let Some(document) = &self.document else { return changed; };
        let number = self.pages[index];
        let image = autoreleasepool(|_| document.page(number - 1).map(|page| page.thumbnail(IMAGE_SIZE)));
        self.cache.retain(|(page, _)| *page != number);
        self.cache.push_back((number, image));
        while self.cache.len() > CACHE_CAPACITY { self.cache.pop_front(); }
        // Buttons retain their NSImage, so eviction must also replace any visible reference.
        self.refresh_buttons();
        true
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
