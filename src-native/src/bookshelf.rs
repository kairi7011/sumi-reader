use sumi_native::i18n::{text as tr, is_japanese};
use crate::pdf::PDFDocument;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{define_class, msg_send, sel, AnyThread, MainThreadOnly};
use objc2_app_kit::{NSAccessibility, NSBezierPath, NSBorderType, NSButton, NSCellImagePosition, NSColor, NSCompositingOperation, NSFont, NSImage, NSImageScaling, NSLineBreakMode, NSMenu, NSMenuItem, NSMutableParagraphStyle, NSScrollView, NSTextAlignment, NSView};
use objc2_foundation::{MainThreadMarker, NSMutableAttributedString, NSPoint, NSRange, NSRect, NSSize, NSString};
use std::{collections::VecDeque, path::PathBuf, time::SystemTime};

const CACHE_LIMIT: usize = 48;
const COVER_WIDTH: f64 = 160.;
const COVER_HEIGHT: f64 = 230.;
const ROW_HEIGHT: f64 = 326.;
const CARD_HEIGHT: f64 = 302.;
const MARGIN: f64 = 28.;
const GAP: f64 = 24.;

fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width.max(0.), height.max(0.)))
}
fn string(text: &str) -> Retained<NSString> { NSString::from_str(text) }

define_class!(
    #[unsafe(super = NSScrollView)]
    #[thread_kind = MainThreadOnly]
    struct ShelfScrollView;
    impl ShelfScrollView {
        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool { true }
    }
);

#[derive(Clone)]
pub struct BookCard {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    pub page: usize,
    pub current: bool,
}

