use std::{cell::RefCell, marker::PhantomData, rc::Rc};
use block2::RcBlock;
use objc2::{msg_send, MainThreadMarker};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2_app_kit::{NSAppearanceCustomization, NSColor, NSView};
use objc2_core_graphics::{CGColor, CGMutablePath, CGPath};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" { fn CACurrentMediaTime() -> f64; }

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
            state.scopes.reset(); state.size = None;
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
        state.layer.clone()
    });
    if let Some(layer) = layer { unsafe {
        transaction(|| { let _: () = msg_send![&*layer, setPosition: NSPoint::new((width - 22.).max(8.), 24f64.min((height - 8.).max(8.)))]; });
    } }
}

pub fn begin() -> Guard {
    let inactive = || Guard { generation: None, _main_thread: PhantomData };
    if MainThreadMarker::new().is_none() { return inactive(); }
    let state = STATE.with(|slot| {
        let mut state = slot.borrow_mut();
        let layer = state.layer.clone()?;
        let root = state.root.clone()?;
        let (generation, first) = state.scopes.enter();
        Some((layer, root, generation, first))
    });
    let Some((layer, root, generation, first)) = state else { return inactive(); };
    if first { unsafe { arm(&layer, &root); } }
    Guard { generation: Some(generation), _main_thread: PhantomData }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let Some(generation) = self.generation else { return; };
        let layer = STATE.with(|slot| {
            let mut state = slot.borrow_mut();
            if state.scopes.leave(generation) { state.layer.clone() } else { None }
        });
        if let Some(layer) = layer { unsafe {
            transaction(|| {
                let _: () = msg_send![&*layer, removeAllAnimations];
                let _: () = msg_send![&*layer, setOpacity: 0f32];
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

unsafe fn arm(layer: &AnyObject, root: &NSView) {
    let class = AnyClass::get(c"CABasicAnimation").expect("QuartzCore is linked");
    let time: f64 = msg_send![layer, convertTime: CACurrentMediaTime(), fromLayer: std::ptr::null::<AnyObject>()];
    transaction(|| {
        root.effectiveAppearance().performAsCurrentDrawingAppearance(&RcBlock::new(|| {
            let color: *const CGColor = msg_send![&*NSColor::labelColor(), CGColor];
            let _: () = msg_send![layer, setStrokeColor: color];
        }));
        let _: () = msg_send![layer, setOpacity: 0f32];
        let _: () = msg_send![layer, removeAllAnimations];
        let opacity: Retained<AnyObject> = msg_send![class, animationWithKeyPath: &*NSString::from_str("opacity")];
        let _: () = msg_send![&*opacity, setFromValue: &*NSNumber::new_f64(0.)];
        let _: () = msg_send![&*opacity, setToValue: &*NSNumber::new_f64(0.85)];
        let _: () = msg_send![&*opacity, setBeginTime: time + 0.4];
        let _: () = msg_send![&*opacity, setDuration: 0.08f64];
        let _: () = msg_send![&*opacity, setFillMode: &*NSString::from_str("forwards")];
        let _: () = msg_send![&*opacity, setRemovedOnCompletion: false];
        let _: () = msg_send![layer, addAnimation: &*opacity, forKey: &*NSString::from_str("sumi-busy-visible")];
        let spin: Retained<AnyObject> = msg_send![class, animationWithKeyPath: &*NSString::from_str("transform.rotation.z")];
        let _: () = msg_send![&*spin, setFromValue: &*NSNumber::new_f64(0.)];
        let _: () = msg_send![&*spin, setToValue: &*NSNumber::new_f64(-std::f64::consts::TAU)];
        let _: () = msg_send![&*spin, setBeginTime: time + 0.4];
        let _: () = msg_send![&*spin, setDuration: 0.8f64];
        let _: () = msg_send![&*spin, setRepeatCount: f32::INFINITY];
        let _: () = msg_send![layer, addAnimation: &*spin, forKey: &*NSString::from_str("sumi-busy-spin")];
    });
    // Only a real blocking-operation boundary may flush. Once submitted, these
    // timed animations do not need the main run loop to reach the 400 ms mark.
    // https://developer.apple.com/documentation/quartzcore/catransaction/flush()
    let _: () = msg_send![AnyClass::get(c"CATransaction").unwrap(), flush];
}

#[cfg(debug_assertions)]
pub fn debug_block_once() {
    thread_local! { static USED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    if MainThreadMarker::new().is_none() { return; }
    let isolated = std::env::var_os("SUMI_LIBRARY_PATH").is_some_and(|path| std::path::Path::new(&path).components().any(|part| part.as_os_str() == "test-artifacts"));
    let delay = std::env::var("SUMI_NATIVE_BLOCK_DELAY_MS").ok().and_then(|value| value.parse::<u64>().ok()).unwrap_or(0).min(10_000);
    if !isolated || delay == 0 || USED.with(|used| used.replace(true)) { return; }
    let _busy = begin();
    std::thread::sleep(std::time::Duration::from_millis(delay));
}

#[cfg(test)]
mod tests {
    use super::Scopes;
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
