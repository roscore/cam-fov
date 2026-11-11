//! cam-fov 3D scene on the WebGL2 renderer: robot on a mast, camera module (stereo pair),
//! translucent frustum cut at the ground, floor footprint + blind zone, target "person",
//! stereo Δz rings, orbit / pan / zoom with inertia and direct manipulation of the inputs.
use crate::gl::{self, Geo, Mesh, Paint, Renderer};
use crate::math::{self, Orbit, OrbitCam, M4, V};
use crate::{by_id, m, set_num, set_text, show, tr, Scene};
use cf_core::{corner_dirs, depth_resolution_mm, floor_footprint, floor_view, min_depth_m, V3};
use std::cell::RefCell;
use std::f32::consts::{PI, TAU};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlCanvasElement, HtmlElement, PointerEvent, WheelEvent};

/// Vertical field of view of the orbit view (rad).
const FOVY: f32 = 0.8;
const TOGGLES: [&str; 4] = ["t_frustum", "t_foot", "t_rings", "t_shadow"];
const VIEWS: [&str; 4] = ["v_persp", "v_top", "v_side", "v_fpv"];

#[derive(Clone, Copy, PartialEq)]
enum Handle {
    Target,
    Height,
    Tilt,
}

#[derive(Clone, Copy)]
enum Drag {
    Orbit,
    Pan,
    /// Handle, world point where the drag started, input value at that moment.
    Handle(Handle, V, f64),
}

#[derive(Clone, Copy, PartialEq)]
enum Layer {
    Frustum,
    Foot,
    Rings,
    Lines,
}

struct Meshes {
    cube: Mesh,
    cyl: Mesh,
    person: Mesh,
    sphere: Mesh,
    ring: Mesh,
}

struct St {
    r: Renderer,
    canvas: HtmlCanvasElement,
    m: Meshes,
    /// Scene-dependent meshes in world coordinates, rebuilt on every input change.
    dynm: Vec<(Layer, Mesh, Paint)>,
    sc: Option<Scene>,
    orbit: OrbitCam,
    fpv: bool,
    show: [bool; 4],
    hover: Option<Handle>,
    drag: Option<Drag>,
    ptrs: Vec<(i32, f32, f32)>,
    last_down: f64,
    /// Scene viewport in CSS px (x, y from top, w, h) and the matrices of the last frame.
    vp_css: [f32; 4],
    vp: M4,
    inv: M4,
    pending: bool,
    last_t: f64,
}

thread_local! {
    static ST: RefCell<Option<St>> = const { RefCell::new(None) };
    static RAF: RefCell<Option<Closure<dyn FnMut(f64)>>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut St) -> R) -> Option<R> {
    ST.with(|s| s.borrow_mut().as_mut().map(f))
}

/// cf_core world (x right, y up, z forward, as seen by the robot) → right-handed render world.
fn rv(p: V3) -> V {
    [-p.x as f32, p.y as f32, p.z as f32]
}

/// Scene layout in render coordinates.
struct Lay {
    /// Optical centre of the (left) camera; the stereo partner sits at x − b.
    cam: V,
    b: f32,
    mast_x: f32,
    up: V,
    fwd: V,
    /// Ground point where the target stands (below the axis point at distance z).
    tgt: V,
    tgt_h: f32,
    body_h: f32,
}

fn lay(sc: &Scene) -> Lay {
    let t = (sc.tilt as f32).to_radians();
    let b = sc.baseline_mm().unwrap_or(0.0) as f32 / 1000.0;
    let h = sc.h as f32;
    Lay {
        cam: [0.0, h, 0.0],
        b,
        mast_x: -b / 2.0,
        up: [0.0, t.cos(), t.sin()],
        fwd: [0.0, -t.sin(), t.cos()],
        tgt: [0.0, 0.0, sc.z as f32 * t.cos().max(0.0)],
        tgt_h: sc.obj_mm as f32 / 1000.0,
        body_h: (0.3 * h).clamp(0.02, 0.25),
    }
}

