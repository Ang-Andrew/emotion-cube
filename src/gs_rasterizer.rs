// gs_rasterizer.rs — Software GS triangle rasterizer.
// Maps to: PS2 GS fixed-function rasterizer writing to 4 MB eDRAM.
// Implements Pineda edge-function rasterization with Gouraud interpolation.

pub const FB_W: usize = 640;
pub const FB_H: usize = 448;

/// Software framebuffer — 640×448 RGBA pixels stored as 0xAA_BB_GG_RR (ABGR little-endian).
pub struct Framebuffer {
    pub pixels: Vec<u32>,
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer {
            pixels: vec![0xFF_08_0A_14; FB_W * FB_H],
        }
    }

    /// Clear to a given ABGR color (e.g. 0xFF_14_0A_08 = dark blue-ish PS2 bg).
    pub fn clear(&mut self, color: u32) {
        self.pixels.fill(color);
    }
}

/// A single GIF-decoded vertex ready for the rasterizer.
pub use crate::gif::GifVertex;

/// Rasterize one triangle using the Pineda edge-function algorithm with Gouraud shading.
/// GifVertex x/y are already in pixel coordinates (decoded from GS 12.4 fixed-point).
pub fn rasterize_triangle(fb: &mut Framebuffer, v0: &GifVertex, v1: &GifVertex, v2: &GifVertex) {
    // Bounding box clamped to framebuffer extent
    let min_x = v0.x.min(v1.x).min(v2.x).max(0) as usize;
    let min_y = v0.y.min(v1.y).min(v2.y).max(0) as usize;
    let max_x = (v0.x.max(v1.x).max(v2.x) as usize).min(FB_W - 1);
    let max_y = (v0.y.max(v1.y).max(v2.y) as usize).min(FB_H - 1);

    // Edge function: e(a,b,p) = (bx-ax)*(py-ay) - (by-ay)*(px-ax)
    // Positive means p is to the left of a→b (CCW convention).
    let edge = |ax: i32, ay: i32, bx: i32, by: i32, px: i32, py: i32| -> i32 {
        (bx - ax) * (py - ay) - (by - ay) * (px - ax)
    };

    // Signed area × 2 — used to normalise barycentric weights.
    let area2 = edge(v0.x, v0.y, v1.x, v1.y, v2.x, v2.y);

    // The viewport Y-flip (screen.y = (1-ndc.y)*H) reverses winding from CCW-NDC to CW-screen.
    // Front-facing triangles (CCW in 3D/NDC) therefore have area2 < 0 in screen space.
    // Cull degenerate (area2==0) and back-facing (area2>0, i.e. CW-NDC) triangles.
    if area2 >= 0 {
        return;
    }

    // area2 is negative; use its absolute value for normalisation.
    let area2f = (-area2) as f32;

    for py in min_y..=max_y {
        for px in min_x..=max_x {
            let px_i = px as i32;
            let py_i = py as i32;

            // Barycentric weights — negative for CW (front-facing) triangles in screen space.
            let w0 = edge(v1.x, v1.y, v2.x, v2.y, px_i, py_i);
            let w1 = edge(v2.x, v2.y, v0.x, v0.y, px_i, py_i);
            let w2 = edge(v0.x, v0.y, v1.x, v1.y, px_i, py_i);

            // Inside test: all weights ≤ 0 (they are negative for interior points of CW triangles).
            if w0 <= 0 && w1 <= 0 && w2 <= 0 {
                // Normalised barycentric coordinates (negative/negative = positive).
                let b0 = (-w0) as f32 / area2f;
                let b1 = (-w1) as f32 / area2f;
                let b2 = (-w2) as f32 / area2f;

                // Gouraud-interpolate RGBA
                let r = (b0 * v0.r as f32 + b1 * v1.r as f32 + b2 * v2.r as f32) as u32;
                let g = (b0 * v0.g as f32 + b1 * v1.g as f32 + b2 * v2.g as f32) as u32;
                let b = (b0 * v0.b as f32 + b1 * v1.b as f32 + b2 * v2.b as f32) as u32;

                // Pack as 0xFF_BB_GG_RR
                let pixel = 0xFF00_0000 | (b << 16) | (g << 8) | r;
                fb.pixels[py * FB_W + px] = pixel;
            }
        }
    }
}
