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
- **3D 보기**: 바닥 격자, 카메라, 시야 절두체, 대상 평면과 물체, 바닥 영역. 드래그로 회전, 슬라이더로 거리 변경
- 프리셋 두 개("일반값", 자유롭게 수정 가능), 한국어/영어 전환, 다크 모드는 시스템 설정을 따름

### 모델과 가정
- 이상적인 핀홀 카메라: 렌즈 왜곡 없음, 피사계 심도 무시
- 주점(cx, cy)은 이미지 중앙 (W/2, H/2)
- 바닥은 평평하며, 거리는 카메라 바로 아래 지점에서 잰 수평 거리
- 픽셀 피치만 입력하면 정사각형 픽셀로 가정
- px/mm 및 물체 픽셀 수는 광축에 수직인 대상에 대해 가로 방향(fx) 기준

### 사용법
프리셋을 고르거나 값을 입력하세요. 모든 결과는 입력 즉시 갱신됩니다. 3D 보기는 드래그로 회전합니다.

### 로컬 빌드
```sh
rustup target add wasm32-unknown-unknown
cargo test -p cf_core
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
- **3D view**: floor grid, camera, frustum, target plane with object, floor footprint. Drag to orbit, slider
  to move the target distance
- Two editable "typical" presets, Korean/English toggle, dark mode follows the system setting

### Model and assumptions
- Ideal pinhole camera: no lens distortion, no depth of field
- Principal point (cx, cy) at the image centre (W/2, H/2)
- Flat floor; floor distances are horizontal, measured from the point directly below the camera
- Square pixels when only the pixel pitch is given
- px/mm and object pixels use fx (horizontal) for a target facing the camera

### How to use
Pick a preset or type values; every output updates live. Drag the 3D view to orbit.

### Local build
```sh
rustup target add wasm32-unknown-unknown
cargo test -p cf_core
trunk build --release      # output in dist/
sh tools/privacy_check.sh  # must print "clean"
```
`trunk serve` runs it locally (open the `/cam-fov/` path, since that is the `public_url`).

### Layout
- `crates/cf_core` — pure Rust geometry (FOV, footprint, floor intersection, stereo error), unit tested
- `crates/cf_web` — UI with plain `web-sys`; `view3d.rs` is the small canvas 2D perspective renderer
- `index.html`, `web/boot.js`, `Trunk.toml` — static page (strict CSP, no inline script)

### License
MIT — see `LICENSE`.