/// Handle sizes scale with the view distance so they stay grabbable: (knob radius, tilt ring radius).
fn sizes(dist: f32) -> (f32, f32) {
    ((0.012 * dist).clamp(0.008, 0.08), (0.06 * dist).clamp(0.06, 0.5))
}

fn shift(mut g: Geo, o: V) -> Geo {
    g.pos.chunks_mut(3).for_each(|p| (0..3).for_each(|i| p[i] += o[i]));
    g
}

/// Green → yellow → red for t in 0…1.
fn ramp(t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        let k = t * 2.0;
        [0.2 + 0.8 * k, 0.85, 0.35 - 0.15 * k, 0.8]
    } else {
        let k = (t - 0.5) * 2.0;
        [1.0 - 0.05 * k, 0.85 - 0.6 * k, 0.2, 0.8]
    }
}

/// Default orbit framing robot and target.
fn fit(sc: &Scene) -> Orbit {
    let l = lay(sc);
    let ext = (sc.h as f32).max(l.tgt[2]).max(0.5);
    Orbit { target: [l.mast_x, sc.h as f32 * 0.4, l.tgt[2] * 0.5], yaw: 2.4, pitch: 0.45, dist: ext * 2.2 }
}

fn rebuild(st: &mut St, sc: &Scene) {
    for (_, mesh, _) in st.dynm.drain(..) {
        st.r.free(mesh);
    }
    let l = lay(sc);
    let o = V3::new(0.0, sc.h, 0.0);
    let dirs = corner_dirs(&sc.cam, sc.tilt);
    let fv = floor_view(sc.h, sc.tilt, sc.cam.vfov_deg());
    let cap = (3.0 * sc.z).max(fv.map_or(0.0, |v| 2.0 * v.near)).max(1.0);
    let (tx, ty) = ((sc.cam.hfov_deg() / 2.0).to_radians().tan() as f32, (sc.cam.vfov_deg() / 2.0).to_radians().tan() as f32);
    // Camera frame → render world: camera right is render −x.
    let at = |d: f64| math::frustum_corners(tx, ty, d as f32).map(|c| math::add(l.cam, math::add([-c[0], 0.0, 0.0], math::add(math::scale(l.up, c[1]), math::scale(l.fwd, c[2])))));
    let far = at(cap);
    let rect = at(sc.z);
    let blue = [0.35, 0.65, 1.0];
    let clip = |c: [f32; 4]| Paint { clip: 0.0, ..Paint::flat(c) };
    let mut add = |layer, g: Geo, p| {
        if let Some(mesh) = st.r.mesh(&g) {
            st.dynm.push((layer, mesh, p));
        }
    };
    add(Layer::Frustum, gl::frustum(l.cam, &far), clip([blue[0], blue[1], blue[2], 0.13]));
    let mut edges: Vec<V> = far.iter().flat_map(|c| [l.cam, *c]).collect();
    edges.extend((0..4).flat_map(|i| [far[i], far[(i + 1) % 4]]));
    add(Layer::Frustum, gl::lines(&edges), clip([0.55, 0.8, 1.0, 1.0]));
    add(Layer::Lines, gl::line_loop(&rect), clip([1.0, 0.65, 0.2, 1.0]));
    if l.b > 0.0 {
        let c2 = [-l.b, l.cam[1], 0.0];
        let e: Vec<V> = rect.iter().flat_map(|c| [c2, [c[0] - l.b, c[1], c[2]]]).collect();
        add(Layer::Frustum, gl::lines(&e), clip([0.9, 0.5, 0.9, 1.0]));
    }
    if let Some(v) = fv {
        let lift = |p: V, y: f32| [p[0], y, p[2]];
        let f = floor_footprint(o, &dirs, cap).map(|p| lift(rv(p), 0.004));
        add(Layer::Foot, gl::fan(&f), Paint::flat([0.3, 0.85, 0.45, 0.28]));
        add(Layer::Foot, gl::line_loop(&f.map(|p| lift(p, 0.005))), Paint::flat([0.4, 0.95, 0.55, 1.0]));
        if v.near > 1e-3 {
            let (br, bl) = (f[2], f[3]);
            let blind = [[bl[0], 0.004, 0.0], [br[0], 0.004, 0.0], br, bl];
            add(Layer::Foot, gl::fan(&blind), Paint::flat([0.95, 0.2, 0.2, 0.32]));
        }
    }
    if let Some((b, dd, dmax)) = sc.stereo {
        let fx = sc.cam.intrinsics().fx;
        let zmin = min_depth_m(fx, b, dmax);
        let step = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0].into_iter().find(|s| cap / s <= 10.0).unwrap_or(20.0);
        let w = (0.012 * cap) as f32;
        for i in 1..=(cap / step) as usize {
            let r = i as f64 * step;
            if r < zmin {
                continue;
            }
            let rel = depth_resolution_mm(fx, b, dd, r) / (r * 1000.0);
            // 0.5 % of the distance → green, 5 % → red (log scale).
            let t = ((rel / 0.005).log10() as f32).clamp(0.0, 1.0);
            let g = shift(gl::ring(r as f32 - w, r as f32 + w, 128), [l.mast_x, 0.003, 0.0]);
            add(Layer::Rings, g, Paint::flat(ramp(t)));
        }
    }
}

