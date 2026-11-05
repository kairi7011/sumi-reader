use std::{cell::RefCell, rc::Rc, f64::consts::PI};
use objc2::{define_class, msg_send, DefinedClass, MainThreadMarker, MainThreadOnly};
use objc2::rc::Retained;
use objc2::runtime::NSObjectProtocol;
use objc2_app_kit::{NSAutoresizingMaskOptions, NSGraphicsContext, NSImage, NSView};
use objc2_core_graphics::{CGAffineTransformMake, CGContext, CGImage};
use objc2_foundation::{NSPoint, NSRect};

pub struct PageImage { pub image: Retained<NSImage>, pub rect: NSRect }
pub struct Curl {
    pub front: PageImage,
    pub companion: Option<PageImage>,
    pub back: Option<Retained<NSImage>>,
    pub direction: f64,
    pub corner: f64,
    pub progress: f64,
    pub vertical: f64,
    pub spread: bool,
    pub gutter: f64,
}
#[derive(Default)]
pub struct Visual {
    pub curl: Option<Curl>, pub capture: Option<NSRect>, pub capture_mode: bool,
    #[cfg(debug_assertions)]
    debug: Option<DebugMetrics>,
}
pub struct OverlayIvars { visual: Rc<RefCell<Visual>> }
define_class!(
    #[unsafe(super = NSView)]
    #[thread_kind = MainThreadOnly]
    #[ivars = OverlayIvars]
    pub struct Overlay;
    unsafe impl NSObjectProtocol for Overlay {}
    impl Overlay {
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool { true }
        #[unsafe(method(hitTest:))]
        fn hit_test(&self, _point: NSPoint) -> *mut NSView { std::ptr::null_mut() }
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _rect: NSRect) {
            let Some(context) = NSGraphicsContext::currentContext() else { return; };
            let cg = context.CGContext();
            #[cfg(debug_assertions)]
            let started = std::time::Instant::now();
            let visual = self.ivars().visual.borrow();
            #[cfg(debug_assertions)]
            let progress = visual.curl.as_ref().map(|curl|curl.progress);
            if let Some(curl) = &visual.curl { draw_curl(&cg, curl); }
            if visual.capture_mode {
                if let Some(rect) = visual.capture {
                    CGContext::set_rgb_fill_color(Some(&cg), 0.25, 0.43, 0.36, 0.14);
                    CGContext::fill_rect(Some(&cg), rect);
                    CGContext::set_rgb_stroke_color(Some(&cg), 0.21, 0.39, 0.32, 0.95);
                    CGContext::set_line_width(Some(&cg), 1.5);
                    CGContext::stroke_rect(Some(&cg), rect);
                }
            }
            #[cfg(debug_assertions)]
            {
                let elapsed = started.elapsed().as_secs_f64() * 1000.;
                drop(visual);
                if let Some(progress) = progress {
                    if let Some(debug) = self.ivars().visual.borrow_mut().debug.as_mut() { debug.record(started, elapsed, progress); }
                }
            }
        }
    }
);
impl Overlay {
    pub fn new(mtm: MainThreadMarker, rect: NSRect, visual: Rc<RefCell<Visual>>) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(OverlayIvars { visual });
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: rect] };
        this.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable);
        this
    }
    pub fn repaint(&self) { self.setNeedsDisplay(true); }
}

