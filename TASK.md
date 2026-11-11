# Task: Robot camera FOV & stereo depth planner (Rust → WebAssembly, static web app)

Original open-source tool. Write all code yourself; do not copy anyone's code, tables or assets.
Use textbook pinhole-camera / stereo geometry only. Presets are generic, clearly labelled "typical"
values that users can edit (no vendor part numbers or brand names).

## Product (SMALL scope — keep the whole tool ~400-800 lines of Rust)
A single-page web app (GitHub Pages, sub-path `/cam-fov/`): "I am mounting a camera on a robot —
what does it see, how many pixels land on my object, and (for stereo) how accurate is depth?"

1. **Camera (pinhole)**: sensor width & height (mm) OR pixel pitch (µm) × resolution (px), resolution
   W×H (px), focal length (mm). Derived: horizontal / vertical / diagonal FOV (deg), focal length in
   pixels f_px = f_mm / pitch_mm, intrinsics matrix K (fx, fy, cx, cy) shown as text.
   Two presets: "wide robot head camera (typical)" and "narrow gripper camera (typical)" (editable).
2. **Mount + target**: camera height above floor (m), downward tilt (deg, 0 = horizontal), target
   distance along the optical axis (m), target object size (mm). Outputs: footprint width × height at the
   target distance, pixels per mm on target, object size in pixels (and ✅ ≥ user min px, default 20 px,
   else ⚠️), floor visibility: nearest and farthest floor distance seen by the vertical FOV (∞ if the upper
   ray is at or above horizon), blind zone in front of the robot.
3. **Stereo (extra)**: checkbox "stereo pair", baseline B (mm), disparity matching accuracy (px, default
   0.25), min disparity search range (max disparity, px, default 128). Outputs: depth resolution
   Δz = z²·Δd/(f_px·B) at the target distance, minimum measurable depth z_min = f_px·B/d_max, and a canvas
   plot of Δz (mm) vs distance from z_min to 3× target distance with the target marked.
4. **3D view** (canvas 2D, own `view3d.rs`, small perspective projection, no WebGL/libs): floor grid,
   camera body, view frustum out to the target distance, the target plane rectangle with the object as a
   small box, floor footprint (trapezoid where the frustum hits the floor), and for stereo a second camera
   offset by B. Drag to orbit (pointer events, `touch-action:none`), auto-fit scale. A distance slider
   moves the target plane and updates the metrics live (the same distance shown as a vertical cursor on
   the Δz plot).
5. Korean + English toggle (browser default). Mobile friendly. CSS prefers-color-scheme only.
Out of scope (do NOT build): lens distortion, depth of field, URL share, JSON import/export,
image upload, multiple cameras beyond one stereo pair.

## Core crate (pure Rust, no web deps, unit tested) — `crates/cf_core`
- Camera, FOV, footprint, pixel density, floor intersection, stereo depth error.
- Tests (hand-checkable, 4–8): 36 mm wide sensor with 18 mm lens → HFOV 90°; footprint width at
  distance z = 2·z·tan(HFOV/2); f_px from pitch; Δz formula (e.g. f_px=700, B=60 mm, Δd=0.25 px,
  z=2 m → ≈ 23.8 mm); z_min = f_px·B/d_max; horizontal camera with VFOV/2 above horizon → far floor = ∞
  and near floor = h / tan(VFOV/2); tilt 45° with VFOV 30° at h=1 m → near ≈ 0.577 m, far ≈ 1.732 m.
- `crates/cf_web`: UI. Plain web-sys — keep the bundle small.

## Repo hygiene
- README (Korean first, then English): what it is, model and assumptions (ideal pinhole, no
  distortion, flat floor, square pixels if only pitch given), how to use, local build
  (`cargo test -p cf_core`, `trunk build --release`), MIT license, "estimates only — verify with a
  calibration".
- LICENSE: MIT, copyright "cam-fov contributors". `.gitignore` for target/ dist/.
- `index.html` with a CSP meta that blocks external requests:
  `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'; object-src 'none'; base-uri 'self'`.
  In `Trunk.toml` set `filehash = false` and
  `pattern_script = "<script type=\"module\" src=\"{base}boot.js\"></script>"` with an external
  `web/boot.js` (copied via `<link data-trunk rel="copy-file">`) that imports `./cf_web.js` and calls
  init, so no inline script is needed.
- `Trunk.toml`: `public_url = "/cam-fov/"`.
- `.github/workflows/pages.yml`: on push to main + workflow_dispatch; checkout, rust stable + wasm32
  target, install trunk 0.22.1 binary release, `sh tools/privacy_check.sh`,
  `cargo test -p cf_core --release`, `trunk build --release`, configure-pages@v5,
  upload-pages-artifact@v3 (dist), deploy job with actions/deploy-pages@v4
  (permissions pages: write, id-token: write).
- `tools/privacy_check.sh` already exists; keep it and keep it clean.

## Done when
- `cargo test -p cf_core` green; `trunk build --release` succeeds; `sh tools/privacy_check.sh` clean.
- Commit everything with clear commits; open a PR. Do NOT create new GitHub repos or change repo settings.