fn hud(sc: &Scene) {
    let fx = sc.cam.intrinsics().fx;
    let opx = sc.obj_mm * fx / (sc.z * 1000.0);
    let ok = if opx >= sc.minpx { "✅" } else { "⚠️" };
    let mut s = format!("{} {:.2} m\n{} {:.1} px {ok}", tr("distance", "거리"), sc.z, tr("pixels on target", "대상 픽셀"), opx);
    if let Some((b, dd, _)) = sc.stereo {
        s += &format!("\nΔz {:.1} mm", depth_resolution_mm(fx, b, dd, sc.z));
    }
    s += &match floor_view(sc.h, sc.tilt, sc.cam.vfov_deg()) {
        Some(v) => format!("\n{} {} ({} … {})", tr("footprint length", "바닥 영역 길이"), m(v.far - v.near), m(v.near), m(v.far)),
        None => format!("\n{}", tr("floor not visible", "바닥 안 보임")),
    };
    set_text("hud", &s);
    set_text("tlabel", &format!("{opx:.0} px"));
}

/// Returns false when WebGL2 is unavailable (caller keeps the 2D fallback).
pub fn set_scene(sc: Scene) -> bool {
    let ok = with(|st| {
        rebuild(st, &sc);
        if st.sc.is_none() {
            st.orbit = OrbitCam::new(fit(&sc));
        }
        st.sc = Some(sc);
    })
    .is_some();
    if ok {
        hud(&sc);
        show("legend", sc.stereo.is_some() && with(|st| st.show[2]).unwrap_or(false));
        request();
    }
    ok
}

fn request() {
    if with(|st| !std::mem::replace(&mut st.pending, true)) == Some(true) {
        RAF.with(|c| {
            if let Some(c) = c.borrow().as_ref() {
                let _ = web_sys::window().unwrap().request_animation_frame(c.as_ref().unchecked_ref());
            }
        });
    }
}

fn frame(t: f64) {
    let again = with(|st| {
        st.pending = false;
        let dt = if st.last_t > 0.0 { ((t - st.last_t) / 1000.0).clamp(0.0, 0.1) } else { 1.0 / 60.0 };
        st.last_t = t;
        let moving = render(st, dt as f32);
        if !moving {
            st.last_t = 0.0;
        }
        moving
    });
    if again == Some(true) {
        request();
    }
}