#[derive(Clone, Copy)]
struct Point { x: f64, y: f64 }
#[derive(Clone, Copy)]
struct Vertex { x: f64, y: f64, u: f64, v: f64, z: f64, theta: f64 }
struct Triangle { points: [Vertex; 3], back: bool, z: f64 }
fn path(ctx: &CGContext, points: impl IntoIterator<Item = Point>) {
    CGContext::begin_path(Some(ctx));
    for (index, point) in points.into_iter().enumerate() {
        if index == 0 { CGContext::move_to_point(Some(ctx), point.x, point.y); }
        else { CGContext::add_line_to_point(Some(ctx), point.x, point.y); }
    }
    CGContext::close_path(Some(ctx));
}
fn clip(points: &[Point], distance: impl Fn(f64, f64) -> f64) -> Vec<Point> {
    let mut result = Vec::new();
    for (index, a) in points.iter().enumerate() {
        let b = points[(index + 1) % points.len()];
        let da = distance(a.x, a.y); let db = distance(b.x, b.y);
        if da <= 0. { result.push(*a); }
        if (da <= 0.) != (db <= 0.) { let t = da / (da - db); result.push(Point { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t }); }
    }
    result
}
fn rectangle(width: f64, top: f64, bottom: f64) -> [Point; 4] {
    [Point { x: 0., y: top }, Point { x: width, y: top }, Point { x: width, y: bottom }, Point { x: 0., y: bottom }]
}
fn image_ref(image: &NSImage) -> Option<Retained<CGImage>> {
    unsafe { msg_send![image, CGImageForProposedRect: std::ptr::null_mut::<NSRect>(), context: std::ptr::null::<NSGraphicsContext>(), hints: std::ptr::null::<objc2::runtime::AnyObject>()] }
}
fn image(ctx: &CGContext, image: &CGImage, rect: NSRect, mirror: bool, alpha: f64) {
    CGContext::save_g_state(Some(ctx));
    CGContext::translate_ctm(Some(ctx), rect.origin.x + if mirror { rect.size.width } else { 0. }, rect.origin.y + rect.size.height);
    CGContext::scale_ctm(Some(ctx), if mirror { -1. } else { 1. }, -1.);
    CGContext::set_alpha(Some(ctx), alpha);
    CGContext::draw_image(Some(ctx), NSRect::new(NSPoint::new(0., 0.), rect.size), Some(image));
    CGContext::restore_g_state(Some(ctx));
}
fn draw_curl(ctx: &CGContext, curl: &Curl) {
    let Some(front) = image_ref(&curl.front.image) else { return; };
    let back = curl.back.as_ref().and_then(|value| image_ref(value));
    if let Some(companion) = &curl.companion { if let Some(texture) = image_ref(&companion.image) { image(ctx, &texture, companion.rect, false, 1.); } }
    let width = curl.front.rect.size.width; let height = curl.front.rect.size.height;
    let p = curl.progress.clamp(0., 1.); let lift = (PI * p).sin();
    let rect = curl.front.rect;
    CGContext::save_g_state(Some(ctx));
    if !curl.spread { CGContext::clip_to_rect(Some(ctx), NSRect::new(NSPoint::new(rect.origin.x - 1., rect.origin.y - 18.), objc2_foundation::NSSize::new(width + 2., height + 36.))); }
    CGContext::translate_ctm(Some(ctx), rect.origin.x + if curl.direction < 0. { width } else { 0. } - curl.direction * curl.gutter * p, rect.origin.y);
    CGContext::scale_ctm(Some(ctx), curl.direction, 1.);
    if p < 0.0001 { image(ctx, &front, NSRect::new(NSPoint::new(0.,0.), rect.size), curl.direction < 0., 1.); CGContext::restore_g_state(Some(ctx)); return; }
    let sign = if curl.corner < 0.5 { -1. } else { 1. };
    let anchor = if sign > 0. { height } else { 0. };
    let tilt = (sign * 0.43 - curl.vertical / height * 0.8).clamp(-0.8,0.8) * (1. - p).powf(0.72);
    let nx = tilt.cos(); let ny = tilt.sin(); let fold = nx * width * (1. - p).powf(1.12);
    let radius = width * (0.045 + 0.065 * lift) * lift;
    let radius_at = |v:f64| (radius * (0.72 + 0.43 * (v-anchor).abs()/height)).max(0.001);
    let distance = |u:f64,v:f64| nx*u + ny*(v-anchor) - fold;
    let project = |point:Point| {
        let u=point.x; let v=point.y; let s=distance(u,v); let r=radius_at(v);
        let theta = if s <= 0. { 0. } else { (s/r).min(PI) };
        let along = if s <= 0. { s } else { r*theta.sin() - (s-PI*r).max(0.) };
        let z = if s <= 0. { 0. } else { r*(1.-theta.cos()) };
        let perspective = 1./(1.-z/(width*5.));
        Vertex {u,v,x:(u+nx*(along-s))*perspective,y:height/2.+(v+ny*(along-s)-height/2.)*perspective,z,theta}
    };
    let mut outline=Vec::with_capacity(80);
    for edge in 0..4 { for index in 0..20 { let t=index as f64/20.; outline.push(project(Point{x:match edge{0=>t*width,1=>width,2=>(1.-t)*width,_=>0.},y:match edge{0=>0.,1=>t*height,2=>height,_=>(1.-t)*height}})); } }
    CGContext::save_g_state(Some(ctx));
    CGContext::set_shadow(Some(ctx), objc2_foundation::NSSize::new(-8.*lift,10.*lift),14.*lift);
    path(ctx,outline.iter().map(|p|Point{x:p.x,y:p.y})); CGContext::set_rgb_fill_color(Some(ctx),1.,0.998,0.98,1.); CGContext::fill_path(Some(ctx)); CGContext::restore_g_state(Some(ctx));
    let flat=clip(&rectangle(width,0.,height),distance);
    if flat.len()>2 { CGContext::save_g_state(Some(ctx)); path(ctx,flat); CGContext::clip(Some(ctx)); image(ctx,&front,NSRect::new(NSPoint::new(0.,0.),rect.size),curl.direction<0.,1.); CGContext::restore_g_state(Some(ctx)); }
    let mut triangles=Vec::with_capacity(240);
    let mut add=|points:Vec<Point>,back:bool| { let vertices:Vec<_>=points.into_iter().map(project).collect(); for i in 1..vertices.len().saturating_sub(1){ let points=[vertices[0],vertices[i],vertices[i+1]]; triangles.push(Triangle{points,back,z:points.iter().map(|p|p.z).sum::<f64>()/3.}); } };
    let bands=16; let rows=6;
    for row in 0..rows {
        let top=height*row as f64/rows as f64; let bottom=height*(row+1) as f64/rows as f64;
        add(clip(&rectangle(width,top,bottom),|u,v|PI*radius_at(v)-distance(u,v)),true);
        for band in 0..bands {
            let from=PI*band as f64/bands as f64; let to=PI*(band+1) as f64/bands as f64;
            let source=|theta:f64,v:f64|Point{x:(fold-ny*(v-anchor)+theta*radius_at(v))/nx,y:v};
            let surface=[source(from,top),source(to,top),source(to,bottom),source(from,bottom)];
            add(clip(&clip(&surface,|u,_|-u),|u,_|u-width),band>=bands/2);
        }
    }
    triangles.sort_by(|a,b|a.z.total_cmp(&b.z));
    for triangle in triangles { let has_back=triangle.back && back.is_some(); texture_triangle(ctx,if has_back {back.as_deref().unwrap()}else{&front},triangle.points,width,height,if has_back {curl.direction>0.} else {curl.direction<0.},if triangle.back && !has_back {0.075}else{1.}); }
    path(ctx,outline.iter().map(|p|Point{x:p.x,y:p.y})); CGContext::set_rgb_stroke_color(Some(ctx),0.25,0.22,0.15,0.12+lift*0.15); CGContext::set_line_width(Some(ctx),0.7); CGContext::stroke_path(Some(ctx));
    CGContext::begin_path(Some(ctx)); for index in 0..32 {let v=project(Point{x:width,y:height*index as f64/31.});if index==0{CGContext::move_to_point(Some(ctx),v.x,v.y)}else{CGContext::add_line_to_point(Some(ctx),v.x,v.y)}}
    CGContext::set_line_width(Some(ctx),1.2); CGContext::set_rgb_stroke_color(Some(ctx),1.,1.,0.98,0.8); CGContext::stroke_path(Some(ctx)); CGContext::restore_g_state(Some(ctx));
}
fn texture_triangle(ctx:&CGContext,texture:&CGImage,points:[Vertex;3],width:f64,height:f64,mirror:bool,alpha:f64) {
    let uv=points.map(|p|Point{x:if mirror{width-p.u}else{p.u},y:p.v}); let [a,b,c]=uv; let [p,q,r]=points;
    let det=a.x*(b.y-c.y)+b.x*(c.y-a.y)+c.x*(a.y-b.y); if det.abs()<0.0001{return;}
    let coefficients=|p:f64,q:f64,r:f64|[(p*(b.y-c.y)+q*(c.y-a.y)+r*(a.y-b.y))/det,(p*(c.x-b.x)+q*(a.x-c.x)+r*(b.x-a.x))/det,(p*(b.x*c.y-c.x*b.y)+q*(c.x*a.y-a.x*c.y)+r*(a.x*b.y-b.x*a.y))/det];
    let x=coefficients(p.x,q.x,r.x);let y=coefficients(p.y,q.y,r.y);
    let orientation=((q.x-p.x)*(r.y-p.y)-(r.x-p.x)*(q.y-p.y)).signum();
    let normals:[Point;3]=std::array::from_fn(|i|{let next=points[(i+1)%3];let dx=next.x-points[i].x;let dy=next.y-points[i].y;let len=dx.hypot(dy).max(0.0001);Point{x:orientation*dy/len,y:-orientation*dx/len}});
    let expanded:[Point;3]=std::array::from_fn(|i|{let before=normals[(i+2)%3];let after=normals[i];let x=before.x+after.x;let y=before.y+after.y;let amount=(0.55/(1.+before.x*after.x+before.y*after.y).max(0.01)).min(2.5/x.hypot(y).max(0.001));Point{x:points[i].x+x*amount,y:points[i].y+y*amount}});
    CGContext::save_g_state(Some(ctx));path(ctx,expanded);CGContext::clip(Some(ctx));
    if alpha<1. {CGContext::set_rgb_fill_color(Some(ctx),1.,0.998,0.98,1.);CGContext::fill_rect(Some(ctx),CGContext::clip_bounding_box(Some(ctx)));}
    CGContext::save_g_state(Some(ctx));CGContext::concat_ctm(Some(ctx),CGAffineTransformMake(x[0],y[0],x[1],y[1],x[2],y[2]));
    image(ctx,texture,NSRect::new(NSPoint::new(0.,0.),objc2_foundation::NSSize::new(width,height)),false,alpha);CGContext::restore_g_state(Some(ctx));
    let shade=points.iter().map(|p|0.018*(1.-p.theta.cos())+0.15*p.theta.sin()).sum::<f64>()/3.;
    CGContext::set_rgb_fill_color(Some(ctx),0.16,0.13,0.08,shade);CGContext::fill_rect(Some(ctx),CGContext::clip_bounding_box(Some(ctx)));CGContext::restore_g_state(Some(ctx));
}

