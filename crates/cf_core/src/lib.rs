//! Ideal pinhole-camera and stereo geometry for robot camera planning.
//!
//! Units: lengths on the sensor in mm, pixel pitch in mm internally, scene distances in metres,
//! object sizes and stereo baseline in mm, angles in degrees at the API boundary.
//! Model: no lens distortion, flat floor, principal point at the image centre.

const EPS: f64 = 1e-9;

/// Pinhole camera described by its sensor, resolution and focal length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub sensor_w_mm: f64,
    pub sensor_h_mm: f64,
    pub res_w: f64,
    pub res_h: f64,
    pub focal_mm: f64,
}

/// Intrinsics matrix entries in pixels. cx, cy use the pixel-edge convention (W/2, H/2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Intrinsics {
    pub fx: f64,
    pub fy: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Camera {
    /// Build from a square pixel pitch (µm) and resolution.
    pub fn from_pitch(pitch_um: f64, res_w: f64, res_h: f64, focal_mm: f64) -> Self {
        let p = pitch_um / 1000.0;
        Camera { sensor_w_mm: p * res_w, sensor_h_mm: p * res_h, res_w, res_h, focal_mm }
    }

    pub fn pitch_x_mm(&self) -> f64 {
        self.sensor_w_mm / self.res_w
    }

    pub fn pitch_y_mm(&self) -> f64 {
        self.sensor_h_mm / self.res_h
    }

    pub fn intrinsics(&self) -> Intrinsics {
        Intrinsics {
            fx: self.focal_mm / self.pitch_x_mm(),
            fy: self.focal_mm / self.pitch_y_mm(),
            cx: self.res_w / 2.0,
            cy: self.res_h / 2.0,
        }
    }

    pub fn hfov_deg(&self) -> f64 {
        fov_deg(self.sensor_w_mm, self.focal_mm)
    }

    pub fn vfov_deg(&self) -> f64 {
        fov_deg(self.sensor_h_mm, self.focal_mm)
    }

    pub fn dfov_deg(&self) -> f64 {
        fov_deg(self.sensor_w_mm.hypot(self.sensor_h_mm), self.focal_mm)
    }

    /// Footprint (width, height) in metres of the image plane at distance `z_m` along the axis.
    pub fn footprint_m(&self, z_m: f64) -> (f64, f64) {
        (footprint(self.hfov_deg(), z_m), footprint(self.vfov_deg(), z_m))
    }

    /// Horizontal pixels per millimetre on a fronto-parallel target at `z_m`.
    pub fn px_per_mm(&self, z_m: f64) -> f64 {
        self.intrinsics().fx / (z_m * 1000.0)
    }

    pub fn valid(&self) -> bool {
        [self.sensor_w_mm, self.sensor_h_mm, self.res_w, self.res_h, self.focal_mm]
            .iter()
            .all(|v| v.is_finite() && *v > 0.0)
    }
}

/// Full field of view (deg) for a sensor extent and focal length (same units).
pub fn fov_deg(extent: f64, focal: f64) -> f64 {
    2.0 * (extent / (2.0 * focal)).atan().to_degrees()
}

/// Extent covered by a full FOV at distance z: 2·z·tan(FOV/2).
pub fn footprint(fov_deg: f64, z: f64) -> f64 {
    2.0 * z * (fov_deg.to_radians() / 2.0).tan()
}

/// Floor distances (m, horizontal from the point below the camera) seen by the vertical FOV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloorView {
    /// Nearest visible floor point = blind zone in front of the robot.
    pub near: f64,
    /// Farthest visible floor point; `f64::INFINITY` if the upper ray is at or above the horizon.
    pub far: f64,
}

/// `tilt_deg` is downward tilt (0 = horizontal). Returns `None` if no ray hits the floor.
pub fn floor_view(height_m: f64, tilt_deg: f64, vfov_deg: f64) -> Option<FloorView> {
    let lower = (tilt_deg + vfov_deg / 2.0).to_radians();
    let upper = (tilt_deg - vfov_deg / 2.0).to_radians();
    if lower <= EPS || height_m <= 0.0 {
        return None;
    }
    let hit = |a: f64| if a >= std::f64::consts::FRAC_PI_2 { 0.0 } else { height_m / a.tan() };
    let near = hit(lower);
    let far = if upper <= 1e-6 { f64::INFINITY } else { hit(upper) };
    Some(FloorView { near, far })
}

/// Stereo depth resolution Δz (mm) at `z_m`: z²·Δd / (f_px·B).
pub fn depth_resolution_mm(f_px: f64, baseline_mm: f64, disp_err_px: f64, z_m: f64) -> f64 {
    let z = z_m * 1000.0;
    z * z * disp_err_px / (f_px * baseline_mm)
}

/// Minimum measurable depth (m) for a disparity search range: f_px·B / d_max.
pub fn min_depth_m(f_px: f64, baseline_mm: f64, max_disp_px: f64) -> f64 {
    f_px * baseline_mm / max_disp_px / 1000.0
}

/// Simple 3-vector used for frustum geometry. World frame: x right, y up, z forward.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct V3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl V3 {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        V3 { x, y, z }
    }
    pub fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn scale(self, s: f64) -> V3 {
        V3::new(self.x * s, self.y * s, self.z * s)
    }
}