fn render(st: &mut St, dt: f32) -> bool {
    let Some(sc) = st.sc else { return false };
    let win = web_sys::window().unwrap();
    let dpr = win.device_pixel_ratio().max(1.0) as f32;
    let (cw, ch) = (st.canvas.client_width().max(1) as f32, st.canvas.client_height().max(1) as f32);
    let (pw, ph) = ((cw * dpr) as u32, (ch * dpr) as u32);
    if st.canvas.width() != pw || st.canvas.height() != ph {
        st.canvas.set_width(pw);
        st.canvas.set_height(ph);
    }
    let dark = win.match_media("(prefers-color-scheme: dark)").ok().flatten().is_some_and(|q| q.matches());
    st.r.theme = if dark { gl::DARK } else { gl::LIGHT };

    let moving = st.orbit.step(dt);
    let l = lay(&sc);
    let (eye, view, proj) = if st.fpv {
        // What the robot sees: the camera's own VFOV and image aspect, letterboxed.
        let ia = (sc.cam.res_w / sc.cam.res_h) as f32;
        let (w, h) = if cw / ch > ia { (ch * ia, ch) } else { (cw, cw / ia) };
        st.vp_css = [(cw - w) / 2.0, (ch - h) / 2.0, w, h];
        let vfov = (sc.cam.vfov_deg() as f32).to_radians();
        (l.cam, math::look_at(l.cam, math::add(l.cam, l.fwd), l.up), math::perspective(vfov, ia, 0.01, 1000.0))
    } else {
        st.vp_css = [0.0, 0.0, cw, ch];
        let o = st.orbit.cur;
        let eye = o.eye();
        (eye, math::look_at(eye, o.target, [0.0, 1.0, 0.0]), math::perspective(FOVY, cw / ch, (o.dist * 0.01).max(0.005), 1000.0))
    };
    st.vp = math::mul(&proj, &view);
    st.inv = math::invert(&st.vp).unwrap_or(math::IDENTITY);
    let d = st.orbit.cur.dist;
    st.r.fog = [d * 2.0 + 5.0, d * 8.0 + 30.0];
    let [x, y, w, h] = st.vp_css.map(|v| v * dpr);
    st.r.begin(&view, &proj, eye, [x as i32, (ph as f32 - y - h) as i32, w as i32, h as i32]);

    // Opaque scene objects (these cast shadows).
    let m = &st.m;
    let (k, ring_r) = sizes(d);
    let glow = |hd: Handle| if st.hover == Some(hd) || matches!(st.drag, Some(Drag::Handle(x, ..)) if x == hd) { 0.35 } else { 0.0 };
    let ax = |s: V, t: V| math::basis([1.0, 0.0, 0.0], l.up, l.fwd, s, t);
    let mut solid: Vec<(&Mesh, M4, Paint)> = vec![];
    let grey = Paint::lit([0.55, 0.57, 0.62, 1.0]);
    solid.push((&m.cube, math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [0.36, l.body_h, 0.3], [l.mast_x, l.body_h / 2.0, -0.12]), grey));
    let mast = l.cam[1] - l.body_h;
    if mast > 0.0 {
        let t = [l.mast_x, l.body_h + mast / 2.0, -0.04];
        solid.push((&m.cyl, math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [0.035, mast, 0.035], t), grey));
    }
    let tgt_r = 0.18 * l.tgt_h;
    let person = math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [tgt_r; 3], [l.tgt[0], l.tgt_h / 2.0, l.tgt[2]]);
    solid.push((&m.person, person, Paint { glow: glow(Handle::Target), ..Paint::lit([0.95, 0.55, 0.15, 1.0]) }));
    if !st.fpv {
        let body = math::add([l.mast_x, l.cam[1], 0.0], math::scale(l.fwd, -0.02));
        solid.push((&m.cube, ax([l.b + 0.06, 0.04, 0.035], body), Paint::lit([0.13, 0.14, 0.16, 1.0])));
        for lx in if l.b > 0.0 { vec![0.0, -l.b] } else { vec![0.0] } {
            let lens = math::add([lx, l.cam[1], 0.0], math::scale(l.fwd, 0.002));
            let mm = math::basis([1., 0., 0.], l.fwd, l.up, [0.026, 0.012, 0.026], lens);
            solid.push((&m.cyl, mm, Paint::lit([0.1, 0.22, 0.4, 1.0])));
        }
    }
    if st.show[3] {
        for (mesh, model, _) in &solid {
            st.r.shadow(mesh, model);
        }
    }
    for (layer, on) in [(Layer::Foot, st.show[1]), (Layer::Rings, st.show[2])] {
        for (_, mesh, p) in st.dynm.iter().filter(|e| e.0 == layer && on) {
            st.r.draw(mesh, &math::IDENTITY, *p);
        }
    }
    for (mesh, model, p) in &solid {
        st.r.draw(mesh, model, *p);
    }
    // Handles: knob on the mast (height), ring around the camera (tilt), disc around the target.
    let disc = math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [(tgt_r * 3.0).max(k * 2.5); 3], [l.tgt[0], 0.006, l.tgt[2]]);
    st.r.draw(&m.ring, &disc, Paint { glow: glow(Handle::Target), ..Paint::flat([1.0, 0.6, 0.15, 0.9]) });
    if !st.fpv {
        let knob = [l.mast_x, (l.cam[1] - 0.05 - k).max(l.body_h + k), -0.04];
        st.r.draw(&m.sphere, &math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [k; 3], knob), Paint { glow: glow(Handle::Height), ..Paint::lit([1.0, 0.62, 0.15, 1.0]) });
        let ring = math::basis([0., 0., 1.], [1., 0., 0.], [0., 1., 0.], [ring_r; 3], [l.mast_x, l.cam[1], 0.0]);
        let tilt = Paint { glow: glow(Handle::Tilt), ..Paint::flat([0.3, 0.8, 1.0, 0.9]) };
        st.r.draw(&m.ring, &ring, tilt);
        let tip = math::add([l.mast_x, l.cam[1], 0.0], math::scale(l.fwd, ring_r * 0.95));
        st.r.draw(&m.sphere, &math::basis([1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [k * 0.8; 3], tip), Paint { lit: true, ..tilt });
    }
    // Translucent volumes and edges last.
    for (layer, mesh, p) in &st.dynm {
        if *layer == Layer::Lines || (*layer == Layer::Frustum && st.show[0] && !st.fpv) {
            st.r.draw(mesh, &math::IDENTITY, *p);
        }
    }
    place_label(st, [l.tgt[0], l.tgt_h * 1.1, l.tgt[2]]);
    moving
}