#[cfg(debug_assertions)]
pub fn debug_frame(view: &NSView, visual: &Visual, path: &std::path::Path) {
    use objc2::ClassType;
    use objc2::rc::Allocated;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::{NSData, NSDictionary, NSString};
    use objc2_app_kit::NSDeviceRGBColorSpace;
    let bounds=view.bounds();let scale=(3_000_000./(bounds.size.width*bounds.size.height).max(1.)).sqrt().min(1.5);
    let width=(bounds.size.width*scale).floor()as isize;let height=(bounds.size.height*scale).floor()as isize;
    let bitmap:Option<Retained<AnyObject>>=unsafe {let Some(class)=AnyClass::get(c"NSBitmapImageRep")else{return;};let allocated:Allocated<AnyObject>=msg_send![class,alloc];msg_send![allocated,initWithBitmapDataPlanes:std::ptr::null_mut::<*mut u8>(),pixelsWide:width,pixelsHigh:height,bitsPerSample:8isize,samplesPerPixel:4isize,hasAlpha:true,isPlanar:false,colorSpaceName:NSDeviceRGBColorSpace,bytesPerRow:0isize,bitsPerPixel:32isize]};
    let Some(bitmap)=bitmap else{return;};let context:Option<Retained<NSGraphicsContext>>=unsafe{msg_send![NSGraphicsContext::class(),graphicsContextWithBitmapImageRep:&*bitmap]};let Some(context)=context else{return;};
    NSGraphicsContext::saveGraphicsState_class();NSGraphicsContext::setCurrentContext(Some(&context));let cg=context.CGContext();CGContext::scale_ctm(Some(&cg),scale,scale);
    view.displayRectIgnoringOpacity_inContext(bounds,&context);
    CGContext::translate_ctm(Some(&cg),0.,bounds.size.height);CGContext::scale_ctm(Some(&cg),1.,-1.);
    if let Some(curl)=&visual.curl{draw_curl(&cg,curl);}
    NSGraphicsContext::restoreGraphicsState_class();
    let properties:Retained<NSDictionary<NSString,AnyObject>>=NSDictionary::new();
    let data:Option<Retained<NSData>>=unsafe{msg_send![&*bitmap,representationUsingType:4usize,properties:&*properties]};
    if let Some(data)=data { if let Some(parent)=path.parent(){let _=std::fs::create_dir_all(parent);}let _=std::fs::write(path,data.to_vec()); }
}

