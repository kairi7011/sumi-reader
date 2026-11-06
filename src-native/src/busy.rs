use std::{cell::RefCell, marker::PhantomData, rc::Rc, time::Duration};
use block2::RcBlock;
use objc2::{msg_send, MainThreadMarker};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::{NSAppearanceCustomization, NSColor, NSView};
use objc2_core_graphics::{CGColor, CGMutablePath, CGPath};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" { fn CACurrentMediaTime() -> f64; }

const REVEAL_DELAY: f64 = 0.4;
const REVEAL_DURATION: f64 = 0.08;
const DISMISS_DURATION: f64 = 0.12;
const SPIN_DURATION: f64 = 0.8;
const OPACITY: f64 = 0.85;

#[derive(Clone, Copy)]
struct Visibility {
    reveal_at: f64,
    from_opacity: f64,
    spin_origin: f64,
    fade: Option<(f64, f64)>,
}
impl Visibility {
    fn begin(now: f64, elapsed: Duration, previous: Option<Self>) -> Self {
        let previous_opacity = previous.map_or(0., |visual| visual.opacity(now));
        let continuing = previous_opacity > 0.;
        let reveal_at = now + if continuing { 0. } else { (REVEAL_DELAY - elapsed.as_secs_f64()).max(0.) };
        Self {
            reveal_at,
            from_opacity: previous_opacity,
            spin_origin: if continuing { previous.unwrap().spin_origin } else { reveal_at },
            fade: None,
        }
    }
    fn opacity(self, now: f64) -> f64 {
        if let Some((start, opacity)) = self.fade {
            return opacity * (1. - (now - start) / DISMISS_DURATION).clamp(0., 1.);
        }
        if now < self.reveal_at { return 0.; }
        self.from_opacity + (OPACITY - self.from_opacity) * ((now - self.reveal_at) / REVEAL_DURATION).clamp(0., 1.)
    }
    fn rotation(self, now: f64) -> f64 {
        -std::f64::consts::TAU * ((now - self.spin_origin).max(0.) / SPIN_DURATION).rem_euclid(1.)
    }
    fn finish(self, now: f64) -> Option<Self> {
        let opacity = self.opacity(now);
        (opacity > 0.).then_some(Self { fade: Some((now, opacity)), ..self })
    }
}

#[derive(Default)]
struct Scopes { depth: usize, generation: u64 }
impl Scopes {
    fn enter(&mut self) -> (u64, bool) {
        let first = self.depth == 0;
        if first { self.generation = self.generation.wrapping_add(1); }
        self.depth += 1;
        (self.generation, first)
    }
    fn leave(&mut self, generation: u64) -> bool {
        if self.depth == 0 || self.generation != generation { return false; }
        self.depth -= 1;
        self.depth == 0
    }
    fn reset(&mut self) { self.depth = 0; self.generation = self.generation.wrapping_add(1); }
}

#[derive(Default)]
struct State {
    root: Option<Retained<NSView>>,
    layer: Option<Retained<AnyObject>>,
    size: Option<(f64, f64)>,
    scopes: Scopes,
    visibility: Option<Visibility>,
    positioned_for_operation: bool,
}

thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }

pub struct Guard { generation: Option<u64>, _main_thread: PhantomData<Rc<()>> }

