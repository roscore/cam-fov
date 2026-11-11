# TASK-3D: game-grade 3D view for cam-fov

Goal: the 3D view must feel like the MuJoCo viewer or a small game scene, not a wireframe sketch.
Replace the canvas-2D `crates/cf_web/src/view3d.rs` with a WebGL2 renderer. All existing
calculations, inputs, plots, i18n and tests stay as they are.

## Rendering (WebGL2 via web-sys, no JS libraries, no external assets, no CDN)
- New module `crates/cf_web/src/gl.rs`: a small reusable renderer with NO cam-fov specific code
  (later tools will copy it). Budget about 700 lines.
  - Meshes generated in code: box, cylinder, sphere, capsule, plane, frustum. Normals included.
  - Lighting: one directional key light + hemisphere ambient (sky/ground colors) + Blinn-Phong
    specular. Optional per-mesh alpha for translucent volumes (draw after opaque, depth write off).
  - Ground: large plane with checker pattern + thin grid lines in the fragment shader, fading
    into fog toward the horizon.
  - Background: vertical sky gradient (dark theme like the MuJoCo viewer; light theme follows
    prefers-color-scheme).
  - Shadows: planar projected shadows on the ground (flatten each opaque mesh onto y=0 along the
    light direction, dark translucent, stencil or polygon offset to avoid z-fighting).
  - Edges: optional line rendering for frustum edges and the selection highlight.
  - Antialias on, devicePixelRatio-aware, resize via ResizeObserver.
  - Render only when something changed or the camera is still moving (inertia), via
    requestAnimationFrame.
- If WebGL2 is unavailable, show a short message and keep the old 2D view as a fallback
  (move the old code to `view2d.rs`).

## Scene
- Robot body (box on a mast), camera module (box + lens cylinder), and the stereo pair when
  stereo is enabled (baseline to scale).
- Translucent view frustum with bright edges, cut at the ground. The visible ground footprint is
  highlighted; the near blind zone is tinted red.
- A target object (simple capsule "person" or box, size from the target-size input) standing on
  the ground at the target distance. Show its pixel height on a label anchored to it.
- When stereo is on: concentric rings on the ground colored by depth error Δz (green → yellow →
  red), with a small legend.

## Interaction (game-like, mouse + touch)
- Left drag: orbit. Right drag or two-finger drag: pan. Wheel or pinch: zoom. Double click /
  double tap: reset view. Smooth damping (inertia) on all camera motion.
- Direct manipulation:
  - Drag the target object along the ground (ray-plane intersection). This updates the
    distance input and all numbers live.
  - Drag a vertical handle on the camera mast to change mount height; a ring handle around the
    camera changes tilt. Both update the form inputs live.
  - Hover highlight + cursor change on draggable things.
- Toggle buttons over the view: frustum, footprint, Δz rings, shadows. Preset camera views:
  Perspective / Top / Side / From camera (first-person view through the robot camera, matching
  its FOV — this is the "what the robot sees" view).
- HUD overlay (HTML positioned over the canvas, not drawn in GL): key numbers for the current
  state (distance, pixels on target, Δz, footprint length).

## Code quality and limits
- Put pure math (mat4/vec3, orbit camera, ray-plane intersection, picking, frustum corners) in a
  host-testable module and add unit tests for it (cargo test must run them). Keep the existing
  cf_core tests green.
- Keep the CSP in index.html working (no inline scripts; shader source as Rust strings is fine).
- Release wasm should stay under about 350 KB. No new crates besides web-sys features / js-sys.
- No personal data, no analytics, no network requests.
- Update README (KO + EN) with the new controls.

## Done when
- `cargo test`, `trunk build --release` and `bash tools/privacy_check.sh` pass with no warnings.
- Open a PR against main titled "3D view: WebGL2 game-grade scene with direct manipulation".