#[cfg(debug_assertions)]
struct DebugMetrics {
    start: std::time::Instant,
    preparation_ms: f64,
    first_draw_ms: Option<f64>,
    last_draw: Option<std::time::Instant>,
    draws: Vec<(f64, f64)>,
    intervals: Vec<f64>,
}
#[cfg(debug_assertions)]
impl DebugMetrics {
    fn record(&mut self, start: std::time::Instant, elapsed: f64, progress: f64) {
        if self.draws.len() >= 600 { return; }
        self.first_draw_ms.get_or_insert(start.duration_since(self.start).as_secs_f64() * 1000.);
        if let Some(last) = self.last_draw { self.intervals.push(start.duration_since(last).as_secs_f64() * 1000.); }
        self.last_draw = Some(start);
        self.draws.push((progress, elapsed));
    }
}
#[cfg(debug_assertions)]
pub fn debug_pngs_enabled() -> bool { std::env::var("SUMI_CAPTURE_TURN_PNGS").as_deref() != Ok("0") }
#[cfg(debug_assertions)]
pub fn debug_begin(visual: &mut Visual) {
    visual.debug = std::env::var_os("SUMI_CAPTURE_TURNS").map(|_| DebugMetrics { start: std::time::Instant::now(), preparation_ms: 0., first_draw_ms: None, last_draw: None, draws: Vec::with_capacity(40), intervals: Vec::with_capacity(40) });
}
#[cfg(debug_assertions)]
pub fn debug_prepared(visual: &mut Visual) {
    if let Some(debug) = visual.debug.as_mut() { debug.preparation_ms = debug.start.elapsed().as_secs_f64() * 1000.; }
}
#[cfg(debug_assertions)]
pub fn debug_finish(visual: &mut Visual, from: usize, target: usize, committed: bool) {
    let Some(debug) = visual.debug.take() else { return; };
    let Some(directory) = std::env::var_os("SUMI_CAPTURE_TURNS") else { return; };
    fn summary(values: impl Iterator<Item=f64>) -> serde_json::Value {
        let mut values: Vec<f64> = values.collect(); values.sort_by(f64::total_cmp);
        let quantile = |p:f64| if values.is_empty() { 0. } else { values[((values.len() as f64 * p).ceil() as usize).saturating_sub(1).min(values.len()-1)] };
        serde_json::json!({"p50":quantile(0.5),"p95":quantile(0.95),"max":values.last().copied().unwrap_or(0.)})
    }
    let mut pixels=Vec::new();
    if let Some(curl)=&visual.curl {
        for texture in [Some(&curl.front.image),curl.companion.as_ref().map(|page|&page.image),curl.back.as_ref()].into_iter().flatten() {
            if let Some(image)=image_ref(texture) {pixels.push((CGImage::width(Some(&image)),CGImage::height(Some(&image))));}
        }
    }
    let result=serde_json::json!({
        "from_page":from,"target_page":target,"committed":committed,"png_capture_enabled":debug_pngs_enabled(),
        "preparation_ms":debug.preparation_ms,"first_draw_ms":debug.first_draw_ms,"elapsed_ms":debug.start.elapsed().as_secs_f64()*1000.,
        "frame_count":debug.draws.len(),"draw_ms":summary(debug.draws.iter().map(|(_,time)|*time)),"frame_interval_ms":summary(debug.intervals.iter().copied()),
        "snapshot_dimensions":pixels,"snapshot_total_pixels":pixels.iter().map(|(w,h)|w*h).sum::<usize>(),
        "draw_samples":debug.draws.iter().map(|(progress,ms)|serde_json::json!({"progress":progress,"ms":ms})).collect::<Vec<_>>()
    });
    let directory=std::path::PathBuf::from(directory);let _=std::fs::create_dir_all(&directory);
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_micros();
    if let Ok(bytes)=serde_json::to_vec_pretty(&result) {let _=std::fs::write(directory.join(format!("turn-{stamp}-{from}-{target}.json")),bytes);}
}
