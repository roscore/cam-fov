//! Browser UI: reads the form, calls `cf_core`, writes text outputs and draws two canvases.
mod gl;
mod math;
mod view2d;
mod view3d;

use cf_core::{depth_resolution_mm, floor_view, min_depth_m, Camera};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d as Ctx, Document, Event, HtmlCanvasElement, HtmlElement, HtmlInputElement, HtmlSelectElement, PointerEvent};

struct Ui {
    ko: bool,
    yaw: f64,
    pitch: f64,
    drag: Option<(f64, f64)>,
}

thread_local! {
    static UI: RefCell<Ui> = const { RefCell::new(Ui { ko: false, yaw: -0.7, pitch: 0.45, drag: None }) };
}

/// Current inputs for the 3D view (validated by `update`).
#[derive(Clone, Copy)]
pub struct Scene {
    pub cam: Camera,
    pub h: f64,
    pub tilt: f64,
    pub z: f64,
    pub obj_mm: f64,
    pub minpx: f64,
    /// Stereo (baseline mm, Δd px, d_max px) when enabled and valid.
    pub stereo: Option<(f64, f64, f64)>,
}

impl Scene {
    pub fn baseline_mm(&self) -> Option<f64> {
        self.stereo.map(|s| s.0)
    }
}

const INPUTS: &[&str] = &["mode", "pitch", "sw", "sh", "rw", "rh", "f", "h", "tilt", "z", "obj", "minpx", "stereo", "base", "dd", "dmax"];

fn doc() -> Document {
    web_sys::window().unwrap().document().unwrap()
}
fn by_id<T: JsCast>(id: &str) -> T {
    doc().get_element_by_id(id).unwrap_or_else(|| panic!("missing #{id}")).dyn_into::<T>().unwrap()
}
fn num(id: &str) -> f64 {
    by_id::<HtmlInputElement>(id).value_as_number()
}
fn set_num(id: &str, v: f64) {
    by_id::<HtmlInputElement>(id).set_value(&trim(v));
}
fn set_text(id: &str, s: &str) {
    by_id::<HtmlElement>(id).set_text_content(Some(s));
}
fn show(id: &str, on: bool) {
    by_id::<HtmlElement>(id).set_hidden(!on);
}
fn ko() -> bool {
    UI.with(|u| u.borrow().ko)
}
fn tr<'a>(en: &'a str, k: &'a str) -> &'a str {
    if ko() { k } else { en }
}
/// Number for an input field: up to 4 decimals, trailing zeros removed.
fn trim(v: f64) -> String {
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
fn m(v: f64) -> String {
    if v.is_infinite() { "∞".into() } else { format!("{v:.3} m") }
}

fn on<F: FnMut(Event) + 'static>(id: &str, ev: &str, f: F) {
    let c = Closure::<dyn FnMut(Event)>::new(f);
    by_id::<web_sys::EventTarget>(id).add_event_listener_with_callback(ev, c.as_ref().unchecked_ref()).unwrap();
    c.forget();
}

pub fn css_var(name: &str) -> String {
    let w = web_sys::window().unwrap();
    w.get_computed_style(&doc().document_element().unwrap())
        .ok()
        .flatten()
        .and_then(|s| s.get_property_value(name).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "#888".into())
}

/// Resize the canvas backing store to its CSS size × devicePixelRatio; returns CSS width/height.
pub fn prepare(id: &str) -> (Ctx, f64, f64) {
    let c: HtmlCanvasElement = by_id(id);
    let dpr = web_sys::window().unwrap().device_pixel_ratio().max(1.0);
    let (w, h) = (c.client_width().max(1) as f64, c.client_height().max(1) as f64);
    c.set_width((w * dpr) as u32);
    c.set_height((h * dpr) as u32);
    let ctx: Ctx = c.get_context("2d").unwrap().unwrap().dyn_into().unwrap();
    ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0).unwrap();
    ctx.clear_rect(0.0, 0.0, w, h);
    (ctx, w, h)
}

