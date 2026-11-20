# MIZU — Development Plan

> **How to use this plan in small chats:**
> Each sub-task (`1A`, `2A`, etc.) is scoped to **1 small chat session**. When starting a new chat, reference a single task (e.g., *"Implement 1A from plan.md"*). Update the checkboxes as items are completed.

---

## Current Architecture Snapshot

- **Engine:** Bevy `0.18` (2D, `FixedUpdate` at `120 Hz`)
- **Package:** `mizu` (`Cargo.toml`)
- **Core Files:**
  - `src/main.rs` — App entry point, plugin/system wiring, and window configuration.
  - `src/sim.rs` — Simulation state (`SimState`), spatial hash grid (`SpatialGrid`), particle physics & collisions, and unit spawning.
  - `src/water.rs` — Offscreen metaball density pass (`RenderLayers::layer(1)` -> `1280x720` render target), `WaterMaterial` / `WaterParams`, and procedural textures.
  - `src/ui.rs` — On-screen HUD, top-right control bar, and input handling.
  - `assets/shaders/water.wgsl` — 9-tap smoothing kernel, density thresholding, depth color ramp (`sparse` / `mid` / `dense`), surface-tension rim/foam, and organic shimmer.
- **Current Controls:**
  - `Left/Right-Drag`: Repel / Attract particles
  - `[` / `]` / `Scroll` / UI `-` `+`: Adjust cursor force (`300`–`4500`)
  - `G` / UI button: Toggle gravity
  - `W`: Toggle Water Shader / Particles view
  - `+`/`Up` & `-`/`Down`: Add/remove `500` units
  - `Space`: Pause / Resume
  - `R`: Reset positions

---

## Phase 0 — Baseline & Rename (Done)

- [x] Rename crate in `Cargo.toml` to `mizu`
- [x] Set primary window title to `"MIZU"`
- [x] Update HUD title to `"MIZU"`

---

## Phase 1 — Bugfix & Lightweight Module Split (Done)

- [x] **1A: Fix broken mouse-scroll force scaling**
  - **Files:** `src/ui.rs`
  - **Scope:** In `handle_input`, `AccumulatedMouseScroll` can report pixel-based deltas (`MouseScrollUnit::Pixel`, e.g., `15`–`120` per notch on Wayland/trackpads) rather than `1.0` per notch, causing `scroll.delta.y * 150.0` to immediately clamp to `300` or `4500`. Handle `MouseScrollUnit::Line` vs `MouseScrollUnit::Pixel` with unit-aware scaling and per-frame clamping.
  - **Acceptance:** Scrolling up/down smoothly steps `Force` instead of jumping straight to min/max.
- [x] **1B: Extract simulation & grid (`src/sim.rs`)**
  - **Files:** `src/sim.rs`, `src/main.rs`
  - **Scope:** Move `SimState`, `SpatialGrid`, `Unit`, `spawn_units`, and `step_simulation` into `src/sim.rs`.
  - **Acceptance:** `cargo check` and `cargo run` behave identically.
- [x] **1C: Extract water material & rendering (`src/water.rs`)**
  - **Files:** `src/water.rs`, `src/main.rs`
  - **Scope:** Move `WaterParams`, `WaterMaterial`, `WaterQuad`, `OffscreenCamera`, `create_circle_image`, `create_blob_image`, and `update_water_time` into `src/water.rs`.
  - **Acceptance:** `cargo check` and water/particle toggle (`W`) works.
- [x] **1D: Extract UI & input (`src/ui.rs`)**
  - **Files:** `src/ui.rs`, `src/main.rs`
  - **Scope:** Move HUD, top-right control bar, button marker components, `handle_input`, and `update_hud` into `src/ui.rs`.
  - **Acceptance:** `cargo check` and all UI buttons/keybinds work.

---

## Phase 2 — Water Shader Colors & Control Panel (Done)