#[derive(Clone, PartialEq, Eq)]
struct CoverKey {
    id: String,
    path: PathBuf,
    file: Option<(u64, Option<SystemTime>)>,
}
impl CoverKey {
    fn for_book(book: &BookCard) -> Self {
        let file = std::fs::metadata(&book.path).ok().filter(|value| value.is_file()).map(|value| (value.len(), value.modified().ok()));
        Self { id: book.id.clone(), path: book.path.clone(), file }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CoverStatus { Pending, Ready, Locked, Missing, Unavailable }

#[derive(Clone)]
struct Cover {
    key: CoverKey,
    image: Option<Retained<NSImage>>,
    pages: Option<usize>,
    status: CoverStatus,
}

struct CardView {
    book: BookCard,
    key: CoverKey,
    button: Retained<NSButton>,
    pages: Option<usize>,
    status: CoverStatus,
}

pub struct Bookshelf {
    mtm: MainThreadMarker,
    target: Retained<AnyObject>,
    scroll: Retained<ShelfScrollView>,
    content: Retained<NSView>,
    cards: Vec<CardView>,
    cache: VecDeque<Cover>,
    placeholder: Retained<NSImage>,
    selected: Option<usize>,
    columns: usize,
    row_height: f64,
}

impl Bookshelf {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let scroll: Retained<ShelfScrollView> = unsafe { msg_send![ShelfScrollView::alloc(mtm), initWithFrame: rect(0., 0., 800., 600.)] };
        scroll.setBorderType(NSBorderType::NoBorder);
        scroll.setHasVerticalScroller(true);
        scroll.setHasHorizontalScroller(false);
        scroll.setAutohidesScrollers(true);
        scroll.setDrawsBackground(true);
        scroll.setBackgroundColor(&NSColor::windowBackgroundColor());
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 800., 600.));
        scroll.setDocumentView(Some(&content));
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live bookshelf action target") };
        Self { mtm, target, scroll, content, cards: Vec::new(), cache: VecDeque::new(), placeholder: placeholder(), selected: None, columns: 1, row_height: ROW_HEIGHT }
    }

    pub fn view(&self) -> &NSScrollView { &self.scroll }
    pub fn id_at(&self, index: usize) -> Option<&str> { self.cards.get(index).map(|card| card.book.id.as_str()) }
    pub fn selected_index(&self) -> Option<usize> { self.selected }
    pub fn columns(&self) -> usize { self.columns }
    pub fn set_background(&self, color: &NSColor) { self.scroll.setBackgroundColor(color); }

    pub fn refresh_language(&mut self) {
        for (index, card) in self.cards.iter().enumerate() {
            unsafe { card.button.setMenu(Some(&self.context_menu(&card.book.id))); }
            self.update_title(index);
        }
    }

    pub fn show(&mut self, books: &[BookCard], width: f64, height: f64) {
        let same_order = self.cards.len() == books.len() && self.cards.iter().zip(books).all(|(old, new)| old.book.id == new.id);
        let selected_id = self.selected.and_then(|index| self.id_at(index)).map(str::to_owned);
        let old_top = self.top_offset();
        for card in self.cards.drain(..) { card.button.removeFromSuperview(); }
        for (index, book) in books.iter().enumerate() {
            let key = CoverKey::for_book(book);
            let cached = self.cache.iter().position(|cover| cover.key == key).and_then(|index| self.cache.remove(index));
            if let Some(cover) = &cached { self.cache.push_back(cover.clone()); }
            let status = cached.as_ref().map_or(if key.file.is_none() { CoverStatus::Missing } else { CoverStatus::Pending }, |cover| cover.status);
            let button = unsafe { NSButton::buttonWithTitle_target_action(&string(""), Some(&*self.target), Some(sel!(activateBook:)), self.mtm) };
            button.setTag(index as isize);
            button.setBordered(false);
            button.setImagePosition(NSCellImagePosition::ImageAbove);
            button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
            button.setImage(Some(cached.as_ref().and_then(|cover| cover.image.as_deref()).unwrap_or(&self.placeholder)));
            button.setFont(Some(&NSFont::systemFontOfSize(13.)));
            button.setAlignment(NSTextAlignment::Center);
            let menu = self.context_menu(&book.id);
            unsafe { button.setMenu(Some(&menu)); }
            self.content.addSubview(&button);
            self.cards.push(CardView { book: book.clone(), key, button, pages: cached.and_then(|cover| cover.pages), status });
        }
        self.selected = selected_id.and_then(|id| self.cards.iter().position(|card| card.book.id == id));
        self.layout(width, height);
        self.scroll_to_top_offset(if same_order { old_top } else { 0. });
        for index in 0..self.cards.len() { self.update_title(index); }
        self.update_selection();
    }

    pub fn layout(&mut self, width: f64, height: f64) {
        let old_top = self.top_offset();
        self.scroll.setFrameSize(NSSize::new(width.max(1.), height.max(1.)));
        let width = self.scroll.contentSize().width.max(1.);
        let card_height = compact_card_height(self.scroll.contentSize().height);
        self.row_height = card_height + (ROW_HEIGHT - CARD_HEIGHT);
        self.columns = column_count(width);
        let available = (width - MARGIN * 2.).max(1.);
        let card_width = ((available - GAP * (self.columns - 1) as f64) / self.columns as f64).max(1.);
        let rows = self.cards.len().div_ceil(self.columns);
        let content_height = (rows as f64 * self.row_height + MARGIN * 2.).max(self.scroll.contentSize().height);
        self.content.setFrame(rect(0., 0., width, content_height));
        for (index, card) in self.cards.iter().enumerate() {
            let x = MARGIN + (index % self.columns) as f64 * (card_width + GAP);
            let y = content_height - MARGIN - (index / self.columns) as f64 * self.row_height - card_height;
            card.button.setFrame(rect(x, y, card_width, card_height));
        }
        self.scroll_to_top_offset(old_top);
    }

    pub fn select(&mut self, index: usize) {
        if index >= self.cards.len() { return; }
        self.selected = Some(index);
        self.update_selection();
        self.content.scrollRectToVisible(self.cards[index].button.frame());
    }

    pub fn move_selection(&mut self, horizontal: isize, vertical: isize) {
        if self.cards.is_empty() { return; }
        let delta = horizontal + vertical * self.columns as isize;
        let viewport = self.scroll.documentVisibleRect();
        let index = self.selected.map(|index| index.saturating_add_signed(delta).min(self.cards.len() - 1))
            .unwrap_or_else(|| self.cards.iter().position(|card| {
                let frame = card.button.frame();
                frame.origin.y >= viewport.origin.y && frame.origin.y + frame.size.height <= viewport.origin.y + viewport.size.height
            }).or_else(|| self.cards.iter().position(|card| intersects(card.button.frame(), viewport))).unwrap_or(0));
        self.select(index);
    }

    pub fn clear_selection_outside(&mut self, hit: Option<&NSView>) -> bool {
        let scroller: Option<Retained<NSView>> = unsafe { msg_send![&*self.scroll, verticalScroller] };
        if hit.is_some_and(|hit| self.cards.iter().any(|card| hit.isDescendantOf(&card.button))
            || scroller.as_ref().is_some_and(|scroller| hit.isDescendantOf(scroller))) { return false; }
        if self.selected.take().is_some() { self.update_selection(); }
        true
    }

    pub fn activate_selected(&self) {
        if let Some(card) = self.selected.and_then(|index| self.cards.get(index)) {
            unsafe { card.button.performClick(None); }
        }
    }

    pub fn render_next(&mut self) -> bool {
        let viewport = self.scroll.documentVisibleRect();
        let near = rect(viewport.origin.x, viewport.origin.y - self.row_height, viewport.size.width, viewport.size.height + self.row_height * 2.);
        let mut active: Vec<_> = self.cards.iter().enumerate().filter(|(_, card)| intersects(card.button.frame(), near))
            .map(|(index, card)| (if Some(index) == self.selected && intersects(card.button.frame(), viewport) { 0 } else if intersects(card.button.frame(), viewport) { 1 } else { 2 }, index)).collect();
        active.sort_unstable();
        // Prefetch must fit the cache, or a tall window repeatedly evicts its own visible covers.
        let candidate = active.into_iter().take(CACHE_LIMIT).find(|(_, index)| self.cards[*index].status == CoverStatus::Pending).map(|(_, index)| index);
        let Some(index) = candidate else { return false; };
        let key = self.cards[index].key.clone();
        let cover = autoreleasepool(|_| {
            let mut cover = Cover { key: key.clone(), image: None, pages: None, status: CoverStatus::Unavailable };
            if !key.path.is_file() { cover.status = CoverStatus::Missing; return cover; }
            let Some(document) = PDFDocument::from_path(&key.path) else { return cover; };
            if document.is_locked() { cover.status = CoverStatus::Locked; return cover; }
            let count = document.page_count();
            cover.pages = (count > 0).then_some(count);
            if let Some(page) = document.page(0) {
                let thumbnail = page.thumbnail(NSSize::new(COVER_WIDTH * 2., COVER_HEIGHT * 2.));
                cover.image = cover_image(&thumbnail);
                if cover.image.is_some() { cover.status = CoverStatus::Ready; }
            }
            cover
        });
        self.cache.retain(|old| old.key != key);
        self.cache.push_back(cover.clone());
        self.cards[index].status = cover.status;
        self.cards[index].pages = cover.pages;
        self.cards[index].button.setImage(Some(cover.image.as_deref().unwrap_or(&self.placeholder)));
        self.update_title(index);
        while self.cache.len() > CACHE_LIMIT {
            if let Some(removed) = self.cache.pop_front() {
                if removed.image.is_some() {
                    for card in &mut self.cards {
                        if card.key == removed.key {
                            card.button.setImage(Some(&self.placeholder));
                            card.status = CoverStatus::Pending;
                        }
                    }
                }
            }
        }
        true
    }

    fn context_menu(&self, id: &str) -> Retained<NSMenu> {
        let menu = NSMenu::new(self.mtm);
        for (tag, title) in [tr("Finderで表示", "Show in Finder"), tr("場所を指定し直す…", "Locate PDF…"), tr("本棚から外す…", "Remove from Library…")].into_iter().enumerate() {
            let item = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(NSMenuItem::alloc(self.mtm), &string(title), Some(sel!(bookAction:)), &string("")) };
            item.setTag(tag as isize);
            unsafe {
                item.setTarget(Some(&self.target));
                item.setRepresentedObject(Some(&*string(id)));
            }
            menu.addItem(&item);
        }
        menu
    }

    fn update_title(&self, index: usize) {
        let card = &self.cards[index];
        let detail = match card.status {
            CoverStatus::Missing => tr("ファイルが見つかりません", "File not found").into(),
            CoverStatus::Locked => if is_japanese() { format!("鍵付き · {}ページから", card.book.page.max(1)) } else { format!("Locked · page {}", card.book.page.max(1)) },
            CoverStatus::Unavailable => tr("表紙を表示できません", "Cover unavailable").into(),
            CoverStatus::Pending => tr("表紙を読み込み中…", "Loading cover…").into(),
            CoverStatus::Ready => {
                let count = card.pages.unwrap_or(1);
                let page = card.book.page.clamp(1, count);
                format!("{}{page} / {count}{} · {}%", if card.book.current { tr("読書中 · ", "Reading · ") } else { "" }, tr("ページ", " pages"), page.saturating_mul(100) / count)
            }
        };
        let title = two_line_title(&card.book.title);
        let text = format!("{title}\n{detail}");
        let attributed = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &string(&text));
        let full = NSRange::new(0, text.encode_utf16().count());
        let detail_range = NSRange::new(title.encode_utf16().count() + 1, detail.encode_utf16().count());
        unsafe {
            attributed.addAttribute_value_range(&string("NSFont"), &NSFont::systemFontOfSize(13.), full);
            attributed.addAttribute_value_range(&string("NSColor"), &NSColor::labelColor(), full);
            attributed.addAttribute_value_range(&string("NSFont"), &NSFont::systemFontOfSize(10.5), detail_range);
            attributed.addAttribute_value_range(&string("NSColor"), &NSColor::secondaryLabelColor(), detail_range);
            let paragraph = NSMutableParagraphStyle::new();
            paragraph.setAlignment(NSTextAlignment::Center);
            paragraph.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            paragraph.setLineSpacing(3.);
            attributed.addAttribute_value_range(&string("NSParagraphStyle"), &paragraph, full);
        }
        card.button.setAttributedTitle(&attributed);
        card.button.setToolTip(Some(&string(&format!("{}\n{detail}", card.book.title))));
        let accessible = if is_japanese() { format!("{}、{detail}、開く", card.book.title) } else { format!("{}, {detail}, open", card.book.title) };
        card.button.setAccessibilityLabel(Some(&string(&accessible)));
    }

    fn update_selection(&self) {
        for (index, card) in self.cards.iter().enumerate() {
            card.button.setAccessibilitySelected(Some(index) == self.selected);
            card.button.setWantsLayer(true);
            let color = if Some(index) == self.selected { NSColor::controlAccentColor().colorWithAlphaComponent(0.11) } else { NSColor::clearColor() };
            unsafe {
                let layer: Option<Retained<AnyObject>> = msg_send![&*card.button, layer];
                if let Some(layer) = layer {
                    let _: () = msg_send![&*layer, setCornerRadius: 10.0f64];
                    let cg: *const objc2_core_graphics::CGColor = msg_send![&*color, CGColor];
                    let _: () = msg_send![&*layer, setBackgroundColor: cg];
                }
            }
        }
    }

    fn top_offset(&self) -> f64 {
        let visible = self.scroll.documentVisibleRect();
        (self.content.frame().size.height - visible.origin.y - visible.size.height).max(0.)
    }
    fn scroll_to_top_offset(&self, offset: f64) {
        let clip = self.scroll.contentView();
        let maximum = (self.content.frame().size.height - clip.bounds().size.height).max(0.);
        clip.scrollToPoint(NSPoint::new(0., (maximum - offset).clamp(0., maximum)));
        self.scroll.reflectScrolledClipView(&clip);
    }
}