/// Anchor the HTML pixel label above the target.
fn place_label(st: &St, p: V) {
    let el: HtmlElement = by_id("tlabel");
    let w = st.vp[3] * p[0] + st.vp[7] * p[1] + st.vp[11] * p[2] + st.vp[15];
    el.set_hidden(w <= 0.0);
    let n = math::transform(&st.vp, p);
    let [x, y, vw, vh] = st.vp_css;
    let s = el.style();
    let _ = s.set_property("left", &format!("{}px", x + (n[0] + 1.0) / 2.0 * vw));
    let _ = s.set_property("top", &format!("{}px", y + (1.0 - n[1]) / 2.0 * vh));
}

fn ndc(st: &St, cx: f32, cy: f32) -> (f32, f32) {
    let r = st.canvas.get_bounding_client_rect();
    let [x, y, w, h] = st.vp_css;
    (2.0 * (cx - r.left() as f32 - x) / w - 1.0, 1.0 - 2.0 * (cy - r.top() as f32 - y) / h)
}

/// Nearest draggable thing under the pointer, with the world point that was hit.
fn pick(st: &St, n: (f32, f32)) -> Option<(Handle, V)> {
    let sc = st.sc?;
    let l = lay(&sc);
    let (o, d) = math::ndc_ray(&st.inv, n.0, n.1);
    let (k, ring_r) = sizes(st.orbit.cur.dist);
    let at = |t: f32| math::add(o, math::scale(d, t));
    let mut hits: Vec<(f32, Handle, V)> = vec![];
    let tr = (0.18 * l.tgt_h * 3.0).max(k * 2.5);
    if let Some(t) = math::ray_sphere(o, d, [l.tgt[0], l.tgt_h / 2.0, l.tgt[2]], (l.tgt_h * 0.6).max(tr)) {
        hits.push((t, Handle::Target, at(t)));
    }
    if !st.fpv {
        let knob = [l.mast_x, (l.cam[1] - 0.05 - k).max(l.body_h + k), -0.04];
        if let Some(t) = math::ray_sphere(o, d, knob, k * 1.8) {
            hits.push((t, Handle::Height, at(t)));
        }
        let c = [l.mast_x, l.cam[1], 0.0];
        if let Some(p) = math::ray_plane(o, d, c, [1.0, 0.0, 0.0]) {
            let r = math::dot(math::sub(p, c), math::sub(p, c)).sqrt();
            if (r - ring_r).abs() < ring_r * 0.3 {
                hits.push((math::dot(math::sub(p, o), d), Handle::Tilt, p));
            }
        }
    }
    hits.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).map(|h| (h.1, h.2))
}

