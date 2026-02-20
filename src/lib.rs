// emotion-cube — PS2 Emotion Engine WASM proof-of-concept.
// This module is the wasm-bindgen interface exposed to JavaScript.

mod cpu;
mod gs;
mod vu;

use wasm_bindgen::prelude::*;

/// Top-level emulator core, exposed to JavaScript.
///
/// Lifecycle (JS):
///   const core = await EmulatorCore.create("canvas-id");
///   function loop() { const t = core.step_frame(); requestAnimationFrame(loop); }
///   loop();
#[wasm_bindgen]
pub struct EmulatorCore {
    gs:          gs::GraphicsSynthesizer,
    cpu:         cpu::EmotionEngine,
    vu1:         vu::Vu1State,
    emu_cycles:  u64,
    vu1_mat_ops: u64,
    frame_count: u64,
}

#[wasm_bindgen]
impl EmulatorCore {
    /// Async factory — call as `await EmulatorCore.create("canvas-id")` from JS.
    /// (wasm-bindgen deprecated async constructors; static factory methods are the
    ///  recommended replacement.)
    pub async fn create(canvas_id: &str) -> Result<EmulatorCore, JsValue> {
        // Install a panic hook so Rust panics appear in the browser console.
        console_error_panic_hook::set_once();

        let gs = gs::GraphicsSynthesizer::new(canvas_id)
            .await
            .map_err(|e| JsValue::from_str(&e))?;

        Ok(EmulatorCore {
            gs,
            cpu: cpu::EmotionEngine::new(),
            vu1: vu::Vu1State::new(),
            emu_cycles: 0,
            vu1_mat_ops: 0,
            frame_count: 0,
        })
    }

    /// Simulate one frame: EE runs, VU1 transforms vertices, GS rasterises.
    /// Maps to: VBLANK interrupt triggering a full EE → VU1 → GS pipeline.
    ///
    /// Returns a plain JS object:
    ///   { emulatedCycles: number, vu1MatOps: number, frameCount: number }
    pub fn step_frame(&mut self) -> JsValue {
        // EE: advance MIPS cycle counter
        let cycles = self.cpu.step();
        self.emu_cycles += cycles;

        // VU1: run micro-program, get display list
        let (display_list, mat_ops) = vu::execute_micro_program(&mut self.vu1);
        self.vu1_mat_ops += mat_ops;

        // GS: rasterise the display list
        self.gs.render(&display_list);

        self.frame_count += 1;

        // Build telemetry object for the JS HUD
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("emulatedCycles"),
            &JsValue::from_f64(self.emu_cycles as f64),
        );
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("vu1MatOps"),
            &JsValue::from_f64(self.vu1_mat_ops as f64),
        );
        let _ = js_sys::Reflect::set(
            &obj,
            &JsValue::from_str("frameCount"),
            &JsValue::from_f64(self.frame_count as f64),
        );

        obj.into()
    }
}
