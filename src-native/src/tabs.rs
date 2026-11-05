use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSBorderType, NSButton, NSButtonType, NSCellImagePosition, NSColor,
    NSFont, NSImageScaling, NSLineBreakMode, NSScrollElasticity, NSScrollView,
    NSTextAlignment, NSView,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

const HOME_WIDTH: f64 = 44.;
const GAP: f64 = 4.;
const MIN_TAB_WIDTH: f64 = 100.;
const MAX_TAB_WIDTH: f64 = 170.;

fn string(value: &str) -> Retained<NSString> { NSString::from_str(value) }
fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width.max(0.), height.max(0.)))
}

struct TabButtons {
    background: Retained<NSView>,
    select: Retained<NSButton>,
    close: Retained<NSButton>,
}

pub struct NativeTabStrip {
    mtm: MainThreadMarker,
    target: Retained<AnyObject>,
    view: Retained<NSView>,
    home: Retained<NSButton>,
    scroll: Retained<NSScrollView>,
    content: Retained<NSView>,
    buttons: Vec<TabButtons>,
    items: Vec<(String, String)>,
    active_id: Option<String>,
    size: NSSize,
}

impl NativeTabStrip {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 600., 38.));
        view.setAccessibilityElement(false);
        view.setAccessibilityLabel(Some(&string("本棚と開いている本")));

        let home = unsafe {
            NSButton::buttonWithTitle_target_action(&string(""), Some(target), Some(sel!(selectTab:)), mtm)
        };
        configure_button(&home);
        home.setTag(0);
        home.setImage(crate::icons::icon("library").as_deref());
        home.setImagePosition(NSCellImagePosition::ImageOnly);
        home.setToolTip(Some(&string("本棚")));
        home.setAccessibilityLabel(Some(&string("本棚")));
        home.setAccessibilityIdentifier(Some(&string("sumi-tab-home")));
        view.addSubview(&home);

        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(HOME_WIDTH + GAP, 0., 552., 38.));
        scroll.setBorderType(NSBorderType::NoBorder);
        scroll.setDrawsBackground(false);
        scroll.setHasVerticalScroller(false);
        scroll.setHasHorizontalScroller(true);
        scroll.setAutohidesScrollers(true);
        scroll.setVerticalScrollElasticity(NSScrollElasticity::None);
        scroll.setUsesPredominantAxisScrolling(true);
        scroll.setAccessibilityLabel(Some(&string("開いている本")));
        // Overlay scrollers keep a user's always-visible-scrollbar setting from shrinking the tab hit targets.
        unsafe { let _: () = msg_send![&*scroll, setScrollerStyle: 1isize]; }
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 552., 38.));
        scroll.setDocumentView(Some(&content));
        view.addSubview(&scroll);
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live tab target") };
        Self { mtm, target, view, home, scroll, content, buttons: Vec::new(), items: Vec::new(), active_id: None, size: NSSize::new(0., 0.) }
    }

    pub fn view(&self) -> &NSView { &self.view }

    pub fn show(&mut self, items: &[(String, String)], active_id: Option<&str>, width: f64, height: f64) {
        let width = width.max(1.);
        let height = height.max(1.);
        let changed = self.items != items;
        let active_changed = self.active_id.as_deref() != active_id;
        let resized = self.size != NSSize::new(width, height);
        let old_offset = self.scroll.contentView().bounds().origin.x;
        if changed { self.rebuild(items); }
        let geometry = geometry(items.len(), width);
        self.view.setFrameSize(NSSize::new(width, height));
        self.home.setFrame(rect(2., 4., (HOME_WIDTH - 4.).min(width), height - 8.));
        self.scroll.setFrame(rect(HOME_WIDTH + GAP, 0., geometry.viewport, height));
        self.scroll.setHidden(items.is_empty());
        self.content.setFrame(rect(0., 0., geometry.content, height));
        for (index, buttons) in self.buttons.iter().enumerate() {
            let x = index as f64 * (geometry.tab_width + GAP);
            buttons.background.setFrame(rect(x, 4., geometry.tab_width, height - 8.));
            buttons.select.setFrame(rect(7., 0., geometry.tab_width - 33., height - 8.));
            buttons.close.setFrame(rect(geometry.tab_width - 23., (height - 26.) / 2., 18., 18.));
            let selected = active_id == Some(items[index].0.as_str());
            decorate(&buttons.background, selected);
            buttons.select.setAccessibilitySelected(selected);
            buttons.select.setState(if selected { 1 } else { 0 });
            let tint = if selected { NSColor::labelColor() } else { NSColor::secondaryLabelColor() };
            buttons.select.setContentTintColor(Some(&tint));
            buttons.close.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        }
        let home_selected = active_id.is_none();
        decorate(&self.home, home_selected);
        self.home.setAccessibilitySelected(home_selected);
        self.home.setState(if home_selected { 1 } else { 0 });
        let tint = if home_selected { NSColor::labelColor() } else { NSColor::secondaryLabelColor() };
        self.home.setContentTintColor(Some(&tint));

        let active_index = active_id.and_then(|id| items.iter().position(|(item, _)| item == id));
        let next_offset = reveal_offset(old_offset, geometry, active_index, active_changed || resized);
        let clip = self.scroll.contentView();
        clip.scrollToPoint(NSPoint::new(next_offset, 0.));
        self.scroll.reflectScrolledClipView(&clip);
        self.active_id = active_id.map(str::to_owned);
        self.size = NSSize::new(width, height);
    }

    fn rebuild(&mut self, items: &[(String, String)]) {
        for buttons in self.buttons.drain(..) { buttons.background.removeFromSuperview(); }
        self.items = items.to_vec();
        for (index, (id, title)) in items.iter().enumerate() {
            let background = NSView::initWithFrame(NSView::alloc(self.mtm), rect(0., 0., MAX_TAB_WIDTH, 30.));
            background.setAccessibilityElement(false);
            let select = unsafe {
                NSButton::buttonWithTitle_target_action(&string(title), Some(&*self.target), Some(sel!(selectTab:)), self.mtm)
            };
            configure_button(&select);
            select.setTag(index as isize + 1);
            select.setAlignment(NSTextAlignment::Left);
            select.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            select.setToolTip(Some(&string(title)));
            select.setAccessibilityLabel(Some(&string(title)));
            select.setAccessibilityIdentifier(Some(&string(&format!("sumi-tab-{id}"))));
            background.addSubview(&select);

            let close = unsafe {
                NSButton::buttonWithTitle_target_action(&string(""), Some(&*self.target), Some(sel!(closeTab:)), self.mtm)
            };
            configure_button(&close);
            close.setTag(index as isize);
            close.setImage(crate::icons::icon("x").as_deref());
            close.setImagePosition(NSCellImagePosition::ImageOnly);
            close.setToolTip(Some(&string(&format!("{title}を閉じる"))));
            close.setAccessibilityLabel(Some(&string(&format!("{title}のタブを閉じる"))));
            close.setAccessibilityIdentifier(Some(&string(&format!("sumi-tab-close-{id}"))));
            background.addSubview(&close);
            self.content.addSubview(&background);
            self.buttons.push(TabButtons { background, select, close });
        }
    }
}

