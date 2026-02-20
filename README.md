# emotion-cube

A browser-runnable **PS2 Emotion Engine proof-of-concept** that renders a Gouraud-shaded spinning cube. The Rust/WASM core stubs out the EE CPU, VU1 vector unit, and the Graphics Synthesizer. A Next.js frontend hosts the canvas and drives a `requestAnimationFrame` loop with a telemetry HUD.

![emotion-cube demo](docs/demo.png)

---

## What this is

The PlayStation 2's rendering pipeline is famous for its unusual multi-chip architecture: the **Emotion Engine** (MIPS R5900) orchestrates geometry through the **Vector Units** (VU0/VU1), which micro-program the **Graphics Synthesizer** via DMA. This project models that logical pipeline in code you can actually run in a browser tab.

```
EE (MIPS R5900)          VU1 micro-program              GS (rasterizer)
┌──────────────┐   VIF1  ┌────────────────────┐  XGKICK ┌─────────────┐
│ Game logic   │──DMA───▶│ Matrix transforms  │────────▶│ Rasterise   │
│ (stub)       │         │ Gouraud lighting   │         │ triangles   │
└──────────────┘         └────────────────────┘         └─────────────┘
   cpu.rs                      vu.rs                        gs.rs
```

Each component maps to a Rust module:

| PS2 Hardware | Code | Notes |
|---|---|---|
| Emotion Engine (MIPS R5900) | `src/cpu.rs` | Cycle counter stub — 300k cycles/frame |
| VU1 micro-program | `src/vu.rs` | Full transform + Gouraud lighting math |
| VIF1 DMA (EE → VU1 data memory) | `CUBE_VERTS` const | Pre-loaded vertex data |
| GIF PATH1 (XGKICK → GS) | `gs.render(&display_list)` | Direct call, no GIF tag parsing |
| Graphics Synthesizer (rasterizer) | `src/gs.rs` | wgpu WebGL2 + passthrough WGSL shader |
| GS eDRAM framebuffer (640×448) | wgpu `Surface` | WebGL2 renderbuffer |
| GS BGCOLOR register | `LoadOp::Clear(0.03, 0.03, 0.08)` | Dark blue-black clear color |
| NTSC VBLANK interrupt | `requestAnimationFrame` | Browser RAF loop |

---

## Architecture

```
emotion-cube/
├── .cargo/
│   └── config.toml           # RUSTFLAGS: web_sys_unstable_apis + bulk-memory
├── Cargo.toml                # wgpu 28, wasm-bindgen, bytemuck
├── src/
│   ├── lib.rs                # EmulatorCore — wasm-bindgen public interface
│   ├── cpu.rs                # EmotionEngine stub (MIPS cycle counter)
│   ├── vu.rs                 # VU1: Mat4, Vec4, CUBE_VERTS, execute_micro_program()
│   └── gs.rs                 # GraphicsSynthesizer: wgpu init, WGSL shader, render()
└── frontend/
    ├── package.json          # Next.js 15 + React 19
    ├── next.config.ts        # asyncWebAssembly webpack experiment
    └── app/
        ├── layout.tsx
        ├── page.tsx
        └── EmulatorCanvas.tsx  # RAF loop, wasm init, telemetry HUD
```

### VU1 pipeline (`src/vu.rs`)

`execute_micro_program()` runs every frame and mirrors what a real VU1 micro-program does:

1. Build **model matrix**: `rotate_x(frame/360°) × rotate_y(frame/180°)`
2. Build **view matrix**: `translate(0, 0, −3)` (cube 3 units in front of camera)
3. Build **projection matrix**: `perspective(60°, 640/448, 0.1, 100)` with **[0,1] depth range** (wgpu/WebGPU NDC — not OpenGL's [−1,1])
4. Concatenate: `mvp = proj × view × model` (5 matrix ops/frame)
5. Per-vertex: transform to clip space, transform normal by model only, compute `diffuse + 0.2 ambient`, output `GsVertex`

### Graphics Synthesizer (`src/gs.rs`)

The PS2 GS is fixed-function — no shaders. Our WGSL shader is a **pure passthrough** (position and color straight through), which replicates fixed-function behavior for untextured Gouraud primitives:

```wgsl
@vertex fn vs_main(in: VertexInput) -> VertexOutput {
    return VertexOutput(in.position, in.color);  // VU1 already did everything
}
@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
```

### Telemetry HUD

The `step_frame()` call returns a JS object with:
- **Emulated Cycles/Frame**: always 300,000 (stub)
- **VU1 Matrix Ops**: cumulative count (5 per frame)
- **Host FPS**: measured via `performance.now()` in the RAF loop

---

## How close is this to real PS2 hardware?

| Dimension | Accuracy |
|---|---|
| Visual output (this demo) | ~95% — same math, same pixels |
| VU1 transform + lighting math | High — equivalent float ops |
| PS2 software ABI / MIPS execution | 0% — no instruction emulation |
| Cycle accuracy | 0% — no timing model |
| Pipeline concurrency (EE ‖ VU1 ‖ GS) | 0% — fully sequential |
| GIF tag parsing / VIF packets | 0% — stubbed |
| IOP, SPU2, DMA controller | 0% — not implemented |

The logical pipeline (EE → VU1 → GS) is correctly modeled. The math is faithful. None of the actual hardware mechanics are cycle-accurate. Think of it as a **PS2-inspired rendering demo** rather than an emulator.

---

## Prerequisites

- Rust 1.87+ (`rustup target add wasm32-unknown-unknown`)
- Node.js 18+
- wasm-pack (`cargo install wasm-pack`)

---

## Build & run

```bash
# 1. Build WASM (from repo root)
wasm-pack build --target web --out-dir frontend/public/pkg --release --no-opt

# 2. Install frontend deps & start dev server
cd frontend
npm install
npm run dev
```

Open **http://localhost:3000** (or whichever port Next.js picks).

### Production build

```bash
cd frontend && npm run build && npm start
```

> **Note:** `npm run build` uses `--no-turbopack` (the `build` script in `package.json`) because Turbopack does not yet support the `asyncWebAssembly` webpack experiment required to load the WASM module.

---

## Key implementation notes

### `--no-opt` flag
wasm-pack's bundled `wasm-opt` rejects `memory.copy` instructions unless the WASM module explicitly declares the bulk-memory feature in its feature section. Rust 1.87 emits these instructions unconditionally for large struct copies. The `--no-opt` flag skips `wasm-opt` entirely; the WASM is slightly larger (~2–3×) but functionally identical.

### `webpackIgnore` import
The WASM glue generated by wasm-pack uses `new URL('emotion_cube_bg.wasm', import.meta.url)` to locate the binary. If webpack bundles the JS glue, `import.meta.url` points to webpack's internal module ID rather than the public URL, breaking the fetch. The `/* webpackIgnore: true */` comment tells webpack to leave the import alone so the browser resolves it natively against `/pkg/emotion_cube.js`.

### `fragile-send-sync-non-atomic-wasm` feature
wgpu's `Device` and `Queue` types are not `Send` in single-threaded WASM. Without this feature flag, the Rust compiler rejects storing them inside a `#[wasm_bindgen]` struct. The flag opts in to wgpu's "I know this is single-threaded WASM" unsafe impl.

### `[0,1]` depth range
wgpu (and WebGPU) use NDC depth range `[0, 1]` (DirectX convention), not OpenGL's `[−1, 1]`. The perspective matrix in `vu.rs` uses `far * range_inv` where `range_inv = 1 / (near − far)`, matching wgpu's expectation.

---

## License

MIT
