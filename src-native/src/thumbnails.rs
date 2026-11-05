use crate::pdf::PDFDocument;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{sel, AnyThread, MainThreadOnly};
use objc2_app_kit::{NSAccessibility, NSBezierPath, NSButton, NSCellImagePosition, NSColor, NSFont, NSImage, NSImageScaling, NSView};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
use std::collections::VecDeque;

pub const COLUMNS: usize = 6;
const CAPACITY: usize = 24;
const ROW_HEIGHT: f64 = 140.;
const IMAGE_SIZE: NSSize = NSSize::new(176., 248.);

pub fn pages_near(target: usize, total: usize) -> Vec<usize> {
    if total == 0 { return Vec::new(); }
    let target = target.clamp(1, total);
    let start = target.saturating_sub(CAPACITY / 2).max(1).min(total.saturating_sub(CAPACITY - 1).max(1));
    (start..=(start + CAPACITY - 1).min(total)).collect()
}

pub struct ThumbnailGrid {
    mtm: MainThreadMarker,
    target: Retained<AnyObject>,
    view: Retained<NSView>,
    buttons: Vec<Retained<NSButton>>,
    pages: Vec<usize>,
    current: usize,
    document: Option<Retained<PDFDocument>>,
    pending: VecDeque<usize>,
    cache: VecDeque<(usize, Retained<NSImage>)>,
    placeholder: Retained<NSImage>,
}

impl ThumbnailGrid {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::new(NSPoint::new(0., 0.), NSSize::new(564., ROW_HEIGHT)));
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live thumbnail action target") };
        Self { mtm, target, view, buttons: Vec::new(), pages: Vec::new(), current: 0, document: None, pending: VecDeque::new(), cache: VecDeque::new(), placeholder: paper_placeholder() }
    }

    pub fn view(&self) -> &NSView { &self.view }
    pub fn buttons(&self) -> &[Retained<NSButton>] { &self.buttons }
    pub fn height(&self) -> f64 { self.view.frame().size.height }
    pub fn remaining(&self) -> usize { self.pending.len() }

    pub fn prioritize(&mut self, page: usize) {
        if let Some(index) = self.pending.iter().position(|number| *number == page) {
            let page = self.pending.remove(index).unwrap();
            self.pending.push_front(page);
        }
    }

    pub fn clear(&mut self) {
        for button in self.buttons.drain(..) { unsafe { button.removeFromSuperview(); } }
        self.pages.clear();
        self.pending.clear();
        self.cache.clear();
        self.document = None;
    }

    pub fn show(&mut self, document: Option<Retained<PDFDocument>>, pages: &[usize], current: usize, width: f64) {
        let same_document = match (&self.document, &document) {
            (Some(old), Some(new)) => std::ptr::eq::<PDFDocument>(&**old, &**new),
            (None, None) => true,
            _ => false,
        };
        if !same_document { self.cache.clear(); }
        self.document = document;
        self.current = current;
        self.pending.clear();
        for button in self.buttons.drain(..) { unsafe { button.removeFromSuperview(); } }
        let total = self.document.as_ref().map_or(0, |pdf| pdf.page_count());
        self.pages = pages.iter().copied().filter(|page| *page > 0 && *page <= total).take(CAPACITY).collect();
        let width = width.max(240.);
        let rows = self.pages.len().div_ceil(COLUMNS).max(1);
        let height = rows as f64 * ROW_HEIGHT + 12.;
        let cell_width = (width - 12.) / COLUMNS as f64;
        unsafe { self.view.setFrame(NSRect::new(NSPoint::new(0., 0.), NSSize::new(width, height))); }
        for (index, page) in self.pages.iter().copied().enumerate() {
            let image = self.cache.iter().find(|(number, _)| *number == page).map(|(_, image)| image.clone());
            let title = caption(page, current, image.is_none());
            let button = unsafe {
                NSButton::buttonWithTitle_target_action(&NSString::from_str(&title), Some(&*self.target), Some(sel!(activateRow:)), self.mtm)
            };
            unsafe {
                button.setTag(index as isize);
                button.setBordered(false);
                button.setFont(Some(&NSFont::systemFontOfSize(11.)));
                button.setImagePosition(NSCellImagePosition::ImageAbove);
                button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
                button.setImage(Some(image.as_deref().unwrap_or(&self.placeholder)));
                button.setFrame(NSRect::new(NSPoint::new(6. + (index % COLUMNS) as f64 * cell_width, height - 6. - (index / COLUMNS + 1) as f64 * ROW_HEIGHT), NSSize::new(cell_width - 6., ROW_HEIGHT - 6.)));
                button.setToolTip(Some(&NSString::from_str(&format!("{page}ページを開く"))));
                button.setAccessibilityLabel(Some(&NSString::from_str(&format!("{page}ページを開く{}", if page == current { " · 読書中" } else { "" }))));
                self.view.addSubview(&button);
            }
            if image.is_none() { self.pending.push_back(page); }
            self.buttons.push(button);
        }
        let priority = if self.pages.contains(&current) { current } else { self.pages.get(self.pages.len() / 2).copied().unwrap_or(0) };
        self.prioritize(priority);
    }

    /// Call once per UI idle tick while the page chooser is visible.
    pub fn render_next(&mut self) -> bool {
        let Some(number) = self.pending.pop_front() else { return false; };
        let Some(document) = &self.document else { return false; };
        let image = autoreleasepool(|_| document.page(number - 1).map(|page| page.thumbnail(IMAGE_SIZE)));
        let Some(image) = image else {
            if let Some(index) = self.pages.iter().position(|page| *page == number) {
                unsafe {
                    self.buttons[index].setTitle(&NSString::from_str(&caption(number, self.current, false)));
                    self.buttons[index].setToolTip(Some(&NSString::from_str(&format!("{number}ページを開く · プレビューを表示できません"))));
                }
            }
            return true;
        };
        self.cache.retain(|(page, _)| *page != number);
        self.cache.push_back((number, image.clone()));
        while self.cache.len() > CAPACITY { self.cache.pop_front(); }
        if let Some(index) = self.pages.iter().position(|page| *page == number) {
            unsafe {
                self.buttons[index].setImage(Some(&image));
                self.buttons[index].setTitle(&NSString::from_str(&caption(number, self.current, false)));
            }
        }
        true
    }
}

fn caption(page: usize, current: usize, loading: bool) -> String {
    if page == current { format!("{page} · 現在") }
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

    #[test]
    fn page_window_keeps_distant_targets_visible_without_requesting_every_page() {
        let pages = pages_near(200, 1000);
        assert_eq!(pages.len(), 24);
        assert!(pages.contains(&200));
        assert!(pages.windows(2).all(|pair| pair[1] == pair[0] + 1));
        assert_eq!(pages_near(1, 1000), (1..=24).collect::<Vec<_>>());
        assert_eq!(pages_near(1000, 1000), (977..=1000).collect::<Vec<_>>());
    }

    #[test]
    fn short_and_empty_books_never_generate_out_of_range_thumbnails() {
        assert!(pages_near(1, 0).is_empty());
        assert_eq!(pages_near(999, 3), vec![1, 2, 3]);
        assert_eq!(pages_near(0, 1), vec![1]);
    }
}
