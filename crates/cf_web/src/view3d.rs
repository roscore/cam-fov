//! Tiny orbit-camera perspective renderer on a 2D canvas (no WebGL).
use crate::css_var;
use cf_core::{corner_dirs, floor_hit, floor_view, tilt_dir, Camera, V3};
use web_sys::CanvasRenderingContext2d as Ctx;

pub struct Scene {
    pub cam: Camera,
    pub h: f64,
    pub tilt: f64,
    pub z: f64,
    pub obj_mm: f64,
    pub baseline_mm: Option<f64>,
}

struct Proj {
    c: V3,
    cy: f64,
    sy: f64,
    cp: f64,
    sp: f64,
    d: f64,
    s: f64,
    ox: f64,
    oy: f64,
}

impl Proj {
    /// Rotate around the scene centre (yaw about y, then pitch), perspective-divide.
    fn raw(&self, p: V3) -> Option<(f64, f64)> {
        let (x, y, z) = (p.x - self.c.x, p.y - self.c.y, p.z - self.c.z);
        let x1 = x * self.cy - z * self.sy;
        let z1 = x * self.sy + z * self.cy;
        let y2 = y * self.cp + z1 * self.sp;
        let z2 = -y * self.sp + z1 * self.cp;
        let den = self.d + z2;
        (den > 1e-3 * self.d).then(|| (x1 * self.d / den, -y2 * self.d / den))
    }
    fn pt(&self, p: V3) -> Option<(f64, f64)> {
        self.raw(p).map(|(u, v)| (self.ox + u * self.s, self.oy + v * self.s))
    }
}

fn poly(ctx: &Ctx, pr: &Proj, pts: &[V3], close: bool) {
    ctx.begin_path();
    let mut first = true;
    for p in pts {
        if let Some((x, y)) = pr.pt(*p) {
            if first { ctx.move_to(x, y) } else { ctx.line_to(x, y) }
            first = false;
        }
    }
    if close {
        ctx.close_path();
    }
}

fn line(ctx: &Ctx, pr: &Proj, a: V3, b: V3) {
    poly(ctx, pr, &[a, b], false);
    ctx.stroke();
}

/// 12 edges of a box given its 8 corners (bit 0 = x, bit 1 = y, bit 2 = z).
fn box_edges(ctx: &Ctx, pr: &Proj, c: &[V3; 8]) {
    for i in 0..8 {
        for bit in [1, 2, 4] {
            if i & bit == 0 {
                line(ctx, pr, c[i], c[i | bit]);
            }
        }
    }
}

/// Box centred at `centre`, axes aligned to the tilted camera frame, half size `s` (m).
fn cam_box(centre: V3, s: V3, tilt: f64) -> [V3; 8] {
    std::array::from_fn(|i| {
        let sg = |b: usize, v: f64| if i & b != 0 { v } else { -v };
        centre.add(tilt_dir(V3::new(sg(1, s.x), sg(2, s.y), sg(4, s.z)), tilt))
    })
}