pub fn install(root: &NSView) {
    if MainThreadMarker::new().is_none() { return; }
    root.setWantsLayer(true);
    unsafe {
        let parent: Option<Retained<AnyObject>> = msg_send![root, layer];
        let Some(parent) = parent else { return; };
        let Some(class) = AnyClass::get(c"CAShapeLayer") else { return; };
        let layer: Retained<AnyObject> = msg_send![class, layer];
        let path = CGMutablePath::new();
        CGMutablePath::add_arc(Some(&path), std::ptr::null(), 8., 8., 6.3, 0., std::f64::consts::PI * 1.5, false);
        let pointer = (&*path as *const CGMutablePath).cast::<CGPath>();
        let _: () = msg_send![&*layer, setPath: pointer];
        let _: () = msg_send![&*layer, setBounds: NSRect::new(NSPoint::new(0., 0.), NSSize::new(16., 16.))];
        let _: () = msg_send![&*layer, setFillColor: std::ptr::null::<CGColor>()];
        let _: () = msg_send![&*layer, setLineWidth: 1.6f64];
        let _: () = msg_send![&*layer, setLineCap: &*NSString::from_str("round")];
        let _: () = msg_send![&*layer, setContentsScale: root.window().map(|window| window.backingScaleFactor()).unwrap_or(2.)];
        let _: () = msg_send![&*layer, setOpacity: 0f32];
        // A layer has no responder or hit-test region. Keep it above PDFView's
        // layer-backed descendants without hiding the current page or taking focus.
        let _: () = msg_send![&*layer, setZPosition: 10_000f64];
        let _: () = msg_send![&*parent, addSublayer: &*layer];
        let previous = STATE.with(|slot| {
            let mut state = slot.borrow_mut();
            state.scopes.reset(); state.size = None; state.visibility = None; state.positioned_for_operation = false;
            state.root = Retained::retain(root as *const NSView as *mut NSView);
            state.layer.replace(layer)
        });
        if let Some(previous) = previous { let _: () = msg_send![&*previous, removeFromSuperlayer]; }
    }
    let size = root.bounds().size;
    layout(size.width, size.height);
}

pub fn layout(width: f64, height: f64) {
    if MainThreadMarker::new().is_none() || !width.is_finite() || !height.is_finite() { return; }
    let layer = STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        if state.size == Some((width, height)) { return None; }
        state.size = Some((width, height));
        if state.positioned_for_operation { None } else { state.layer.clone() }
    });
    if let Some(layer) = layer { unsafe {
        transaction(|| { let _: () = msg_send![&*layer, setPosition: default_position(width, height)]; });
    } }
}

pub fn begin() -> Guard {
    begin_with_elapsed(Duration::ZERO)
}

pub fn begin_with_elapsed(elapsed: Duration) -> Guard {
    begin_scoped(elapsed, None)
}

pub fn begin_at_with_elapsed(elapsed: Duration, position: NSPoint) -> Guard {
    begin_scoped(elapsed, Some(position))
}

fn default_position(width: f64, height: f64) -> NSPoint {
    NSPoint::new((width - 22.).max(8.), 24f64.min((height - 8.).max(8.)))
}

fn begin_scoped(elapsed: Duration, position: Option<NSPoint>) -> Guard {
    let inactive = || Guard { generation: None, _main_thread: PhantomData };
    if MainThreadMarker::new().is_none() { return inactive(); }
    let state = STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let layer = state.layer.clone()?;
        let root = state.root.clone()?;
        let (generation, first) = state.scopes.enter();
        let setup = if first {
            let now = unsafe { CACurrentMediaTime() };
            let visual = Visibility::begin(now, elapsed, state.visibility);
            state.visibility = Some(visual);
            state.positioned_for_operation = position.is_some();
            let (width, height) = state.size.unwrap_or((0., 0.));
            Some((visual, now, position.unwrap_or_else(|| default_position(width, height))))
        } else { None };
        Some((layer, root, generation, setup))
    });
    let Some((layer, root, generation, setup)) = state else { return inactive(); };
    if let Some((visual, now, position)) = setup { unsafe { arm(&layer, &root, visual, now, position); } }
    Guard { generation: Some(generation), _main_thread: PhantomData }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Some(generation) = self.generation else { return; };
        let finished = STATE.with(|slot| {
            let mut state = slot.borrow_mut();
            if !state.scopes.leave(generation) { return None; }
            let now = unsafe { CACurrentMediaTime() };
            state.visibility = state.visibility.and_then(|visual| visual.finish(now));
            state.layer.clone().map(|layer| (layer, state.visibility, now))
        });
        if let Some((layer, visual, now)) = finished { unsafe {
            transaction(|| {
                // The zero model opacity and finite animations need no deferred
                // cleanup that could hide a newer operation after a rapid restart.
                let _: () = msg_send![&*layer, removeAllAnimations];
                let _: () = msg_send![&*layer, setOpacity: 0f32];
                if let Some(visual) = visual {
                    let time: f64 = msg_send![&*layer, convertTime: now, fromLayer: std::ptr::null::<AnyObject>()];
                    let opacity = basic("opacity", visual.opacity(now), 0., time, DISMISS_DURATION);
                    let _: () = msg_send![&*layer, addAnimation: &*opacity, forKey: &*NSString::from_str("sumi-busy-dismiss")];
                    let phase = visual.rotation(now);
                    let spin = basic("transform.rotation.z", phase, phase - std::f64::consts::TAU * DISMISS_DURATION / SPIN_DURATION, time, DISMISS_DURATION);
                    let _: () = msg_send![&*layer, addAnimation: &*spin, forKey: &*NSString::from_str("sumi-busy-spin")];
                }
            });
        } }
    }
}

