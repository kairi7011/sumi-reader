use sumi_native::i18n::{text as tr, is_japanese};
use std::{cell::Cell, collections::HashMap, time::{Duration, Instant}};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly};
use objc2_app_kit::{
    NSAccessibility, NSAppearanceCustomization, NSBorderType, NSButton, NSButtonType,
    NSCellImagePosition, NSColor, NSFont, NSImageScaling, NSLineBreakMode,
    NSScrollElasticity, NSScrollView, NSTextAlignment, NSView, NSWorkspace,
};
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};

const HOME_WIDTH: f64 = 44.;
const GAP: f64 = 4.;
const MIN_TAB_WIDTH: f64 = 100.;
const MAX_TAB_WIDTH: f64 = 170.;
const TRANSITION: Duration = Duration::from_millis(200);

fn string(value: &str) -> Retained<NSString> { NSString::from_str(value) }
fn rect(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(width.max(0.), height.max(0.)))
}

pub struct TabBackgroundState { interactive: Cell<bool> }
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = TabBackgroundState]
    struct TabBackground;
    impl TabBackground {
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, point: NSPoint) -> *mut NSView {
            if !self.ivars().interactive.get() { return std::ptr::null_mut(); }
            unsafe { msg_send![super(self), hitTest: point] }
        }
    }
);

struct TabButtons {
    id: String,
    title: String,
    background: Retained<TabBackground>,
    select: Retained<NSButton>,
    close: Retained<NSButton>,
    selected: bool,
}
impl TabButtons {
    fn set_interactive(&self, interactive: bool, target: &AnyObject) {
        self.background.ivars().interactive.set(interactive);
        self.select.setEnabled(interactive);self.close.setEnabled(interactive);
        self.select.setAccessibilityHidden(!interactive);self.close.setAccessibilityHidden(!interactive);
        unsafe {
            self.select.setTarget(interactive.then_some(target));self.close.setTarget(interactive.then_some(target));
            self.select.setAction(interactive.then_some(sel!(selectTab:)));
            self.close.setAction(interactive.then_some(sel!(closeTab:)));
        }
        if !interactive { self.select.setTag(-1);self.close.setTag(-1); }
    }
    fn localize(&self) {
        let tooltip = if is_japanese() { format!("{}を閉じる", self.title) } else { format!("Close {}", self.title) };
        let accessible = if is_japanese() { format!("{}のタブを閉じる", self.title) } else { format!("Close tab: {}", self.title) };
        self.close.setToolTip(Some(&string(&tooltip)));
        self.close.setAccessibilityLabel(Some(&string(&accessible)));
    }
    fn finish_animation(&self) {
        for view in [&*self.background as &NSView, &*self.select, &*self.close] { finish_animation(view); }
    }
}
struct ClosingTab { buttons: TabButtons, until: Instant }

pub struct NativeTabStrip {
    mtm: MainThreadMarker,
    target: Retained<AnyObject>,
    view: Retained<NSView>,
    home: Retained<NSButton>,
    scroll: Retained<NSScrollView>,
    content: Retained<NSView>,
    buttons: Vec<TabButtons>,
    closing: Vec<ClosingTab>,
    items: Vec<(String, String)>,
    active_id: Option<String>,
    size: NSSize,
    appearance: String,
    animations_enabled: bool,
    animation_until: Option<Instant>,
}