fn apply_preset(name: &str) {
    // Generic, editable example values; not tied to any product.
    let (pitch, rw, rh, f) = match name {
        "wide" => (3.0, 1280.0, 800.0, 2.1),
        "narrow" => (1.4, 1920.0, 1080.0, 6.0),
        _ => return,
    };
    by_id::<HtmlSelectElement>("mode").set_value("pitch");
    set_num("pitch", pitch);
    set_num("rw", rw);
    set_num("rh", rh);
    set_num("f", f);
}

fn apply_lang() {
    let k = ko();
    let list = doc().query_selector_all("[data-en]").unwrap();
    for i in 0..list.length() {
        let e: web_sys::Element = list.item(i).unwrap().dyn_into().unwrap();
        if let Some(t) = e.get_attribute(if k { "data-ko" } else { "data-en" }) {
            e.set_text_content(Some(&t));
        }
    }
    doc().document_element().unwrap().set_attribute("lang", if k { "ko" } else { "en" }).unwrap();
    set_text("lang", if k { "English" } else { "한국어" });
    update();
}

fn read_camera() -> Camera {
    let (rw, rh, f) = (num("rw"), num("rh"), num("f"));
    let pitch_mode = by_id::<HtmlSelectElement>("mode").value() == "pitch";
    show("g_pitch", pitch_mode);
    show("g_size", !pitch_mode);
    if pitch_mode {
        let cam = Camera::from_pitch(num("pitch"), rw, rh, f);
        if cam.valid() {
            set_num("sw", cam.sensor_w_mm);
            set_num("sh", cam.sensor_h_mm);
        }
        cam
    } else {
        let cam = Camera { sensor_w_mm: num("sw"), sensor_h_mm: num("sh"), res_w: rw, res_h: rh, focal_mm: f };
        if cam.valid() {
            set_num("pitch", cam.pitch_x_mm() * 1000.0);
        }
        cam
    }
}