unsafe fn transaction(action: impl FnOnce()) {
    let class = AnyClass::get(c"CATransaction").expect("QuartzCore is linked");
    let _: () = msg_send![class, begin];
    let _: () = msg_send![class, setDisableActions: true];
    action();
    let _: () = msg_send![class, commit];
}

unsafe fn basic(path: &str, from: f64, to: f64, time: f64, duration: f64) -> Retained<AnyObject> {
    let class = AnyClass::get(c"CABasicAnimation").expect("QuartzCore is linked");
    let animation: Retained<AnyObject> = msg_send![class, animationWithKeyPath: &*NSString::from_str(path)];
    let _: () = msg_send![&*animation, setFromValue: &*NSNumber::new_f64(from)];
    let _: () = msg_send![&*animation, setToValue: &*NSNumber::new_f64(to)];
    let _: () = msg_send![&*animation, setBeginTime: time];
    let _: () = msg_send![&*animation, setDuration: duration];
    let linear: Retained<AnyObject> = msg_send![AnyClass::get(c"CAMediaTimingFunction").unwrap(), functionWithName: &*NSString::from_str("linear")];
    let _: () = msg_send![&*animation, setTimingFunction: &*linear];
    animation
}

unsafe fn arm(layer: &AnyObject, root: &NSView, visual: Visibility, now: f64, position: NSPoint) {
    let time: f64 = msg_send![layer, convertTime: now, fromLayer: std::ptr::null::<AnyObject>()];
    let reveal = time + (visual.reveal_at - now).max(0.);
    transaction(|| {
        root.effectiveAppearance().performAsCurrentDrawingAppearance(&RcBlock::new(|| {
            let color: *const CGColor = msg_send![&*NSColor::labelColor(), CGColor];
            let _: () = msg_send![layer, setStrokeColor: color];
        }));
        let _: () = msg_send![layer, setPosition: position];
        let _: () = msg_send![layer, setOpacity: 0f32];
        let _: () = msg_send![layer, removeAllAnimations];
        let opacity = basic("opacity", visual.from_opacity, OPACITY, reveal, REVEAL_DURATION);
        let _: () = msg_send![&*opacity, setFillMode: &*NSString::from_str("forwards")];
        let _: () = msg_send![&*opacity, setRemovedOnCompletion: false];
        let _: () = msg_send![layer, addAnimation: &*opacity, forKey: &*NSString::from_str("sumi-busy-visible")];
        let phase = visual.rotation(now);
        let spin = basic("transform.rotation.z", phase, phase - std::f64::consts::TAU, reveal, SPIN_DURATION);
        let _: () = msg_send![&*spin, setRepeatCount: f32::INFINITY];
        let _: () = msg_send![layer, addAnimation: &*spin, forKey: &*NSString::from_str("sumi-busy-spin")];
    });
    // Only a real blocking-operation boundary may flush. Once submitted, these
    // timed animations do not need the main run loop to reach the 400 ms mark.
    // https://developer.apple.com/documentation/quartzcore/catransaction/flush()
    let _: () = msg_send![AnyClass::get(c"CATransaction").unwrap(), flush];
    #[cfg(debug_assertions)]
    crate::activity::trace_event("busy-armed");
}

