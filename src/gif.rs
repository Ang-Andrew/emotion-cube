// gif.rs — GIF tag parser.
// Maps to: PS2 GIF (Graphics Interface) parsing PACKED-mode GIF tags from VU1 output.

/// A single GIF-decoded vertex ready for the software rasterizer.
#[derive(Clone, Debug)]
pub struct GifVertex {
    pub r: u8, pub g: u8, pub b: u8, pub a: u8,
    /// Pixel-space X (decoded from GS 12.4 fixed-point via >> 4)
    pub x: i32,
    /// Pixel-space Y (decoded from GS 12.4 fixed-point via >> 4)
    pub y: i32,
}

/// A GS primitive (triangle strip/list) with Gouraud flag.
pub struct GsPrimitive {
    pub iip:      bool,
    pub vertices: Vec<GifVertex>,
}

/// Parse a GIF packet starting at `vu_mem[base_qw]`.
///
/// Layout expected:
///   vu_mem[base_qw]      — 128-bit GIF tag (low u64 / high u64 in two f32×4 QWs)
///   vu_mem[base_qw+1..]  — NLOOP×NREG data QWs
///     QW+0: RGBAQ register  → [r,g,b,1.0] as f32
///     QW+1: XYZ2 register   → [x_fixed, y_fixed, z, _] (bit-cast i32 from FTOI4)
pub fn parse_gif_packet(vu_mem: &[[f32; 4]; 1024], base_qw: usize) -> Vec<GsPrimitive> {
    // --- Decode GIF tag (128-bit = two f32[4] QWs merged) ---
    // The GIF tag is stored in a single f32[4] QW (VU mem uses [f32;4] per slot).
    // bit-cast the two f32 pairs as two u64s.
    let tag_qw = vu_mem[base_qw];

    // Low 64 bits: indices [0] and [1] as u32 pairs
    let lo_lo = tag_qw[0].to_bits();   // bits [31:0]
    let lo_hi = tag_qw[1].to_bits();   // bits [63:32]
    let hi_lo = tag_qw[2].to_bits();   // bits [95:64]  (REGS low 32)
    // hi_hi = tag_qw[3] not needed

    // NLOOP[14:0]
    let nloop = (lo_lo & 0x7FFF) as usize;
    // EOP[15] — end of packet (we don't need it here)
    // PRE[46] — whether PRIM field is pre-set
    let pre   = ((lo_hi >> 14) & 1) != 0;
    // PRIM[57:47] in the low 64 bits
    let prim_raw = ((lo_hi >> 15) & 0x7FF) as u16;
    // FLG at lo64[60:59] = lo_hi[28:27]
    let flg   = (lo_hi >> 27) & 0x3;
    // NREG[63:60] — actually bits [63:60] of the low u64
    let nreg_raw = (lo_hi >> 28) & 0xF; // bits [63:60] of lo64 is [31:28] of lo_hi u32
    // Re-read: lo64 = lo_lo | (lo_hi << 32)
    // NREG is bits [63:60] of lo64 → bits [31:28] of lo_hi
    let nreg  = (nreg_raw as usize).max(1); // 0 means 16

    // REGS field in hi64: 4 bits per register (we care about reg0 and reg1)
    let reg0  = (hi_lo & 0xF) as u8;       // first reg descriptor
    let reg1  = ((hi_lo >> 4) & 0xF) as u8; // second reg descriptor

    if nloop == 0 || flg != 0 {
        // Only handle PACKED mode (FLG=0)
        return vec![];
    }

    // IIP (Gouraud) = bit 3 of PRIM
    let iip = pre && ((prim_raw >> 3) & 1) != 0;

    let mut vertices = Vec::with_capacity(nloop);
    let mut cur = base_qw + 1;

    for _ in 0..nloop {
        let mut r = 0u8;
        let mut g = 0u8;
        let mut b = 0u8;
        let mut a = 255u8;
        let mut px = 0i32;
        let mut py = 0i32;

        for reg_idx in 0..nreg {
            if cur >= 1024 {
                break;
            }
            let qw = vu_mem[cur];
            cur += 1;

            let reg_id = if reg_idx == 0 { reg0 } else { reg1 };
            match reg_id {
                0x01 => {
                    // RGBAQ: f32 [r,g,b,a] in [0,1]
                    r = (qw[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    g = (qw[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    b = (qw[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    a = (qw[3].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                0x05 => {
                    // XYZ2: bit-cast i32 from FTOI4 output, then >> 4 for pixel coords
                    let xi = qw[0].to_bits() as i32;
                    let yi = qw[1].to_bits() as i32;
                    px = xi >> 4;
                    py = yi >> 4;
                }
                _ => {} // unknown register — skip
            }
        }

        vertices.push(GifVertex { r, g, b, a, x: px, y: py });
    }

    // Split into triangles (primitive type assumed TRIANGLE_LIST from PRIM)
    vec![GsPrimitive { iip, vertices }]
}