fn column_count(width: f64) -> usize { (((width - MARGIN * 2. + GAP) / (184. + GAP)).floor() as usize).clamp(1, 6) }
fn compact_card_height(viewport_height: f64) -> f64 { (viewport_height - MARGIN * 2.).clamp(224., CARD_HEIGHT) }
fn intersects(a: NSRect, b: NSRect) -> bool {
    a.origin.x < b.origin.x + b.size.width && a.origin.x + a.size.width > b.origin.x && a.origin.y < b.origin.y + b.size.height && a.origin.y + a.size.height > b.origin.y
}
fn two_line_title(title: &str) -> String {
    let clean = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut lines = vec![String::new()];
    let mut width = 0;
    for ch in clean.chars() {
        let advance = if ch.is_ascii() { 1 } else { 2 };
        if width + advance > 28 {
            if lines.len() == 2 { lines.last_mut().unwrap().push('…'); break; }
            lines.push(String::new()); width = 0;
        }
        lines.last_mut().unwrap().push(ch); width += advance;
    }
    if lines.len() == 1 { lines.push(String::new()); }
    lines.join("\n")
}

#[allow(deprecated)]
fn placeholder() -> Retained<NSImage> {
    let size = NSSize::new(COVER_WIDTH * 2., COVER_HEIGHT * 2.);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.lockFocus();
    NSColor::colorWithSRGBRed_green_blue_alpha(0.93, 0.93, 0.91, 1.).setFill();
    NSBezierPath::fillRect(rect(3., 3., size.width - 6., size.height - 6.));
    NSColor::colorWithSRGBRed_green_blue_alpha(0.82, 0.83, 0.80, 1.).setStroke();
    NSBezierPath::strokeRect(rect(3., 3., size.width - 6., size.height - 6.));
    if let Some(symbol) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&string("book.closed"), None) {
        symbol.drawInRect_fromRect_operation_fraction(rect(120., 190., 80., 80.), NSRect::new(NSPoint::new(0., 0.), symbol.size()), NSCompositingOperation::SourceOver, 0.22);
    }
    image.unlockFocus();
    image.setSize(NSSize::new(COVER_WIDTH, COVER_HEIGHT));
    image
}