fn configure_button(button: &NSButton) {
    button.setBordered(false);
    button.setButtonType(NSButtonType::MomentaryPushIn);
    button.setFont(Some(&NSFont::systemFontOfSize(12.)));
    button.setImageScaling(NSImageScaling::ScaleProportionallyDown);
}

fn decorate(view: &NSView, selected: bool) {
    view.setWantsLayer(true);
    let color = if selected { NSColor::controlAccentColor().colorWithAlphaComponent(0.13) } else { NSColor::clearColor() };
    unsafe {
        let layer: Option<Retained<AnyObject>> = msg_send![view, layer];
        if let Some(layer) = layer {
            let _: () = msg_send![&*layer, setCornerRadius: 7.0f64];
            let cg: *const objc2_core_graphics::CGColor = msg_send![&*color, CGColor];
            let _: () = msg_send![&*layer, setBackgroundColor: cg];
        }
    }
}

#[derive(Clone, Copy)]
struct Geometry { viewport: f64, tab_width: f64, content: f64 }
fn geometry(count: usize, width: f64) -> Geometry {
    let viewport = (width - HOME_WIDTH - GAP).max(0.);
    let gaps = count.saturating_sub(1) as f64 * GAP;
    let tab_width = if count == 0 { MIN_TAB_WIDTH } else { ((viewport - gaps) / count as f64).clamp(MIN_TAB_WIDTH, MAX_TAB_WIDTH) };
    let content = (count as f64 * tab_width + gaps).max(viewport);
    Geometry { viewport, tab_width, content }
}

fn reveal_offset(current: f64, geometry: Geometry, active: Option<usize>, reveal: bool) -> f64 {
    let mut offset = current;
    if reveal {
        if let Some(index) = active {
            let left = index as f64 * (geometry.tab_width + GAP);
            let right = left + geometry.tab_width;
            if left < offset { offset = left; }
            else if right > offset + geometry.viewport { offset = right - geometry.viewport; }
        }
    }
    offset.clamp(0., (geometry.content - geometry.viewport).max(0.))
}

#[cfg(test)]
mod tests {
    use super::{geometry, reveal_offset, GAP, HOME_WIDTH, MIN_TAB_WIDTH, MAX_TAB_WIDTH};
    #[test]
    fn home_keeps_its_width_and_book_tabs_do_not_expand_to_fill_the_header() {
        let layout = geometry(2, 900.);
        assert_eq!(layout.viewport, 900. - HOME_WIDTH - GAP);
        assert_eq!(layout.tab_width, MAX_TAB_WIDTH);
        assert_eq!(layout.content, layout.viewport);
        assert_eq!(geometry(0, 900.).content, layout.viewport);
    }
    #[test]
    fn overflowing_tabs_stay_readable_and_the_selected_tab_becomes_visible() {
        let layout = geometry(12, 620.);
        assert_eq!(layout.tab_width, MIN_TAB_WIDTH);
        assert!(layout.content > layout.viewport);
        let offset = reveal_offset(0., layout, Some(11), true);
        let left = 11. * (layout.tab_width + GAP);
        assert!(offset <= left);
        assert!(left + layout.tab_width <= offset + layout.viewport + 0.001);
        assert_eq!(offset, layout.content - layout.viewport);
    }
    #[test]
    fn a_theme_or_layout_refresh_preserves_user_scrolling_until_selection_changes() {
        let layout = geometry(12, 620.);
        assert_eq!(reveal_offset(300., layout, Some(0), false), 300.);
        assert_eq!(reveal_offset(300., layout, Some(0), true), 0.);
        let narrowed = geometry(12, 450.);
        let offset = reveal_offset(300., narrowed, Some(8), true);
        let right = 8. * (narrowed.tab_width + GAP) + narrowed.tab_width;
        assert!(right <= offset + narrowed.viewport + 0.001);
    }
}