/// New input value while dragging a handle.
fn drag_value(st: &St, hd: Handle, start: V, v0: f64, n: (f32, f32)) -> Option<(&'static str, f64)> {
    let sc = st.sc?;
    let l = lay(&sc);
    let (o, d) = math::ndc_ray(&st.inv, n.0, n.1);
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    match hd {
        Handle::Target => {
            let p = math::ray_plane(o, d, [0.0; 3], [0.0, 1.0, 0.0])?;
            let c = sc.tilt.to_radians().cos().max(0.05);
            Some(("z", r3((p[2] as f64 / c).max(0.05))))
        }
        Handle::Height => {
            let e = math::sub(o, start);
            let nrm = math::norm([e[0], 0.0, e[2]]);
            let p = math::ray_plane(o, d, start, nrm)?;
            Some(("h", r3((v0 + (p[1] - start[1]) as f64).max(0.0))))
        }
        Handle::Tilt => {
            let c = [l.mast_x, l.cam[1], 0.0];
            let p = math::ray_plane(o, d, c, [1.0, 0.0, 0.0])?;
            let ang = |p: V| (-(p[1] - c[1])).atan2(p[2] - c[2]).to_degrees() as f64;
            let da = (ang(p) - ang(start) + 540.0).rem_euclid(360.0) - 180.0;
            Some(("tilt", ((v0 + da).clamp(-89.0, 90.0) * 10.0).round() / 10.0))
        }
    }
}

fn set_cursor(st: &St) {
    let c = match (st.drag, st.hover) {
        (Some(Drag::Orbit | Drag::Pan), _) => "grabbing",
        (_, Some(Handle::Target)) => "move",
        (_, Some(Handle::Height)) => "ns-resize",
        (_, Some(Handle::Tilt)) => "pointer",
        _ => "grab",
    };
    let _ = st.canvas.style().set_property("cursor", c);
}

/// Glide to a preset view; "fpv" switches to the robot camera.
fn preset(st: &mut St, name: &str) {
    let Some(sc) = st.sc else { return };
    let f = fit(&sc);
    st.fpv = name == "v_fpv";
    let (yaw, pitch, dist, ty) = match name {
        "v_top" => (PI, math::PITCH_MAX, f.dist * 1.1, 0.0),
        "v_side" => (-PI / 2.0, 0.08, f.dist, f.target[1]),
        _ => (f.yaw, f.pitch, f.dist, f.target[1]),
    };
    // Shortest way round to the new yaw.
    let cur = st.orbit.goal.yaw;
    let yaw = cur + ((yaw - cur + PI).rem_euclid(TAU) - PI);
    st.orbit.set_goal(Orbit { target: [f.target[0], ty, f.target[2]], yaw, pitch, dist });
    for v in VIEWS {
        let _ = by_id::<Element>(v).class_list().toggle_with_force("on", v == name || (v == "v_persp" && name == "reset"));
    }
}