fn update() {
    let cam = read_camera();
    let (h, tilt, z, obj, minpx) = (num("h"), num("tilt"), num("z"), num("obj"), num("minpx"));
    let stereo = by_id::<HtmlInputElement>("stereo").checked();
    show("g_stereo", stereo);
    show("s_plot", stereo);
    let bad = tr("⚠️ check inputs (all values must be positive numbers)", "⚠️ 입력값을 확인하세요 (모든 값은 양수여야 합니다)");
    if !cam.valid() {
        set_text("o_cam", bad);
        return;
    }
    let k = cam.intrinsics();
    set_text(
        "o_cam",
        &format!(
            "HFOV {:.2}°  VFOV {:.2}°  DFOV {:.2}°\n{} {:.3} µm × {:.3} µm\nf_px = {:.1} px\nK = [{:9.2} {:9.2} {:9.2}]\n    [{:9.2} {:9.2} {:9.2}]\n    [{:9.2} {:9.2} {:9.2}]",
            cam.hfov_deg(), cam.vfov_deg(), cam.dfov_deg(), tr("pixel pitch", "픽셀 피치"),
            cam.pitch_x_mm() * 1000.0, cam.pitch_y_mm() * 1000.0, k.fx,
            k.fx, 0.0, k.cx, 0.0, k.fy, k.cy, 0.0, 0.0, 1.0
        ),
    );
    if ![h, tilt, z, obj, minpx].iter().all(|v| v.is_finite()) || z <= 0.0 || h < 0.0 || obj <= 0.0 {
        set_text("o_mount", bad);
        return;
    }
    let zs: HtmlInputElement = by_id("zs");
    zs.set_max(&trim(10f64.max(z.ceil())));
    zs.set_value(&trim(z));
    set_text("zs_val", &format!("{z:.2} m"));
    let (fw, fh) = cam.footprint_m(z);
    let ppm = cam.px_per_mm(z);
    let opx = obj * ppm;
    let ok = if opx >= minpx { "✅" } else { "⚠️" };
    let floor = match floor_view(h, tilt, cam.vfov_deg()) {
        Some(v) => format!(
            "{} {} … {}\n{} 0 … {}",
            tr("floor visible:", "보이는 바닥:"), m(v.near), m(v.far), tr("blind zone in front:", "전방 사각지대:"), m(v.near)
        ),
        None => tr("floor not visible (camera looks above the horizon)", "바닥이 보이지 않음 (카메라가 수평선 위를 향함)").into(),
    };
    set_text(
        "o_mount",
        &format!(
            "{} {:.3} m × {:.3} m\n{} {:.3} px/mm\n{} {:.1} px {} ({} {} px)\n{}",
            tr("footprint at target:", "대상 거리 촬영 범위:"), fw, fh, tr("pixel density (horizontal):", "픽셀 밀도 (가로):"), ppm,
            tr("object size:", "물체 크기:"), opx, ok, tr("min", "최소"), minpx, floor
        ),
    );
    let st = stereo.then(|| (num("base"), num("dd"), num("dmax"))).filter(|(b, d, x)| *b > 0.0 && *d > 0.0 && *x > 0.0);
    if stereo {
        match st {
            Some((b, dd, dmax)) => set_text(
                "o_stereo",
                &format!(
                    "Δz @ {:.2} m = {:.2} mm\nz_min = f_px·B/d_max = {:.3} m",
                    z, depth_resolution_mm(k.fx, b, dd, z), min_depth_m(k.fx, b, dmax)
                ),
            ),
            None => set_text("o_stereo", bad),
        }
    }
    let scene = Scene { cam, h, tilt, z, obj_mm: obj, minpx, stereo: st };
    if !view3d::set_scene(scene) {
        let (yaw, pitch) = UI.with(|u| (u.borrow().yaw, u.borrow().pitch));
        let (ctx, w, hh) = prepare("view3d");
        view2d::draw(&ctx, w, hh, &scene, yaw, pitch);
    }
    if let Some((b, dd, dmax)) = st {
        draw_plot(k.fx, b, dd, min_depth_m(k.fx, b, dmax), z);
    }
}

/// Δz (mm) vs distance from z_min to 3× target, with the target distance as a vertical cursor.
fn draw_plot(fpx: f64, b: f64, dd: f64, zmin: f64, z: f64) {
    let (ctx, w, h) = prepare("plot");
    let (fg, acc, tgt) = (css_var("--fg"), css_var("--accent"), css_var("--target"));
    ctx.set_font("12px system-ui, sans-serif");
    ctx.set_fill_style_str(&fg);
    let z1 = 3.0 * z;
    if zmin >= z1 {
        let _ = ctx.fill_text(tr("z_min is beyond 3× target distance", "z_min이 대상 거리의 3배보다 멉니다"), 12.0, 24.0);
        return;
    }
    let ymax = depth_resolution_mm(fpx, b, dd, z1) * 1.05;
    let (l, r, t, bm) = (56.0, 12.0, 12.0, 30.0);
    let px = |x: f64| l + (x - zmin) / (z1 - zmin) * (w - l - r);
    let py = |y: f64| h - bm - y / ymax * (h - t - bm);
    ctx.set_stroke_style_str(&fg);
    ctx.set_line_width(1.0);
    ctx.stroke_rect(l, t, w - l - r, h - t - bm);
    for i in 0..=4 {
        let fx = zmin + (z1 - zmin) * i as f64 / 4.0;
        let fy = ymax * i as f64 / 4.0;
        let _ = ctx.fill_text(&format!("{fx:.2}"), px(fx) - 12.0, h - bm + 16.0);
        let _ = ctx.fill_text(&format!("{fy:.0}"), 4.0, py(fy) + 4.0);
    }
    let _ = ctx.fill_text("m", w - r - 10.0, h - 4.0);
    let _ = ctx.fill_text("Δz mm", l + 4.0, t + 14.0);
    ctx.set_stroke_style_str(&acc);
    ctx.set_line_width(2.0);
    ctx.begin_path();
    for i in 0..=120 {
        let x = zmin + (z1 - zmin) * i as f64 / 120.0;
        let (sx, sy) = (px(x), py(depth_resolution_mm(fpx, b, dd, x)));
        if i == 0 { ctx.move_to(sx, sy) } else { ctx.line_to(sx, sy) }
    }
    ctx.stroke();
    if z >= zmin {
        let dz = depth_resolution_mm(fpx, b, dd, z);
        ctx.set_stroke_style_str(&tgt);
        ctx.set_fill_style_str(&tgt);
        ctx.set_line_width(1.5);
        ctx.begin_path();
        ctx.move_to(px(z), t);
        ctx.line_to(px(z), h - bm);
        ctx.stroke();
        let _ = ctx.fill_text(&format!("{z:.2} m: {dz:.1} mm"), px(z) + 4.0, t + 30.0);
    }
}