/// Camera-frame direction (x right, y up, z forward) rotated by a downward tilt into the world.
pub fn tilt_dir(d: V3, tilt_deg: f64) -> V3 {
    let (s, c) = tilt_deg.to_radians().sin_cos();
    V3::new(d.x, d.y * c - d.z * s, d.y * s + d.z * c)
}

/// The four image-corner ray directions (unit z in camera frame), order TL, TR, BR, BL, in world.
pub fn corner_dirs(cam: &Camera, tilt_deg: f64) -> [V3; 4] {
    let tx = (cam.hfov_deg().to_radians() / 2.0).tan();
    let ty = (cam.vfov_deg().to_radians() / 2.0).tan();
    [(-tx, ty), (tx, ty), (tx, -ty), (-tx, -ty)].map(|(x, y)| tilt_dir(V3::new(x, y, 1.0), tilt_deg))
}

/// Where a ray from `origin` along `dir` meets the floor (y = 0), capped at horizontal range
/// `max_range` (so rays at/above the horizon still give a drawable far edge).
pub fn floor_hit(origin: V3, dir: V3, max_range: f64) -> V3 {
    let horiz = dir.x.hypot(dir.z).max(EPS);
    let t_cap = max_range / horiz;
    let t = if dir.y < -EPS { (-origin.y / dir.y).min(t_cap) } else { t_cap };
    let p = origin.add(dir.scale(t));
    V3::new(p.x, p.y.max(0.0), p.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "{a} vs {b}");
    }

    #[test]
    fn hfov_full_frame_18mm_is_90() {
        let cam = Camera { sensor_w_mm: 36.0, sensor_h_mm: 24.0, res_w: 6000.0, res_h: 4000.0, focal_mm: 18.0 };
        close(cam.hfov_deg(), 90.0, 1e-9);
        close(cam.vfov_deg(), 2.0 * (12.0f64 / 18.0).atan().to_degrees(), 1e-9);
    }

    #[test]
    fn footprint_is_two_z_tan_half_fov() {
        let cam = Camera { sensor_w_mm: 36.0, sensor_h_mm: 24.0, res_w: 6000.0, res_h: 4000.0, focal_mm: 18.0 };
        let (w, _) = cam.footprint_m(1.5);
        close(w, 2.0 * 1.5 * 45f64.to_radians().tan(), 1e-9); // = 3.0 m
        close(w, 3.0, 1e-9);
    }

    #[test]
    fn focal_px_from_pitch() {
        // 3 µm pitch, 2.1 mm lens -> 700 px; square pixels -> fx == fy.
        let cam = Camera::from_pitch(3.0, 1280.0, 800.0, 2.1);
        let k = cam.intrinsics();
        close(k.fx, 700.0, 1e-9);
        close(k.fy, 700.0, 1e-9);
        close(k.cx, 640.0, 1e-12);
        close(cam.sensor_w_mm, 3.84, 1e-12);
        // 700 px / 2000 mm = 0.35 px/mm, so a 20 mm object spans 7 px at 2 m.
        close(cam.px_per_mm(2.0) * 20.0, 7.0, 1e-9);
    }

    #[test]
    fn stereo_depth_resolution_and_min_depth() {
        close(depth_resolution_mm(700.0, 60.0, 0.25, 2.0), 23.8095, 1e-3);
        close(min_depth_m(700.0, 60.0, 128.0), 700.0 * 60.0 / 128.0 / 1000.0, 1e-12); // 0.328 m
    }

    #[test]
    fn horizontal_camera_sees_to_infinity() {
        let v = floor_view(1.2, 0.0, 40.0).unwrap();
        assert!(v.far.is_infinite());
        close(v.near, 1.2 / 20f64.to_radians().tan(), 1e-9);
        // Upper ray exactly at the horizon also counts as infinite.
        assert!(floor_view(1.0, 15.0, 30.0).unwrap().far.is_infinite());
        // Camera tilted up so that no ray goes below the horizon.
        assert!(floor_view(1.0, -20.0, 30.0).is_none());
    }

    #[test]
    fn tilt_45_vfov_30_at_1m() {
        let v = floor_view(1.0, 45.0, 30.0).unwrap();
        close(v.near, 0.5774, 1e-3);
        close(v.far, 1.7321, 1e-3);
    }

    #[test]
    fn frustum_floor_hits_agree_with_floor_view() {
        let cam = Camera { sensor_w_mm: 4.0, sensor_h_mm: 3.0, res_w: 640.0, res_h: 480.0, focal_mm: 4.0 };
        let origin = V3::new(0.0, 1.0, 0.0);
        let d = corner_dirs(&cam, 45.0);
        let v = floor_view(1.0, 45.0, cam.vfov_deg()).unwrap();
        let bl = floor_hit(origin, d[3], 100.0);
        let tl = floor_hit(origin, d[0], 100.0);
        close(bl.y, 0.0, 1e-9);
        close(bl.z, v.near, 1e-9);
        close(tl.z, v.far, 1e-9);
    }
}