/// Leave the first-person view when the user starts orbiting / zooming.
fn leave_fpv(st: &mut St) {
    if st.fpv {
        st.fpv = false;
        for v in VIEWS {
            let _ = by_id::<Element>(v).class_list().toggle_with_force("on", false);
        }
    }
}

fn pan(st: &mut St, dx: f32, dy: f32) {
    let o = st.orbit.cur;
    let f = math::norm(math::sub(o.target, o.eye()));
    let r = math::norm(math::cross(f, [0.0, 1.0, 0.0]));
    let u = math::cross(r, f);
    let k = 2.0 * o.dist * (FOVY / 2.0).tan() / st.vp_css[3].max(1.0);
    st.orbit.pan(math::add(math::scale(r, -dx * k), math::scale(u, dy * k)));
}

fn listen<E: JsCast + 'static>(target: &web_sys::EventTarget, ev: &str, passive: bool, mut f: impl FnMut(E) + 'static) {
    let c = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| f(e.unchecked_into()));
    let opts = web_sys::AddEventListenerOptions::new();
    opts.set_passive(passive);
    target.add_event_listener_with_callback_and_add_event_listener_options(ev, c.as_ref().unchecked_ref(), &opts).unwrap();
    c.forget();
}

fn on_down(e: PointerEvent) {
    let (x, y) = (e.client_x() as f32, e.client_y() as f32);
    with(|st| {
        let _ = st.canvas.set_pointer_capture(e.pointer_id());
        st.ptrs.retain(|p| p.0 != e.pointer_id());
        st.ptrs.push((e.pointer_id(), x, y));
        st.orbit.held = true;
        if st.ptrs.len() >= 2 {
            st.drag = Some(Drag::Pan);
            return;
        }
        let now = e.time_stamp();
        if now - st.last_down < 300.0 {
            // Double click / double tap: reset the view.
            st.last_down = 0.0;
            leave_fpv(st);
            preset(st, "reset");
            st.drag = None;
            return;
        }
        st.last_down = now;
        st.drag = Some(match (e.button(), pick(st, ndc(st, x, y)), st.sc) {
            (2, ..) => Drag::Pan,
            (_, Some((hd, hit)), Some(sc)) => {
                let v0 = match hd {
                    Handle::Target => sc.z,
                    Handle::Height => sc.h,
                    Handle::Tilt => sc.tilt,
                };
                Drag::Handle(hd, hit, v0)
            }
            _ => Drag::Orbit,
        });
        set_cursor(st);
    });
    request();
}

fn on_move(e: PointerEvent) {
    let (x, y) = (e.client_x() as f32, e.client_y() as f32);
    let set = with(|st| {
        let i = st.ptrs.iter().position(|p| p.0 == e.pointer_id());
        let (lx, ly) = i.map_or((x, y), |i| (st.ptrs[i].1, st.ptrs[i].2));
        let spread = |ps: &[(i32, f32, f32)]| (ps[0].1 - ps[1].1).hypot(ps[0].2 - ps[1].2);
        let before = (st.ptrs.len() == 2).then(|| spread(&st.ptrs));
        if let Some(i) = i {
            st.ptrs[i] = (e.pointer_id(), x, y);
        }
        let (dx, dy) = (x - lx, y - ly);
        match st.drag {
            None => {
                let h = pick(st, ndc(st, x, y)).map(|h| h.0);
                if h != st.hover {
                    st.hover = h;
                    set_cursor(st);
                    return Some(None);
                }
                None
            }
            Some(Drag::Orbit) => {
                leave_fpv(st);
                st.orbit.rotate(-dx * 0.008, dy * 0.008);
                Some(None)
            }
            Some(Drag::Pan) => {
                leave_fpv(st);
                // Two fingers: pan by half the moved finger (= centroid), pinch by spread ratio.
                let k = if before.is_some() { 0.5 } else { 1.0 };
                pan(st, dx * k, dy * k);
                if let Some(b) = before {
                    st.orbit.zoom(b / spread(&st.ptrs).max(1.0));
                }
                Some(None)
            }
            Some(Drag::Handle(hd, start, v0)) => Some(drag_value(st, hd, start, v0, ndc(st, x, y))),
        }
    })
    .flatten();
    match set {
        Some(Some((id, v))) => {
            set_num(id, v);
            crate::update();
        }
        Some(None) => request(),
        None => {}
    }
}

