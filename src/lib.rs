// emotion-cube — PS2 Emotion Engine WASM proof-of-concept.
// PS2-faithful pipeline: EE → DMAC → VIF1 → VU1 → GIF → SW-GS → wgpu blit

mod dmac;
mod ee;
mod gif;
mod gs_display;
mod gs_rasterizer;
mod vif1;
mod vu1;
mod vu1_program;

use wasm_bindgen::prelude::*;

/// Top-level emulator core, exposed to JavaScript.
///
/// Lifecycle (JS):
///   const core = await EmulatorCore.create("canvas-id");
///   function loop() { const t = core.step_frame(); requestAnimationFrame(loop); }
///   loop();
#[wasm_bindgen]
pub struct EmulatorCore {
    ee:          ee::EmotionEngine,
    dmac:        dmac::Dmac,
    vif1:        vif1::Vif1,
    vu1:         vu1::Vu1,
    gs_fb:       gs_rasterizer::Framebuffer,
    gs_display:  gs_display::GsDisplay,
    frame_count: u64,
    emu_cycles:  u64,
    vu1_mat_ops: u64,
}

#[wasm_bindgen]
impl EmulatorCore {
    /// Async factory — `await EmulatorCore.create("canvas-id")` from JS.
    pub async fn create(canvas_id: &str) -> Result<EmulatorCore, JsValue> {
        console_error_panic_hook::set_once();

        let gs_display = gs_display::GsDisplay::new(canvas_id)
            .await
            .map_err(|e| JsValue::from_str(&e))?;

        Ok(EmulatorCore {
            ee:          ee::EmotionEngine::new(),
            dmac:        dmac::Dmac::new(),
            vif1:        vif1::Vif1::new(),
            vu1:         vu1::Vu1::new(),
            gs_fb:       gs_rasterizer::Framebuffer::new(),
            gs_display,
            frame_count: 0,
            emu_cycles:  0,
            vu1_mat_ops: 0,
        })
    }

    /// Simulate one frame through the full PS2 pipeline.
    ///
    /// 1. EE builds VIF1 DMA packet in EE RAM, kicks DMAC
    /// 2. DMAC transfers QWs from EE RAM → VIF1 FIFO
    /// 3. VIF1 parser: STCYCL, UNPACK, MSCAL, FLUSH → VU data memory
    /// 4. VU1 micro-program: MVP transform, lighting, viewport → GIF buffer
    /// 5. GIF tag parser → triangle primitives
    /// 6. Software GS rasterizer → Framebuffer
    /// 7. wgpu texture blit → canvas
    ///
    /// Returns telemetry: { emulatedCycles, vu1MatOps, frameCount }
    pub fn step_frame(&mut self) -> JsValue {
        // 1. EE: build VIF1 packet, kick DMAC (300 MHz / 60 fps ≈ 5M cycles/frame)
        let (madr, qwc) = self.ee.build_packet();
        self.emu_cycles += 300_000;
        self.dmac.kick(madr, qwc);

        // 2. DMAC: transfer EE RAM → VIF1 FIFO
        self.dmac.transfer(&*self.ee.ee_ram, &mut self.vif1.fifo);

        // 3. VIF1: parse packet → VU1 data memory
        self.vif1.process(&mut self.vu1.data_mem);

        // 4. VU1: run micro-program until XGKICK
        self.vu1.pc = self.vif1.mscal_addr.take().unwrap_or(0);
        let xgkick_base = self.vu1.run_until_xgkick();
        self.vu1_mat_ops += 5; // 3 mat-mul + 2 rotation = 5 per frame

        // 5. GIF: parse tag + vertex data from VU data memory
        // xgkick_base = VI[05] = 108 (GIF tag QW address in VU data memory)
        let prims = gif::parse_gif_packet(&self.vu1.data_mem, xgkick_base as usize);

        // 6. GS rasterizer: clear then draw triangles
        self.gs_fb.clear(0xFF_08_0A_14); // PS2-ish dark bg
        for prim in &prims {
            for tri in prim.vertices.chunks(3) {
                if tri.len() == 3 {
                    gs_rasterizer::rasterize_triangle(
                        &mut self.gs_fb,
                        &tri[0], &tri[1], &tri[2],
                    );
                }
            }
        }

        // 7. Upload framebuffer texture and blit to canvas
        self.gs_display.upload_and_present(&self.gs_fb);

        self.frame_count += 1;

        // Telemetry
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &obj, &JsValue::from_str("emulatedCycles"),
            &JsValue::from_f64(self.emu_cycles as f64),
        );
        let _ = js_sys::Reflect::set(
            &obj, &JsValue::from_str("vu1MatOps"),
            &JsValue::from_f64(self.vu1_mat_ops as f64),
        );
        let _ = js_sys::Reflect::set(
            &obj, &JsValue::from_str("frameCount"),
            &JsValue::from_f64(self.frame_count as f64),
        );
        obj.into()
    }
}
