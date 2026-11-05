use block2::RcBlock;
use objc2::{rc::Retained, runtime::Bool};
use objc2_app_kit::{NSBezierPath, NSColor, NSImage, NSLineCapStyle, NSLineJoinStyle};
use objc2_foundation::{NSPoint, NSRect, NSSize};

#[derive(Clone, Copy)]
enum Icon { Library, FolderInput, Search, Close }

pub fn icon(name: &str) -> Option<Retained<NSImage>> {
    let kind = match name {
        "library" => Icon::Library,
        "folder-input" => Icon::FolderInput,
        "search" => Icon::Search,
        "x" => Icon::Close,
        _ => return None,
    };
    // The drawing handler keeps the reviewed SVG geometry sharp at each display's backing scale.
    let draw = RcBlock::new(move |bounds: NSRect| -> Bool {
        draw_icon(kind, bounds);
        Bool::YES
    });
    let image = NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(18., 18.), false, &draw);
    image.setTemplate(true);
    Some(image)
}

// Geometry from assets/icons/lucide; the original SVGs and Lucide's ISC license are retained there.
fn draw_icon(kind: Icon, bounds: NSRect) {
    let path = NSBezierPath::bezierPath();
    let point = |x: f64, y: f64| NSPoint::new(
        bounds.origin.x + x * bounds.size.width / 24.,
        bounds.origin.y + (24. - y) * bounds.size.height / 24.,
    );
    let segment = |from: (f64, f64), to: (f64, f64)| {
        path.moveToPoint(point(from.0, from.1));
        path.lineToPoint(point(to.0, to.1));
    };
    match kind {
        Icon::Library => {
            segment((16., 6.), (20., 20.));
            segment((12., 6.), (12., 20.));
            segment((8., 8.), (8., 20.));
            segment((4., 4.), (4., 20.));
        }
        Icon::FolderInput => {
            path.moveToPoint(point(2., 9.));
            path.lineToPoint(point(2., 5.));
            arc(&path, &point, (2., 5.), (4., 3.), true);
            path.lineToPoint(point(7.9, 3.));
            arc(&path, &point, (7.9, 3.), (9.59, 3.9), true);
            path.lineToPoint(point(10.4, 5.1));
            arc(&path, &point, (10.4, 5.1), (12.07, 6.), false);
            path.lineToPoint(point(20., 6.));
            arc(&path, &point, (20., 6.), (22., 8.), true);
            path.lineToPoint(point(22., 18.));
            arc(&path, &point, (22., 18.), (20., 20.), true);
            path.lineToPoint(point(4., 20.));
            arc(&path, &point, (4., 20.), (2., 18.), true);
            path.lineToPoint(point(2., 17.));
            segment((2., 13.), (12., 13.));
            path.moveToPoint(point(9., 16.));
            path.lineToPoint(point(12., 13.));
            path.lineToPoint(point(9., 10.));
        }
        Icon::Search => {
            path.appendBezierPathWithOvalInRect(NSRect::new(point(3., 19.), NSSize::new(bounds.size.width * 16. / 24., bounds.size.height * 16. / 24.)));
            segment((21., 21.), (16.7, 16.7));
        }
        Icon::Close => {
            segment((18., 6.), (6., 18.));
            segment((6., 6.), (18., 18.));
        }
    }
    path.setLineWidth(2. * bounds.size.width / 24.);
    path.setLineCapStyle(NSLineCapStyle::Round);
    path.setLineJoinStyle(NSLineJoinStyle::Round);
    NSColor::blackColor().setStroke();
    path.stroke();
}

fn arc(path: &NSBezierPath, point: &impl Fn(f64, f64) -> NSPoint, from: (f64, f64), to: (f64, f64), sweep: bool) {
    // Lucide's folder uses only radius-2, non-large circular arcs. Cubics preserve their rounded joins.
    let (dx, dy) = ((from.0 - to.0) / 2., (from.1 - to.1) / 2.);
    let distance = dx * dx + dy * dy;
    let radius = 2f64.max(distance.sqrt());
    let factor = ((radius * radius - distance).max(0.) / distance).sqrt() * if sweep { 1. } else { -1. };
    let center = ((from.0 + to.0) / 2. + factor * dy, (from.1 + to.1) / 2. - factor * dx);
    let start = (from.1 - center.1).atan2(from.0 - center.0);
    let mut delta = (to.1 - center.1).atan2(to.0 - center.0) - start;
    if sweep && delta < 0. { delta += std::f64::consts::TAU; }
    if !sweep && delta > 0. { delta -= std::f64::consts::TAU; }
    let count = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize;
    let step = delta / count as f64;
    for index in 0..count {
        let a = start + step * index as f64;
        let b = a + step;
        let k = 4. / 3. * (step / 4.).tan();
        let c1 = (center.0 + radius * (a.cos() - k * a.sin()), center.1 + radius * (a.sin() + k * a.cos()));
        let c2 = (center.0 + radius * (b.cos() + k * b.sin()), center.1 + radius * (b.sin() - k * b.cos()));
        let end = if index + 1 == count { to } else { (center.0 + radius * b.cos(), center.1 + radius * b.sin()) };
        path.curveToPoint_controlPoint1_controlPoint2(point(end.0, end.1), point(c1.0, c1.1), point(c2.0, c2.1));
    }
}
