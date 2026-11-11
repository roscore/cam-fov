# cam-fov

로봇 카메라 FOV · 스테레오 깊이 플래너 / Robot camera FOV & stereo depth planner

> **추정치일 뿐입니다 — 실제 값은 캘리브레이션으로 확인하세요.**
> **Estimates only — verify with a calibration.**

## 한국어

### 무엇인가요?
"로봇에 카메라를 달면 무엇이 보이고, 물체에 픽셀이 몇 개나 맺히며, (스테레오라면) 깊이는 얼마나 정확할까?"에
답하는 단일 페이지 웹 앱입니다. Rust → WebAssembly로 빌드되며 브라우저 안에서만 동작합니다
(서버, 분석 도구, 외부 요청 없음).

- **카메라**: 센서 크기(mm) 또는 픽셀 피치(µm) × 해상도, 초점거리 → 가로/세로/대각 FOV, f_px, 내부 행렬 K
- **장착 + 대상**: 높이, 아래쪽 틸트, 광축 거리, 물체 크기 → 촬영 범위, px/mm, 물체 픽셀 수(최소값 대비 ✅/⚠️),
  보이는 바닥 범위(가까운/먼 거리, 수평선 위면 ∞), 전방 사각지대
- **스테레오**: 베이스라인 B, 시차 정확도 Δd, 최대 시차 d_max → Δz = z²·Δd/(f_px·B), z_min = f_px·B/d_max, Δz 그래프
- **3D 보기** (WebGL2): 조명·그림자가 있는 장면 — 기둥 위 로봇과 카메라(스테레오 쌍), 지면에서 잘린 반투명
  시야 절두체, 바닥 영역(초록)과 전방 사각지대(빨강), 대상 거리에 선 대상 물체와 픽셀 수 라벨, 스테레오일 때
  Δz 링(초록 → 노랑 → 빨강). WebGL2가 없으면 간단한 2D 보기로 대체
- 프리셋 두 개("일반값", 자유롭게 수정 가능), 한국어/영어 전환, 다크 모드는 시스템 설정을 따름

### 모델과 가정
- 이상적인 핀홀 카메라: 렌즈 왜곡 없음, 피사계 심도 무시
- 주점(cx, cy)은 이미지 중앙 (W/2, H/2)
- 바닥은 평평하며, 거리는 카메라 바로 아래 지점에서 잰 수평 거리
- 픽셀 피치만 입력하면 정사각형 픽셀로 가정
- px/mm 및 물체 픽셀 수는 광축에 수직인 대상에 대해 가로 방향(fx) 기준

### 사용법
프리셋을 고르거나 값을 입력하세요. 모든 결과는 입력 즉시 갱신됩니다.

3D 보기 조작:
- 왼쪽 드래그: 회전 · 오른쪽 드래그 또는 두 손가락 드래그: 이동 · 휠 또는 핀치: 확대/축소 · 더블 클릭/더블 탭: 초기화
- 주황색 대상을 바닥 위로 끌면 대상 거리가, 기둥의 주황색 손잡이를 위아래로 끌면 카메라 높이가, 카메라를 두른
  파란 링을 돌리면 틸트가 바뀌며 입력란과 모든 결과가 함께 갱신됩니다
- 상단 버튼: 원근 / 위 / 옆 / 카메라 시점(로봇 카메라의 FOV 그대로 "로봇이 보는 장면"), 절두체 · 바닥 영역 ·
  Δz 링 · 그림자 켜기/끄기
- 왼쪽 아래 HUD: 거리, 대상 픽셀 수, Δz, 바닥 영역 길이

### 로컬 빌드
```sh
rustup target add wasm32-unknown-unknown
cargo test                 # cf_core + cf_web 수학 모듈
trunk build --release      # 결과물: dist/
sh tools/privacy_check.sh  # "clean" 출력 확인
```
`trunk serve`로 로컬에서 실행할 수 있습니다 (`public_url`이 `/cam-fov/`이므로 해당 경로로 접속).

### 라이선스
MIT — `LICENSE` 참고.

## English

### What is it?
A single-page web app answering "I am mounting a camera on a robot — what does it see, how many pixels land
on my object, and (for stereo) how accurate is depth?" Built in Rust → WebAssembly, runs entirely in the
browser (no server, no analytics, no external requests).

- **Camera**: sensor size (mm) or pixel pitch (µm) × resolution, focal length → horizontal / vertical /
  diagonal FOV, f_px, intrinsics K
- **Mount + target**: height, downward tilt, distance along the optical axis, object size → footprint,
  px/mm, object size in pixels (✅/⚠️ against a minimum), visible floor range (near / far, ∞ above the
  horizon), blind zone in front of the robot
- **Stereo**: baseline B, disparity accuracy Δd, max disparity d_max → Δz = z²·Δd/(f_px·B),
  z_min = f_px·B/d_max and a Δz-vs-distance plot
- **3D view** (WebGL2): lit scene with shadows — robot with the camera (or stereo pair) on a mast,
  translucent frustum cut at the ground, floor footprint (green) and blind zone (red), the target standing at
  the target distance with a pixel-count label, and for stereo Δz rings (green → yellow → red). Falls back to
  a simple 2D view without WebGL2
- Two editable "typical" presets, Korean/English toggle, dark mode follows the system setting

### Model and assumptions
- Ideal pinhole camera: no lens distortion, no depth of field
- Principal point (cx, cy) at the image centre (W/2, H/2)
- Flat floor; floor distances are horizontal, measured from the point directly below the camera
- Square pixels when only the pixel pitch is given
- px/mm and object pixels use fx (horizontal) for a target facing the camera

### How to use
Pick a preset or type values; every output updates live.

3D view controls:
- Left drag: orbit · right drag or two-finger drag: pan · wheel or pinch: zoom · double click / double tap: reset
- Drag the orange target along the floor to change the target distance, the orange knob on the mast up/down
  for camera height, and the blue ring around the camera for tilt — the inputs and all results follow live
- Buttons over the view: Perspective / Top / Side / From camera (first-person through the robot camera with
  its own FOV — "what the robot sees"), and toggles for frustum, footprint, Δz rings and shadows
- HUD (bottom left): distance, pixels on target, Δz, footprint length

### Local build
```sh
rustup target add wasm32-unknown-unknown
cargo test                 # cf_core + cf_web math module
trunk build --release      # output in dist/
sh tools/privacy_check.sh  # must print "clean"
```
`trunk serve` runs it locally (open the `/cam-fov/` path, since that is the `public_url`).

### Layout
- `crates/cf_core` — pure Rust geometry (FOV, footprint, floor intersection, stereo error), unit tested
- `crates/cf_web` — UI with plain `web-sys`: `gl.rs` small reusable WebGL2 renderer, `math.rs` pure
  vector / matrix / orbit / picking math (unit tested), `view3d.rs` the scene and interaction, `view2d.rs`
  the canvas 2D fallback
- `index.html`, `web/boot.js`, `Trunk.toml` — static page (strict CSP, no inline script)

### License
MIT — see `LICENSE`.