fn on_up(e: PointerEvent) {
    with(|st| {
        st.ptrs.retain(|p| p.0 != e.pointer_id());
        // Lifting one finger of a pinch must not turn into an orbit jump.
        st.drag = None;
        st.orbit.held = !st.ptrs.is_empty();
        set_cursor(st);
    });
    request();
}

/// Set up the WebGL2 view. Returns false if WebGL2 is unavailable.
pub fn init() -> bool {
    let canvas: HtmlCanvasElement = by_id("view3d");
    let Some(r) = Renderer::new(&canvas) else { return false };
    let mk = |g: Geo| r.mesh(&g);
    let Some(m) = (|| {
        Some(Meshes { cube: mk(gl::cube())?, cyl: mk(gl::cylinder(32))?, person: mk(gl::capsule(16, 1.78))?, sphere: mk(gl::sphere(16))?, ring: mk(gl::ring(0.88, 1.0, 96))? })
    })() else {
        return false;
    };
    let o = Orbit { target: [0.0; 3], yaw: 2.4, pitch: 0.45, dist: 4.0 };
    let st = St {
        r,
        canvas: canvas.clone(),
        m,
        dynm: vec![],
        sc: None,
        orbit: OrbitCam::new(o),
        fpv: false,
        show: [true; 4],
        hover: None,
        drag: None,
        ptrs: vec![],
        last_down: 0.0,
        vp_css: [0.0, 0.0, 1.0, 1.0],
        vp: math::IDENTITY,
        inv: math::IDENTITY,
        pending: false,
        last_t: 0.0,
    };
    ST.with(|s| *s.borrow_mut() = Some(st));
    RAF.with(|c| *c.borrow_mut() = Some(Closure::new(frame)));

    let t: &web_sys::EventTarget = canvas.as_ref();
    listen(t, "pointerdown", true, on_down);
    listen(t, "pointermove", true, on_move);
    listen(t, "pointerup", true, on_up);
    listen(t, "pointercancel", true, on_up);
    listen(t, "pointerleave", true, |_: PointerEvent| {
        if with(|st| std::mem::take(&mut st.hover).is_some()) == Some(true) {
            request();
        }
    });
    listen(t, "contextmenu", false, |e: web_sys::Event| e.prevent_default());
    listen(t, "wheel", false, |e: WheelEvent| {
        e.prevent_default();
        with(|st| {
            leave_fpv(st);
            st.orbit.zoom((e.delta_y() as f32 * 0.0015).clamp(-0.5, 0.5).exp());
        });
        request();
    });
    let ro = Closure::<dyn FnMut()>::new(request);
    if let Ok(obs) = web_sys::ResizeObserver::new(ro.as_ref().unchecked_ref()) {
        obs.observe(&canvas);
    }
    ro.forget();
    for (i, id) in TOGGLES.into_iter().enumerate() {
        crate::on(id, "click", move |_| {
            let on = with(|st| {
                st.show[i] = !st.show[i];
                st.show[i]
            });
            let _ = by_id::<Element>(id).class_list().toggle_with_force("on", on == Some(true));
            if i == 2 {
                let stereo = with(|st| st.sc.is_some_and(|s| s.stereo.is_some())) == Some(true);
                show("legend", stereo && on == Some(true));
            }
            request();
        });
    }
    for id in VIEWS {
        crate::on(id, "click", move |_| {
            with(|st| preset(st, id));
            request();
        });
    }
    true
}