- [x] **2A: Water color presets**
  - **Files:** `assets/shaders/water.wgsl`, `src/water.rs`, `src/ui.rs`
  - **Scope:** Move `sparse_color`, `mid_color`, `dense_color`, and `rim_color` into `WaterParams` uniforms. Add a `WaterPalette` enum with curated presets (*Ocean*, *Lava*, *Slime*, *Ink*, *Sunset*) and UI cycle controls.
  - **Acceptance:** Cycling presets immediately updates the water shader's depth gradient and rim highlight.
- [x] **2B: Collapsible shader & color control panel**
  - **Files:** `src/ui.rs`, `src/water.rs`, `assets/shaders/water.wgsl`
  - **Scope:** Add a toggleable control panel (`Tab` or `Shader (Tab)` button) with `-`/`+` stepper buttons for `threshold`, `foam_width`, `shimmer`, palette selection, and `Reset Defaults`, plus cleaned-up translucent glass HUD/toolbar cards.
  - **Acceptance:** Opening the panel and adjusting controls updates `WaterParams` live without blocking simulation input.

---

## Phase 3 — Particle Visuals (Gradients & Size) (Done)

- [x] **3A: Nice color gradients in Particle mode**
  - **Files:** `src/sim.rs` (or `src/main.rs`), `src/ui.rs`
  - **Scope:** Replace the random per-unit `base_hue` rainbow with cohesive color gradients when `water_mode == false` — e.g., selectable gradient modes (*Speed*, *Depth/Height*, *Density*, *Sunset/Ocean/Neon* ramps) using smooth `Vec3`/color interpolation.
  - **Acceptance:** Toggling `W` to Particles shows smooth, cohesive color gradients that respond to particle motion or position.
- [x] **3B: Dynamic particle size**
  - **Files:** `src/sim.rs` (or `src/main.rs`), `src/ui.rs`
  - **Scope:** Move `UNIT_RADIUS` into `SimState` (e.g., `2.5`–`10.0`). When changed via UI `-`/`+` (or hotkeys), update each `Unit` sprite's `custom_size` (both circle and water blob scale), collision `min_dist = unit_radius * 2.0`, and ensure `SpatialGrid` cell size / neighbor search covers `min_dist`.
  - **Acceptance:** Increasing/decreasing particle size visibly resizes particles and updates collision spacing and water metaball thickness cleanly.

---

## Phase 4 — Gravity Direction & Strength

- [ ] **4A: Configurable gravity vector (direction + strength)**
  - **Files:** `src/sim.rs` (or `src/main.rs`), `src/ui.rs`
  - **Scope:** Replace `gravity_on: bool` + hardcoded `-520.0` with `gravity_enabled: bool`, `gravity_strength: f32`, and `gravity_angle: f32` (or `Vec2` direction). Add UI controls for strength (`-`/`+`) and direction (e.g., cardinal presets `↓ ↑ ← →` or rotating angle step, plus optional radial/center gravity). Update wall friction/bounce logic to work with any gravity direction instead of assuming `pos.y < min_y` is always the floor.
  - **Acceptance:** Gravity can be pointed in any direction and scaled in strength, with particles settling naturally against whichever wall is "down".

---

## Phase 5 — Container Shapes

- [ ] **5A: Container shape system (Circle, Rounded Rect, Triangle, Hexagon)**
  - **Files:** `src/sim.rs` (or `src/main.rs`), `src/ui.rs`
  - **Scope:** Add a `ContainerShape` enum (`Rectangle`, `Circle`, `Hexagon`, `Triangle`) in `SimState` with a UI cycle button. Replace the AABB wall clamping in `step_simulation` with shape-specific boundary distance + normal reflection (e.g., radial clamp for `Circle`, half-plane / polygon edge SDF for `Hexagon` and `Triangle`). Update the background arena border visual (using `Mesh2d` or gizmos) to match the active container shape.
  - **Acceptance:** Cycling container shapes updates both the rendered arena boundary and particle/water collisions smoothly.

---

## Phase Template *(Copy for new ideas)*

```markdown
## Phase X — <Feature Name>

- [ ] **XA: <Small Chat Task>**
  - **Files:** `<file1>`, `<file2>`
  - **Scope:** <1-3 sentences describing exact changes>
  - **Acceptance:** <How to verify it works in-game>
```