impl NativeTabStrip {
    pub fn new(mtm: MainThreadMarker, target: &AnyObject) -> Self {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 600., 38.));
        view.setAccessibilityElement(false);
        view.setAccessibilityLabel(Some(&string(tr("本棚と開いている本", "Library and open books"))));
        let home = unsafe { NSButton::buttonWithTitle_target_action(&string(""), Some(target), Some(sel!(selectTab:)), mtm) };
        configure_button(&home);home.setTag(0);
        home.setImage(crate::icons::icon("library").as_deref());home.setImagePosition(NSCellImagePosition::ImageOnly);
        home.setToolTip(Some(&string(tr("本棚", "Library"))));home.setAccessibilityLabel(Some(&string(tr("本棚", "Library"))));
        home.setAccessibilityIdentifier(Some(&string("sumi-tab-home")));view.addSubview(&home);
        let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(HOME_WIDTH + GAP, 0., 552., 38.));
        scroll.setBorderType(NSBorderType::NoBorder);scroll.setDrawsBackground(false);
        scroll.setHasVerticalScroller(false);scroll.setHasHorizontalScroller(true);scroll.setAutohidesScrollers(true);
        scroll.setVerticalScrollElasticity(NSScrollElasticity::None);scroll.setUsesPredominantAxisScrolling(true);
        scroll.setAccessibilityLabel(Some(&string(tr("開いている本", "Open books"))));
        // Overlay scrollers must not shrink the tab hit targets with always-visible system scrollbars.
        unsafe { let _: () = msg_send![&*scroll, setScrollerStyle: 1isize]; }
        let content = NSView::initWithFrame(NSView::alloc(mtm), rect(0., 0., 552., 38.));
        scroll.setDocumentView(Some(&content));view.addSubview(&scroll);
        let target = unsafe { Retained::retain(target as *const AnyObject as *mut AnyObject).expect("live tab target") };
        Self { mtm, target, view, home, scroll, content, buttons: Vec::new(), closing: Vec::new(), items: Vec::new(), active_id: None, size: NSSize::new(0., 0.), appearance: String::new(), animations_enabled: false, animation_until: None }
    }

    pub fn view(&self) -> &NSView { &self.view }
    /// Call after the initial saved session has been laid out; restoration has no entrance animation.
    pub fn enable_animations(&mut self) { self.animations_enabled = true; }

    pub fn refresh_language(&mut self) {
        self.view.setAccessibilityLabel(Some(&string(tr("本棚と開いている本", "Library and open books"))));
        self.home.setToolTip(Some(&string(tr("本棚", "Library"))));self.home.setAccessibilityLabel(Some(&string(tr("本棚", "Library"))));
        self.scroll.setAccessibilityLabel(Some(&string(tr("開いている本", "Open books"))));
        for buttons in &self.buttons { buttons.localize(); }
    }

    /// Core Animation advances the visuals; the timer only retires finished, inert controls.
    pub fn tick(&mut self) -> bool {
        let Some(until) = self.animation_until else { return false; };
        let now = Instant::now();
        let reduced = reduce_motion();
        let old_count = self.closing.len();
        self.closing.retain(|tab| {
            if reduced || now >= tab.until { tab.buttons.background.removeFromSuperview();false } else {true}
        });
        if reduced || now >= until {
            self.animation_until = None;
            finish_animation(&self.home);
            for buttons in &self.buttons { buttons.finish_animation(); }
        }
        if self.closing.len() != old_count {
            self.fit_content();
            return true;
        }
        false
    }

    pub fn show(&mut self, items: &[(String, String)], active_id: Option<&str>, width: f64, height: f64) {
        self.tick();
        let width = width.max(1.);let height = height.max(1.);
        let changed = self.items != items;
        let active_changed = self.active_id.as_deref() != active_id;
        let resized = self.size != NSSize::new(width, height);
        let appearance = self.view.effectiveAppearance().name().to_string();
        let theme_changed = self.appearance != appearance;
        if !changed && !active_changed && !resized && !theme_changed { return; }
        let animated = self.animations_enabled && !reduce_motion();
        let old_offset = self.scroll.contentView().bounds().origin.x;
        let layout = geometry(items.len(), width);
        let now = Instant::now();
        let mut animating = false;
        if changed { animating |= self.reconcile(items, layout, height, animated, now); }
        transaction(|| {
            self.view.setFrameSize(NSSize::new(width, height));
            self.home.setFrame(rect(2., 4., (HOME_WIDTH - 4.).min(width), height - 8.));
            self.scroll.setFrame(rect(HOME_WIDTH + GAP, 0., layout.viewport, height));
        });
        for (index, buttons) in self.buttons.iter_mut().enumerate() {
            let x = index as f64 * (layout.tab_width + GAP);
            animating |= animate_frame(&buttons.background, rect(x, 4., layout.tab_width, height - 8.), animated);
            animating |= animate_frame(&buttons.select, rect(7., 0., layout.tab_width - 33., height - 8.), animated);
            animating |= animate_frame(&buttons.close, rect(layout.tab_width - 23., (height - 26.) / 2., 18., 18.), animated);
            animating |= animate_opacity(&buttons.background, 1., animated);
            let selected = active_id == Some(buttons.id.as_str());
            if selected != buttons.selected || theme_changed {
                animating |= decorate(&buttons.background, selected, animated && selected != buttons.selected);
            }
            buttons.selected = selected;
            buttons.select.setAccessibilitySelected(selected);buttons.select.setState(if selected {1} else {0});
            let tint = if selected {NSColor::labelColor()} else {NSColor::secondaryLabelColor()};
            buttons.select.setContentTintColor(Some(&tint));buttons.close.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        }
        let home_selected = active_id.is_none();
        if active_changed || theme_changed { animating |= decorate(&self.home, home_selected, animated && active_changed); }
        self.home.setAccessibilitySelected(home_selected);self.home.setState(if home_selected {1} else {0});
        let home_tint = if home_selected {NSColor::labelColor()} else {NSColor::secondaryLabelColor()};
        self.home.setContentTintColor(Some(&home_tint));
        self.size = NSSize::new(width, height);self.appearance = appearance;
        self.fit_content();
        let active_index = active_id.and_then(|id| items.iter().position(|(item, _)| item == id));
        let closing_only_extent = (items.is_empty() && !self.closing.is_empty()).then(||self.content.frame().size.width);
        let next_offset = transition_offset(old_offset, layout, active_index, active_changed || resized, closing_only_extent);
        let clip = self.scroll.contentView();clip.scrollToPoint(NSPoint::new(next_offset, 0.));self.scroll.reflectScrolledClipView(&clip);
        self.active_id = active_id.map(str::to_owned);
        if animating { self.animation_until = Some(now + TRANSITION); }
    }

    fn fit_content(&self) {
        let layout = geometry(self.items.len(), self.size.width);
        let width = self.closing.iter().map(|tab| {
            let frame = presentation_frame(&tab.buttons.background);frame.origin.x + frame.size.width
        }).fold(layout.content, f64::max);
        transaction(|| self.content.setFrame(rect(0., 0., width, self.size.height)));
        let clip = self.scroll.contentView();
        let current = clip.bounds().origin.x;
        let offset = reveal_offset(current, Geometry {content:width,..layout}, None, false);
        if offset != current {clip.scrollToPoint(NSPoint::new(offset,0.));self.scroll.reflectScrolledClipView(&clip);}
        self.scroll.setHidden(self.items.is_empty() && self.closing.is_empty());
    }

    fn reconcile(&mut self, items: &[(String, String)], layout: Geometry, height: f64, animated: bool, now: Instant) -> bool {
        let live_ids: Vec<_> = self.buttons.iter().map(|tab|tab.id.as_str()).collect();
        let closing_ids: Vec<_> = self.closing.iter().map(|tab|tab.buttons.id.as_str()).collect();
        let next_ids: Vec<_> = items.iter().map(|(id,_)|id.as_str()).collect();
        let plan = reconcile_plan(&live_ids, &closing_ids, &next_ids);
        let mut live: Vec<_> = std::mem::take(&mut self.buttons).into_iter().map(Some).collect();
        let mut closing: Vec<_> = std::mem::take(&mut self.closing).into_iter().map(Some).collect();
        let mut animating = false;
        for (index, ((id,title), origin)) in items.iter().zip(plan).enumerate() {
            let mut buttons = match origin {
                Origin::Live(slot) => live[slot].take().unwrap(),
                Origin::Closing(slot) => closing[slot].take().unwrap().buttons,
                Origin::New => {
                    let buttons = self.create_tab(id, title);
                    let x = index as f64 * (layout.tab_width + GAP);
                    transaction(|| {
                        buttons.background.setFrame(rect(x, 4., if animated {1.} else {layout.tab_width}, height - 8.));
                        buttons.select.setFrame(rect(7., 0., layout.tab_width - 33., height - 8.));
                        buttons.close.setFrame(rect(layout.tab_width - 23., (height - 26.) / 2., 18., 18.));
                        buttons.background.setAlphaValue(if animated {0.} else {1.});
                    });
                    buttons
                }
            };
            buttons.set_interactive(true, &self.target);
            // Live actions always address the new order, including while old visuals are fading out.
            buttons.select.setTag(index as isize + 1);buttons.close.setTag(index as isize);
            if buttons.title != *title {
                buttons.title = title.clone();buttons.select.setTitle(&string(title));
                buttons.select.setToolTip(Some(&string(title)));buttons.select.setAccessibilityLabel(Some(&string(title)));
            }
            buttons.localize();self.buttons.push(buttons);
        }
        self.closing.extend(closing.into_iter().flatten());
        for buttons in live.into_iter().flatten() {
            buttons.set_interactive(false, &self.target);
            if animated {
                let frame = presentation_frame(&buttons.background);
                animate_frame(&buttons.background, rect(frame.origin.x, frame.origin.y, 1., frame.size.height), true);
                animate_opacity(&buttons.background, 0., true);
                self.closing.push(ClosingTab { buttons, until: now + TRANSITION });animating = true;
            } else { buttons.background.removeFromSuperview(); }
        }
        self.items = items.to_vec();
        animating
    }

    fn create_tab(&self, id: &str, title: &str) -> TabButtons {
        let allocated = TabBackground::alloc(self.mtm).set_ivars(TabBackgroundState { interactive: Cell::new(true) });
        let background: Retained<TabBackground> = unsafe { msg_send![super(allocated), initWithFrame: rect(0.,0.,MAX_TAB_WIDTH,30.)] };
        background.setAccessibilityElement(false);background.setWantsLayer(true);
        if let Some(layer) = layer(&background) { unsafe { let _: () = msg_send![&*layer, setMasksToBounds: true]; } }
        decorate(&background, false, false);
        let select = unsafe { NSButton::buttonWithTitle_target_action(&string(title), Some(&*self.target), Some(sel!(selectTab:)), self.mtm) };
        configure_button(&select);select.setAlignment(NSTextAlignment::Left);select.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        select.setToolTip(Some(&string(title)));select.setAccessibilityLabel(Some(&string(title)));
        select.setAccessibilityIdentifier(Some(&string(&format!("sumi-tab-{id}"))));background.addSubview(&select);
        let close = unsafe { NSButton::buttonWithTitle_target_action(&string(""), Some(&*self.target), Some(sel!(closeTab:)), self.mtm) };
        configure_button(&close);close.setImage(crate::icons::icon("x").as_deref());close.setImagePosition(NSCellImagePosition::ImageOnly);
        close.setAccessibilityIdentifier(Some(&string(&format!("sumi-tab-close-{id}"))));background.addSubview(&close);
        self.content.addSubview(&background);
        TabButtons { id:id.into(),title:title.into(),background,select,close,selected:false }
    }
}