fn setup_orbit() {
    on("view3d", "pointerdown", |e| {
        let p: PointerEvent = e.dyn_into().unwrap();
        let _ = by_id::<HtmlElement>("view3d").set_pointer_capture(p.pointer_id());
        UI.with(|u| u.borrow_mut().drag = Some((p.client_x() as f64, p.client_y() as f64)));
    });
    on("view3d", "pointermove", |e| {
        let p: PointerEvent = e.dyn_into().unwrap();
        let (x, y) = (p.client_x() as f64, p.client_y() as f64);
        let moved = UI.with(|u| {
            let mut u = u.borrow_mut();
            let Some((lx, ly)) = u.drag else { return false };
            u.yaw += (x - lx) * 0.01;
            u.pitch = (u.pitch + (y - ly) * 0.01).clamp(-0.3, 1.5);
            u.drag = Some((x, y));
            true
        });
        if moved {
            update();
        }
    });
    for ev in ["pointerup", "pointercancel"] {
        on("view3d", ev, |_| UI.with(|u| u.borrow_mut().drag = None));
    }
}

#[wasm_bindgen(start)]
pub fn start() {
    let lang = web_sys::window().unwrap().navigator().language().unwrap_or_default();
    UI.with(|u| u.borrow_mut().ko = lang.to_lowercase().starts_with("ko"));
    by_id::<HtmlSelectElement>("preset").set_value("wide");
    apply_preset("wide");
    for (id, v) in [("h", 0.8), ("tilt", 20.0), ("z", 1.5), ("obj", 50.0), ("minpx", 20.0), ("base", 60.0), ("dd", 0.25), ("dmax", 128.0)] {
        set_num(id, v);
    }
    for id in INPUTS {
        on(id, "input", |_| update());
    }
    // Editing camera fields by hand turns the preset selector back to "custom".
    for id in ["mode", "pitch", "sw", "sh", "rw", "rh", "f"] {
        on(id, "input", |_| by_id::<HtmlSelectElement>("preset").set_value(""));
    }
    on("preset", "change", |_| {
        apply_preset(&by_id::<HtmlSelectElement>("preset").value());
        update();
    });
    on("zs", "input", |_| {
        set_num("z", num("zs"));
        update();
    });
    on("lang", "click", |_| {
        UI.with(|u| {
            let mut u = u.borrow_mut();
            u.ko = !u.ko;
        });
        apply_lang();
    });
    let c = Closure::<dyn FnMut()>::new(update);
    web_sys::window().unwrap().add_event_listener_with_callback("resize", c.as_ref().unchecked_ref()).unwrap();
    c.forget();
    // ponytail: if WebGL2 exists but shaders fail, the canvas is already taken and the 2D fallback
    // cannot draw; swap in a fresh canvas element if that ever shows up in practice.
    if !view3d::init() {
        show("nogl", true);
        for id in ["bar", "hud", "tlabel"] {
            show(id, false);
        }
        setup_orbit();
    }
    apply_lang();
}

fn main() {}