#[allow(deprecated)]
fn cover_image(source: &NSImage) -> Option<Retained<NSImage>> {
    let original = source.size();
    if original.width <= 0. || original.height <= 0. { return None; }
    let size = NSSize::new(COVER_WIDTH * 2., COVER_HEIGHT * 2.);
    let scale = (size.width / original.width).min(size.height / original.height);
    let width = original.width * scale;
    let height = original.height * scale;
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.lockFocus();
    source.drawInRect_fromRect_operation_fraction(rect((size.width - width) / 2., 0., width, height), NSRect::new(NSPoint::new(0., 0.), original), NSCompositingOperation::SourceOver, 1.);
    image.unlockFocus();
    image.setSize(NSSize::new(COVER_WIDTH, COVER_HEIGHT));
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_and_wide_shelves_keep_cover_cards_inside_the_viewport() {
        for width in [320., 680., 900., 1160., 1600.] {
            let columns = column_count(width);
            let card_width = (width - MARGIN * 2. - GAP * (columns - 1) as f64) / columns as f64;
            assert!(card_width >= COVER_WIDTH);
            assert!((1..=6).contains(&columns));
            assert!(MARGIN * 2. + columns as f64 * card_width + GAP * (columns - 1) as f64 <= width + 0.001);
        }
    }

    #[test]
    fn offscreen_cover_work_excludes_pages_beyond_the_prefetch_row() {
        let viewport = rect(0., 1000., 900., 600.);
        let near = rect(0., viewport.origin.y - ROW_HEIGHT, 900., viewport.size.height + ROW_HEIGHT * 2.);
        assert!(intersects(rect(28., 1020., 184., CARD_HEIGHT), viewport));
        assert!(!intersects(rect(28., 690., 184., CARD_HEIGHT), viewport));
        assert!(intersects(rect(28., 690., 184., CARD_HEIGHT), near));
        assert!(!intersects(rect(28., 0., 184., CARD_HEIGHT), near));
    }

    #[test]
    fn minimum_window_keeps_the_first_cards_title_inside_the_visible_shelf() {
        for viewport_height in [296., 320., 360., 616.] {
            let card_height = compact_card_height(viewport_height);
            assert!(card_height + MARGIN * 2. <= viewport_height);
            assert!(card_height >= 224.);
            assert!(card_height <= CARD_HEIGHT);
        }
    }
}