fn configure_button(button: &NSButton) {
    button.setBordered(false);button.setButtonType(NSButtonType::MomentaryPushIn);
    button.setFont(Some(&NSFont::systemFontOfSize(12.)));button.setImageScaling(NSImageScaling::ScaleProportionallyDown);button.setWantsLayer(true);
}
fn reduce_motion() -> bool { NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() }
fn layer(view: &NSView) -> Option<Retained<AnyObject>> { unsafe { msg_send![view,layer] } }
fn presentation(layer: &AnyObject) -> Retained<AnyObject> {
    unsafe { let shown: Option<Retained<AnyObject>> = msg_send![layer,presentationLayer];shown.unwrap_or_else(||Retained::retain(layer as *const AnyObject as *mut AnyObject).unwrap()) }
}
fn presentation_frame(view: &NSView) -> NSRect {
    layer(view).map(|layer|unsafe{msg_send![&*presentation(&layer),frame]}).unwrap_or_else(||view.frame())
}
fn value(layer: &AnyObject, key: &str) -> Option<Retained<AnyObject>> { unsafe { msg_send![layer,valueForKey:&*string(key)] } }
fn transaction(action: impl FnOnce()) {
    unsafe {
        let class=AnyClass::get(c"CATransaction").expect("QuartzCore is linked");
        let _:()=msg_send![class,begin];let _:()=msg_send![class,setDisableActions:true];action();let _:()=msg_send![class,commit];
    }
}
fn animate_value(layer: &AnyObject, key: &str, from: Option<Retained<AnyObject>>, animated: bool) -> bool {
    let name = string(&format!("sumi-tab-{key}"));
    let to = value(layer,key);
    unsafe {
        let different = match (&from,&to) { (Some(a),Some(b)) => { let equal:bool=msg_send![&**a,isEqual:&**b];!equal }, _=>false };
        let _:()=msg_send![layer,removeAnimationForKey:&*name];
        if !animated || !different {return false;}
        let class=AnyClass::get(c"CABasicAnimation").expect("QuartzCore is linked");
        let animation:Retained<AnyObject>=msg_send![class,animationWithKeyPath:&*string(key)];
        let _:()=msg_send![&*animation,setFromValue:from.as_deref()];let _:()=msg_send![&*animation,setToValue:to.as_deref()];
        let _:()=msg_send![&*animation,setDuration:TRANSITION.as_secs_f64()];
        let timing:Retained<AnyObject>=msg_send![AnyClass::get(c"CAMediaTimingFunction").unwrap(),functionWithName:&*string("easeOut")];
        let _:()=msg_send![&*animation,setTimingFunction:&*timing];
        let _:()=msg_send![layer,addAnimation:&*animation,forKey:&*name];true
    }
}
fn animate_frame(view: &NSView, target: NSRect, animated: bool) -> bool {
    if view.frame()==target {return false;}
    let Some(layer)=layer(view) else {view.setFrame(target);return false;};
    let shown=presentation(&layer);let position=value(&shown,"position");let bounds=value(&shown,"bounds");
    transaction(||view.setFrame(target));
    let position=animate_value(&layer,"position",position,animated);let bounds=animate_value(&layer,"bounds",bounds,animated);
    position||bounds
}
fn animate_opacity(view: &NSView, target: f64, animated: bool) -> bool {
    if view.alphaValue()==target {return false;}
    let Some(layer)=layer(view) else {view.setAlphaValue(target);return false;};
    let from=value(&presentation(&layer),"opacity");transaction(||view.setAlphaValue(target));animate_value(&layer,"opacity",from,animated)
}
fn decorate(view: &NSView, selected: bool, animated: bool) -> bool {
    view.setWantsLayer(true);
    let Some(layer)=layer(view)else{return false;};let from=value(&presentation(&layer),"backgroundColor");
    let color=if selected {NSColor::controlAccentColor().colorWithAlphaComponent(0.13)}else{NSColor::clearColor()};
    transaction(||unsafe{
        let _:()=msg_send![&*layer,setCornerRadius:7.0f64];
        let cg:*const objc2_core_graphics::CGColor=msg_send![&*color,CGColor];let _:()=msg_send![&*layer,setBackgroundColor:cg];
    });
    animate_value(&layer,"backgroundColor",from,animated)
}
fn finish_animation(view: &NSView) {
    if let Some(layer)=layer(view) {unsafe{for key in ["position","bounds","opacity","backgroundColor"] {let _:()=msg_send![&*layer,removeAnimationForKey:&*string(&format!("sumi-tab-{key}"))];}}}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin { Live(usize), Closing(usize), New }
fn reconcile_plan(live: &[&str], closing: &[&str], next: &[&str]) -> Vec<Origin> {
    let mut available: HashMap<_,_> = closing.iter().enumerate().map(|(index,id)|(*id,Origin::Closing(index))).collect();
    for (index,id) in live.iter().enumerate() {available.insert(*id,Origin::Live(index));}
    next.iter().map(|id|available.remove(id).unwrap_or(Origin::New)).collect()
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

fn transition_offset(current: f64, geometry: Geometry, active: Option<usize>, reveal: bool, closing_only_extent: Option<f64>) -> f64 {
    // Closing every tab must fade the currently visible tabs, not jump to the first ghosts.
    let geometry = Geometry {content:closing_only_extent.unwrap_or(geometry.content),..geometry};
    reveal_offset(current, geometry, active, reveal)
}

#[cfg(test)]
mod tests {
    use super::{geometry, reconcile_plan, reveal_offset, transition_offset, Origin, GAP, HOME_WIDTH, MIN_TAB_WIDTH, MAX_TAB_WIDTH};
    #[test]
    fn closing_all_overflowed_tabs_holds_the_visible_end_until_retirement() {
        for width in [280.,472.,620.] {
            let opened = geometry(14,width);
            let offset = reveal_offset(0.,opened,Some(13),true);
            assert!(offset > 0.);
            let empty = geometry(0,width);
            assert_eq!(transition_offset(offset,empty,None,true,Some(opened.content)),offset);
            assert_eq!(transition_offset(offset,empty,None,true,None),0.);
        }
    }
    #[test]
    fn closing_a_middle_tab_preserves_neighbor_controls_in_the_new_action_order() {
        assert_eq!(reconcile_plan(&["first","closed","last"], &[], &["first","last","added"]),
            vec![Origin::Live(0),Origin::Live(2),Origin::New]);
    }
    #[test]
    fn reopening_before_a_close_finishes_reuses_its_visual_instead_of_duplicating_it() {
        assert_eq!(reconcile_plan(&["first"], &["returning","removed"], &["returning","first"]),
            vec![Origin::Closing(0),Origin::Live(0)]);
        assert_eq!(reconcile_plan(&["first","returning"], &[], &["returning","first"]),
            vec![Origin::Live(1),Origin::Live(0)]);
    }
    #[test]
    fn restoring_or_closing_every_tab_does_not_reuse_an_unrelated_control() {
        assert_eq!(reconcile_plan(&[], &[], &["first","second"]),vec![Origin::New,Origin::New]);
        assert!(reconcile_plan(&["first","second"], &[], &[]).is_empty());
        assert_eq!(reconcile_plan(&[], &["first","second"], &["third"]),vec![Origin::New]);
    }
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