pub fn draw(ctx: &Ctx, w: f64, h: f64, sc: &Scene, yaw: f64, pitch: f64) {
    let o = V3::new(0.0, sc.h, 0.0);
    let dirs = corner_dirs(&sc.cam, sc.tilt);
    let rect: Vec<V3> = dirs.iter().map(|d| o.add(d.scale(sc.z))).collect();
    let fv = floor_view(sc.h, sc.tilt, sc.cam.vfov_deg());
    let range = (3.0 * sc.z).max(1.0);
    let foot: Vec<V3> = if fv.is_some() { dirs.iter().map(|d| floor_hit(o, *d, range)).collect() } else { vec![] };
    let b = sc.baseline_mm.unwrap_or(0.0) / 1000.0;
    let o2 = V3::new(b, sc.h, 0.0);

    // Grid bounds: cover everything interesting, snapped to 0.5 m.
    let key: Vec<V3> = [o, o2].iter().chain(&rect).chain(&foot).copied().collect();
    let snap = |v: f64, up: bool| if up { (v / 0.5).ceil() * 0.5 } else { (v / 0.5).floor() * 0.5 };
    let (mut x0, mut x1, mut z0, mut z1) = (-0.5f64, 0.5f64, -0.5f64, 0.5f64);
    for p in &key {
        x0 = x0.min(p.x);
        x1 = x1.max(p.x);
        z0 = z0.min(p.z);
        z1 = z1.max(p.z);
    }
    let (x0, x1, z0, z1) = (snap(x0, false), snap(x1, true), snap(z0, false), snap(z1, true));
    let c = V3::new((x0 + x1) / 2.0, sc.h / 2.0, (z0 + z1) / 2.0);
    let rad = key.iter().map(|p| (p.x - c.x).hypot(p.y - c.y).hypot(p.z - c.z)).fold(0.1, f64::max).max((x1 - x0).hypot(z1 - z0) / 2.0);
    let mut pr = Proj { c, cy: yaw.cos(), sy: yaw.sin(), cp: pitch.cos(), sp: pitch.sin(), d: 3.0 * rad, s: 1.0, ox: 0.0, oy: 0.0 };

    // Auto-fit: bounding box of the projected grid corners and key points.
    let corners = [V3::new(x0, 0.0, z0), V3::new(x1, 0.0, z0), V3::new(x1, 0.0, z1), V3::new(x0, 0.0, z1)];
    let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (u, v) in key.iter().chain(&corners).filter_map(|p| pr.raw(*p)) {
        u0 = u0.min(u);
        u1 = u1.max(u);
        v0 = v0.min(v);
        v1 = v1.max(v);
    }
    let m = 16.0;
    pr.s = ((w - 2.0 * m) / (u1 - u0).max(1e-6)).min((h - 2.0 * m) / (v1 - v0).max(1e-6));
    pr.ox = w / 2.0 - (u0 + u1) / 2.0 * pr.s;
    pr.oy = h / 2.0 - (v0 + v1) / 2.0 * pr.s;

    ctx.set_line_width(1.0);
    ctx.set_stroke_style_str(&css_var("--grid"));
    let n = ((x1 - x0) / 0.5).round() as i32;
    for i in 0..=n {
        let x = x0 + i as f64 * 0.5;
        line(ctx, &pr, V3::new(x, 0.0, z0), V3::new(x, 0.0, z1));
    }
    let n = ((z1 - z0) / 0.5).round() as i32;
    for i in 0..=n {
        let z = z0 + i as f64 * 0.5;
        line(ctx, &pr, V3::new(x0, 0.0, z), V3::new(x1, 0.0, z));
    }

    // Floor footprint (trapezoid, far edge capped at 3× target distance if it reaches the horizon).
    if !foot.is_empty() {
        let col = css_var("--floor");
        ctx.set_fill_style_str(&col);
        ctx.set_stroke_style_str(&col);
        ctx.set_global_alpha(0.25);
        poly(ctx, &pr, &foot, true);
        ctx.fill();
        ctx.set_global_alpha(1.0);
        ctx.stroke();
    }

    // Cameras: mast, body, frustum.
    let fg = css_var("--fg");
    let s = 0.03 * rad;
    let mut cams = vec![(o, css_var("--frustum"))];
    if sc.baseline_mm.is_some() {
        cams.push((o2, css_var("--cam2")));
    }
    for (i, (org, col)) in cams.iter().enumerate() {
        ctx.set_stroke_style_str(&fg);
        line(ctx, &pr, *org, V3::new(org.x, 0.0, org.z));
        ctx.set_stroke_style_str(col);
        ctx.set_line_width(1.5);
        box_edges(ctx, &pr, &cam_box(org.add(tilt_dir(V3::new(0.0, 0.0, -s), sc.tilt)), V3::new(s, s * 0.7, s), sc.tilt));
        ctx.set_line_width(1.0);
        let off = V3::new(org.x, 0.0, 0.0);
        if i > 0 {
            ctx.set_global_alpha(0.5);
        }
        for p in &rect {
            line(ctx, &pr, *org, p.add(off));
        }
        ctx.set_global_alpha(1.0);
    }

    // Target plane and object box at the target distance.
    let tgt = css_var("--target");
    ctx.set_stroke_style_str(&tgt);
    ctx.set_line_width(2.0);
    poly(ctx, &pr, &rect, true);
    ctx.stroke();
    let hs = sc.obj_mm / 2000.0;
    let centre = o.add(tilt_dir(V3::new(0.0, 0.0, sc.z), sc.tilt));
    ctx.set_fill_style_str(&tgt);
    box_edges(ctx, &pr, &cam_box(centre, V3::new(hs, hs, hs), sc.tilt));
    if let Some((x, y)) = pr.pt(centre) {
        ctx.fill_rect(x - 2.0, y - 2.0, 4.0, 4.0);
    }
}