#[cfg(debug_assertions)]
pub fn debug_block_once() {
    thread_local! { static USED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if MainThreadMarker::new().is_none() { return; }
    let isolated = std::env::var_os("SUMI_LIBRARY_PATH").is_some_and(|path| std::path::Path::new(&path).components().any(|part| part.as_os_str() == "test-artifacts"));
    let delay = std::env::var("SUMI_NATIVE_BLOCK_DELAY_MS").ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0).min(10_000);
    if !isolated || delay == 0 || USED.with(|used| used.replace(true)) { return; }
    let _busy = begin();
    crate::activity::trace_event("blocking-start");
    std::thread::sleep(std::time::Duration::from_millis(delay));
    crate::activity::trace_event("blocking-end");
}

#[cfg(test)]
mod tests {
    use super::{Scopes, Visibility, DISMISS_DURATION, OPACITY};
    use std::time::Duration;
    fn close(actual: f64, expected: f64) { assert!((actual - expected).abs() < 0.000_001, "{actual} != {expected}"); }
    #[test]
    fn main_thread_handoff_keeps_the_original_four_hundred_millisecond_deadline() {
        let visual = Visibility::begin(10., Duration::from_millis(301), None);
        close(visual.reveal_at, 10.099);
        close(visual.opacity(10.098), 0.);
        assert!(visual.opacity(10.110) > 0.);
        let overdue = Visibility::begin(10., Duration::from_millis(800), None);
        close(overdue.reveal_at, 10.);
    }
    #[test]
    fn fast_completion_cancels_the_unseen_ring_without_a_later_flash() {
        let visual = Visibility::begin(0., Duration::ZERO, None);
        assert!(visual.finish(0.399).is_none());
        let next = Visibility::begin(0.5, Duration::ZERO, visual.finish(0.399));
        close(next.opacity(0.89), 0.);
        close(next.reveal_at, 0.9);
    }
    #[test]
    fn visible_completion_fades_for_one_hundred_twenty_milliseconds() {
        let visual = Visibility::begin(0., Duration::ZERO, None);
        let fade = visual.finish(0.8).unwrap();
        close(fade.opacity(0.8), OPACITY);
        close(fade.opacity(0.8 + DISMISS_DURATION / 2.), OPACITY / 2.);
        close(fade.opacity(0.8 + DISMISS_DURATION), 0.);
        close(fade.opacity(1.5), 0.);
        let short = visual.finish(0.42).unwrap();
        close(short.opacity(0.42), visual.opacity(0.42));
        assert!(short.opacity(0.48) > 0.);
        close(short.opacity(0.54), 0.);
    }
    #[test]
    fn restart_during_dismissal_preserves_opacity_and_spin_without_an_old_deadline() {
        let fading = Visibility::begin(0., Duration::ZERO, None).finish(0.8).unwrap();
        let resumed = Visibility::begin(0.86, Duration::ZERO, Some(fading));
        close(resumed.reveal_at, 0.86);
        close(resumed.opacity(0.86), fading.opacity(0.86));
        close(resumed.rotation(0.86), fading.rotation(0.86));
        close(resumed.opacity(1.), OPACITY);
        let fresh = Visibility::begin(1., Duration::ZERO, Some(fading));
        close(fresh.reveal_at, 1.4);
    }
    #[test]
    fn nested_work_keeps_one_deadline_until_the_final_guard_finishes() {
        let mut scopes = Scopes::default();
        let (outer, first) = scopes.enter(); assert!(first);
        let (inner, first) = scopes.enter(); assert!(!first); assert_eq!(outer, inner);
        assert!(!scopes.leave(outer)); assert!(scopes.leave(inner));
        assert!(!scopes.leave(inner));
    }
    #[test]
    fn old_guard_cannot_hide_a_reinstalled_or_later_busy_indicator() {
        let mut scopes = Scopes::default();
        let (old, _) = scopes.enter(); scopes.reset();
        let (current, first) = scopes.enter(); assert!(first); assert_ne!(old, current);
        assert!(!scopes.leave(old)); assert_eq!(scopes.depth, 1); assert!(scopes.leave(current));
    }
}
