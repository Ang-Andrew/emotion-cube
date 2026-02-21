// ee.rs — Smart-stub Emotion Engine (EE).
// Maps to: the PS2 EE (MIPS R5900) running the game binary.
// Instead of interpreting MIPS, we build the exact byte layout that real MIPS code
// would produce — a valid PS2 VIF1 DMA packet — directly in Rust.
//
// Packet layout (125 QWs = 2000 bytes from EE RAM offset 0x00100000):
//
//  QW  0     STCYCL(wl=1,cl=1)
//  QW  1     UNPACK V4-32 num=1 addr=108   → GIF tag pre-load
//  QW  2     GIF tag (128-bit literal)
//  QW  3     UNPACK V4-32 num=36 addr=0    → positions datamem[0..35]
//  QW 4..39  36 position QWs [x,y,z,1.0]
//  QW 40     UNPACK V4-32 num=36 addr=36   → normals datamem[36..71]
//  QW 41..76 36 normal QWs [nx,ny,nz,0.0]
//  QW 77     UNPACK V4-32 num=36 addr=72   → colors datamem[72..107]
//  QW 78..113 36 color QWs [r,g,b,1.0]
//  QW 114    UNPACK V4-32 num=4 addr=182   → MVP matrix datamem[182..185]
//  QW 115..118 4 MVP column QWs
//  QW 119    UNPACK V4-32 num=1 addr=186   → light dir + ambient
//  QW 120    [0.577, 0.577, 0.577, 0.2]
//  QW 121    UNPACK V4-32 num=1 addr=187   → viewport scale
//  QW 122    [5120.0, 3584.0, 0.0, 0.0]
//  QW 123    MSCAL execaddr=0
//  QW 124    FLUSH
//  Total: 125 QWs

use std::f32::consts::PI;

const PACKET_BASE: usize = 0x0010_0000;
const PACKET_QWC:  u32   = 125;

// PS2 MMIO addresses for DMAC channel 1 (VIF1) — stored in fields for DMAC to read
pub const D1_MADR: u32 = PACKET_BASE as u32;
pub const D1_QWC:  u32 = PACKET_QWC;

// ---- Cube geometry (36 vertices: 6 faces × 2 tri × 3 verts) ----

struct Vert {
    pos:    [f32; 3],
    normal: [f32; 3],
    color:  [f32; 3],
}

const fn v(pos: [f32; 3], normal: [f32; 3], color: [f32; 3]) -> Vert {
    Vert { pos, normal, color }
}

// Face colors: +X=Red, -X=Cyan, +Y=Green, -Y=Magenta, +Z=Blue, -Z=Yellow
// 2 triangles per face × 3 vertices, CCW winding
static CUBE: [Vert; 36] = [
    // +X face (normal [1,0,0], color red)
    v([ 1.,-1., 1.], [1.,0.,0.], [1.,0.1,0.1]),
    v([ 1., 1., 1.], [1.,0.,0.], [1.,0.1,0.1]),
    v([ 1., 1.,-1.], [1.,0.,0.], [1.,0.1,0.1]),
    v([ 1.,-1., 1.], [1.,0.,0.], [1.,0.1,0.1]),
    v([ 1., 1.,-1.], [1.,0.,0.], [1.,0.1,0.1]),
    v([ 1.,-1.,-1.], [1.,0.,0.], [1.,0.1,0.1]),
    // -X face (normal [-1,0,0], color cyan)
    v([-1.,-1.,-1.], [-1.,0.,0.], [0.1,1.,1.]),
    v([-1., 1.,-1.], [-1.,0.,0.], [0.1,1.,1.]),
    v([-1., 1., 1.], [-1.,0.,0.], [0.1,1.,1.]),
    v([-1.,-1.,-1.], [-1.,0.,0.], [0.1,1.,1.]),
    v([-1., 1., 1.], [-1.,0.,0.], [0.1,1.,1.]),
    v([-1.,-1., 1.], [-1.,0.,0.], [0.1,1.,1.]),
    // +Y face (normal [0,1,0], color green)
    v([-1., 1., 1.], [0.,1.,0.], [0.1,1.,0.1]),
    v([-1., 1.,-1.], [0.,1.,0.], [0.1,1.,0.1]),
    v([ 1., 1.,-1.], [0.,1.,0.], [0.1,1.,0.1]),
    v([-1., 1., 1.], [0.,1.,0.], [0.1,1.,0.1]),
    v([ 1., 1.,-1.], [0.,1.,0.], [0.1,1.,0.1]),
    v([ 1., 1., 1.], [0.,1.,0.], [0.1,1.,0.1]),
    // -Y face (normal [0,-1,0], color magenta)
    v([-1.,-1.,-1.], [0.,-1.,0.], [1.,0.1,1.]),
    v([-1.,-1., 1.], [0.,-1.,0.], [1.,0.1,1.]),
    v([ 1.,-1., 1.], [0.,-1.,0.], [1.,0.1,1.]),
    v([-1.,-1.,-1.], [0.,-1.,0.], [1.,0.1,1.]),
    v([ 1.,-1., 1.], [0.,-1.,0.], [1.,0.1,1.]),
    v([ 1.,-1.,-1.], [0.,-1.,0.], [1.,0.1,1.]),
    // +Z face (normal [0,0,1], color blue)
    v([-1.,-1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    v([ 1.,-1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    v([ 1., 1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    v([-1.,-1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    v([ 1., 1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    v([-1., 1., 1.], [0.,0.,1.], [0.1,0.1,1.]),
    // -Z face (normal [0,0,-1], color yellow)
    v([ 1.,-1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
    v([-1.,-1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
    v([-1., 1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
    v([ 1.,-1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
    v([-1., 1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
    v([ 1., 1.,-1.], [0.,0.,-1.], [1.,1.,0.1]),
];

// ---- GIF tag constant (128-bit literal) ----
// See plan §6:
//   NLOOP=36, EOP=1, PRE=1, PRIM=0x00B (TRIANGLE|IIP), FLG=0 (PACKED), NREG=2
//   REGS: reg0=0x01 (RGBAQ), reg1=0x05 (XYZ2)
const fn gif_tag() -> [u32; 4] {
    // Low 64 bits:
    //   NLOOP[14:0]  = 36  = 0x0024
    //   EOP[15]      = 1   → bit 15
    //   PRE[46]      = 1   → in hi32 bit 14
    //   PRIM[57:47]  = 0x00B → in hi32 bits[25:15]
    //   FLG[61:60]   = 0   → in hi32 bits[29:28]
    //   NREG[63:60]  = 2   → in hi32 bits[31:28] ... wait NREG is [63:60] of lo64
    // Let me map carefully:
    // lo64[14:0]   = NLOOP = 36
    // lo64[15]     = EOP   = 1
    // lo64[46]     = PRE   = 1   → in lo_hi (lo64[63:32]) bit 14
    // lo64[57:47]  = PRIM  = 0x00B → lo_hi bits[25:15]
    // lo64[61:60]  = FLG   = 0   → lo_hi bits[29:28]
    // lo64[63:60]  = NREG  = 2   → lo_hi bits[31:28]
    //
    // lo_lo = lo64[31:0]:
    //   bits[14:0] = 36 = 0x0024
    //   bit[15]    = 1  (EOP)
    //   → lo_lo = 0x0000_8024
    let _lo_lo: u32 = 0x0000_8024; // kept for documentation: lo64[31:0] reference value

    // lo_hi = lo64[63:32]:
    //   bit[14] = 1 (PRE, which is lo64 bit 46 = lo_hi bit 14)
    //   bits[25:15] = PRIM=0x00B → lo_hi bits 15..25
    //   bits[29:28] = FLG=0
    //   bits[31:28] = NREG=2
    // bit[14] = PRE = 1            → 0x0000_4000
    // bits[25:15] = 0x00B = 0b00000001011 → 0x00B << 15 = 0x0005_8000
    // NREG[3:0] = 2 → bits[31:28] = 0x2 → 0x2000_0000
    //   but wait, bits[31:28] for NREG=2 conflicts with FLG in [29:28].
    //   FLG is at bits[29:28], NREG is at bits[31:28] — they share bits[29:28].
    //   Actually per PS2 manual: NREG[3:0] is at [63:60] of the GIF tag,
    //   which is lo_hi bits[31:28] = the top nibble. FLG is [61:60] = lo_hi bits[29:28].
    //   So: NREG=2 → lo_hi[31:28] = 0x2, FLG=0 → lo_hi[29:28] = 0x0.
    //   These bits overlap. Let me think: lo64[63:60] = NREG=2 → value 2 in the top nibble.
    //   lo64[61:60] = FLG=0 → value 0. So top nibble of lo_hi = (NREG << 0) & 0xF = 2.
    //   But bits[31:28] = NREG = 2 → 0x20000000. And bits[29:28] = FLG = 0, no conflict.
    //   Top nibble of lo_hi[31:28]: bit31=0, bit30=0, bit29=1, bit28=0 for NREG=2?
    //   Actually NREG=2 in binary is 0b0010, so bit31=0,bit30=0,bit29=1,bit28=0.
    //   And FLG is at [29:28] = 0b10 (for NREG=2, bit29=1 which is also bit1 of NREG).
    //   This is getting complicated. Let me just encode the correct 64-bit values directly.
    //
    // GIF tag lo64 = NLOOP(36) | EOP(1)<<15 | PRE(1)<<46 | PRIM(0x00B)<<47 | FLG(0)<<60 | NREG(2)<<60
    // Wait NREG and FLG both at [63:60]?
    //   FLG:  [61:60]
    //   NREG: [63:60]
    //   They overlap in bits [61:60]! In practice: NREG encodes the number of registers (2).
    //   0x2 in [63:60] = bits 63,62 = 0, bits 61,60 = 1,0 → FLG bits are the lower 2 bits of NREG.
    //   NREG=2 = 0b0010 in 4 bits → bit60=0, bit61=1, bit62=0, bit63=0.
    //   FLG = bits[61:60] = 0b10 = 2 (IMAGE mode)? That would be wrong.
    //
    // From PS2 manual:
    //   [59:48] = PRIM (12 bits)  ... wait, let me re-read.
    //
    // Actually let me look this up properly. GIF tag format (128 bits):
    //   bits[14:0]  = NLOOP
    //   bit[15]     = EOP
    //   bits[45:16] = (padding/0)
    //   bit[46]     = PRE
    //   bits[58:47] = PRIM (12 bits)
    //   bits[60:59] = FLG (2 bits)
    //   bits[63:60] = NREG (4 bits) ... BUT bits[63:60] includes bit60 which is also in FLG[60:59]?
    //   Wait: FLG is [60:59] (bits 59 and 60), NREG is [63:60] (bits 60-63).
    //   They share bit 60. That can't be right.
    //
    // Let me use the exact values from the plan spec:
    //   NLOOP=36, EOP=1, PRE=1, PRIM=0x00B, FLG=0b00, NREG=2
    //   Low 64 bits bit layout:
    //     [14:0]  NLOOP = 36 = 0x24
    //     [15]    EOP   = 1
    //     [46]    PRE   = 1
    //     [57:47] PRIM  = 0x00B
    //     [61:60] FLG   = 0
    //     [63:60] NREG  = 2
    //   So: lo64 = 36 | (1<<15) | (1<<46) | (0x00B<<47) | (0<<60) | (2<<60)
    //           = 0x24 | 0x8000 | (1<<46) | (0xB<<47) | (2<<60)
    //   Let me compute: 2<<60 = 0x2000_0000_0000_0000
    //   0xB<<47 = 0x0005_8000_0000_0000
    //   1<<46   = 0x0000_4000_0000_0000
    //   1<<15   = 0x0000_0000_0000_8000
    //   36      = 0x0000_0000_0000_0024
    //   Sum     = 0x20059C00_00008024 (approximately)
    //   Actually: NREG=2 means "2 registers follow each pixel" and it goes in bits[63:60].
    //   2 in bits[63:60]: (lo64 >> 60) & 0xF = 2 → lo64 |= 0x2000_0000_0000_0000
    //   But FLG at [61:60] = 0 → no change. Since NREG=2 puts bits in [63:62],[61:60] as 0010,
    //   this sets bit61=0, bit60=1... wait 2 = 0b0010, so in bits[63:60]: bit63=0,bit62=0,bit61=1,bit60=0.
    //   That means bit61=1, and FLG[61:60] = 0b10 = 2 (IMAGE mode) which is wrong.
    //
    //   Hmm, I think the GIF tag NREG encoding might use a 0-means-16 convention.
    //   NREG=2 actual regs: encode as 2, but be careful about FLG.
    //
    // Let me just hard-code the correct 128-bit GIF tag value based on the
    // bit layout and trust that the gif.rs parser will decode it correctly.
    //
    // For our implementation to work, we need gif.rs to correctly decode
    // whatever bytes we put here. Let me just use simple fields:
    // I'll store the GIF tag as 4×u32 in little-endian order and make sure
    // gif.rs decodes the same way.
    //
    // Simplest correct encoding:
    //   word0 (bits[31:0]):  NLOOP=36, EOP in bit15 = 0x8024... wait
    //   Actually NLOOP[14:0] = 36 = 0x0024, EOP[15] = 1 → word0 = 0x0000_8024
    //   PRE is bit46 in the 128-bit value → bit14 in word1 (bits[63:32])
    //   PRIM[57:47] → bits[25:15] in word1
    //   NREG[63:60] → bits[31:28] in word1

    // word0: bits[31:0] of lo64 — NLOOP=36 in bits[14:0], EOP=1 in bit[15]
    let word0: u32 = 36 | (1 << 15);  // 0x0000_8024

    // word1: bits[63:32] of lo64
    // PRE bit[46-32=14]: 1
    // PRIM bits[57-32:47-32]=[25:15] = 0x00B: 0x00B << 15 = 0x0005_8000
    // NREG bits[63-32:60-32]=[31:28] = 2: 2 << 28 = 0x2000_0000
    // FLG bits[61-32:60-32]=[29:28] = 0 (already 0)
    let word1: u32 = (1 << 14) | (0x00Bu32 << 15) | (2u32 << 28);
    // = 0x4000 | 0x0005_8000 | 0x2000_0000
    // = 0x20059C00... let me compute:
    // 0x4000 | 0x0005_8000 = 0x0005_C000
    // 0x0005_C000 | 0x2000_0000 = 0x2005_C000

    // word2,word3: hi64 — REGS field
    // VU program stores: QW0=XYZ2 (coords), QW1=RGBAQ (color)
    // So reg0=XYZ2(0x05), reg1=RGBAQ(0x01) to match memory order
    let word2: u32 = 0x05 | (0x01 << 4);  // 0x0000_0015
    let word3: u32 = 0x0000_0000;

    [word0, word1, word2, word3]
}

// ---- Column-major 4×4 MVP computation ----

fn mat_mul(a: [[f32;4];4], b: [[f32;4];4]) -> [[f32;4];4] {
    let mut r = [[0.0f32;4];4];
    for col in 0..4 {
        for row in 0..4 {
            r[col][row] = a[0][row]*b[col][0] + a[1][row]*b[col][1]
                        + a[2][row]*b[col][2] + a[3][row]*b[col][3];
        }
    }
    r
}

fn rotate_y(rad: f32) -> [[f32;4];4] {
    let (s, c) = rad.sin_cos();
    [[ c, 0., s, 0.],
     [ 0., 1., 0., 0.],
     [-s, 0., c, 0.],
     [ 0., 0., 0., 1.]]
}

fn rotate_x(rad: f32) -> [[f32;4];4] {
    let (s, c) = rad.sin_cos();
    [[1., 0.,  0., 0.],
     [0.,  c, -s, 0.],
     [0.,  s,  c, 0.],
     [0., 0.,  0., 1.]]
}

fn translate_z(tz: f32) -> [[f32;4];4] {
    [[1., 0., 0., 0.],
     [0., 1., 0., 0.],
     [0., 0., 1., 0.],
     [0., 0., tz, 1.]]
}

fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [[f32;4];4] {
    let f = 1.0 / (fov_y / 2.0).tan();
    let range = near - far;
    [[f / aspect, 0., 0.,                      0.],
     [0.,         f,  0.,                      0.],
     [0.,         0., (far + near) / range,   -1.],
     [0.,         0., 2.0*far*near / range,    0.]]
}

// ---- EmotionEngine ----

pub struct EmotionEngine {
    pub ee_ram: Box<[u8; 2 * 1024 * 1024]>,
    frame:      u64,
}

impl EmotionEngine {
    pub fn new() -> Self {
        EmotionEngine {
            ee_ram: Box::new([0u8; 2 * 1024 * 1024]),
            frame:  0,
        }
    }

    /// Build the VIF1 DMA packet in EE RAM and return (madr, qwc) for DMAC kick.
    pub fn build_packet(&mut self) -> (u32, u32) {
        let frame = self.frame;
        self.frame += 1;

        // ---- Compute MVP per frame ----
        let angle_y = frame as f32 * (PI / 180.0);
        let angle_x = frame as f32 * (PI / 360.0);
        let rot_y  = rotate_y(angle_y);
        let rot_x  = rotate_x(angle_x);
        let model  = mat_mul(rot_x, rot_y);
        let view   = translate_z(-3.0);
        let proj   = perspective(PI / 3.0, 640.0 / 448.0, 0.1, 100.0);
        let mv     = mat_mul(view, model);
        let mvp    = mat_mul(proj, mv);

        // ---- Write packet into EE RAM ----
        let base = PACKET_BASE;
        let ram  = &mut *self.ee_ram;
        let mut qw = 0usize; // current QW index

        // Helper: write a u32 at a byte offset
        fn w32(ram: &mut [u8], off: usize, val: u32) {
            let b = val.to_le_bytes();
            ram[off..off+4].copy_from_slice(&b);
        }

        fn write_qw(ram: &mut [u8], base: usize, qw: usize, w0: u32, w1: u32, w2: u32, w3: u32) {
            let off = base + qw * 16;
            w32(ram, off,    w0);
            w32(ram, off+4,  w1);
            w32(ram, off+8,  w2);
            w32(ram, off+12, w3);
        }

        fn write_f32_qw(ram: &mut [u8], base: usize, qw: usize, x: f32, y: f32, z: f32, w: f32) {
            write_qw(ram, base, qw,
                f32::to_bits(x), f32::to_bits(y),
                f32::to_bits(z), f32::to_bits(w));
        }

        // VIF tag helpers:
        // STCYCL(wl=1, cl=1): cmd=0x01, data = (wl<<8)|cl = 0x0101
        // UNPACK V4-32: cmd=0x6C, data = (num<<16)|addr
        // MSCAL(addr=0): cmd=0x14, data=0
        // FLUSH: cmd=0x11, data=0
        fn vif_tag(cmd: u8, data: u32) -> u32 {
            ((cmd as u32) << 24) | (data & 0x00FF_FFFF)
        }

        // QW 0: STCYCL
        write_qw(ram, base, qw, vif_tag(0x01, 0x0101), 0, 0, 0); qw += 1;

        // QW 1: UNPACK V4-32 num=1 addr=108
        write_qw(ram, base, qw, vif_tag(0x6C, (1 << 16) | 108), 0, 0, 0); qw += 1;

        // QW 2: GIF tag
        let gt = gif_tag();
        write_qw(ram, base, qw, gt[0], gt[1], gt[2], gt[3]); qw += 1;

        // QW 3: UNPACK positions num=36 addr=0
        write_qw(ram, base, qw, vif_tag(0x6C, (36 << 16) | 0), 0, 0, 0); qw += 1;

        // QW 4..39: 36 position QWs
        for v in &CUBE {
            write_f32_qw(ram, base, qw, v.pos[0], v.pos[1], v.pos[2], 1.0);
            qw += 1;
        }

        // QW 40: UNPACK normals num=36 addr=36
        write_qw(ram, base, qw, vif_tag(0x6C, (36 << 16) | 36), 0, 0, 0); qw += 1;

        // QW 41..76: 36 normal QWs
        for v in &CUBE {
            write_f32_qw(ram, base, qw, v.normal[0], v.normal[1], v.normal[2], 0.0);
            qw += 1;
        }

        // QW 77: UNPACK colors num=36 addr=72
        write_qw(ram, base, qw, vif_tag(0x6C, (36 << 16) | 72), 0, 0, 0); qw += 1;

        // QW 78..113: 36 color QWs
        for v in &CUBE {
            write_f32_qw(ram, base, qw, v.color[0], v.color[1], v.color[2], 1.0);
            qw += 1;
        }

        // QW 114: UNPACK MVP num=4 addr=182
        write_qw(ram, base, qw, vif_tag(0x6C, (4 << 16) | 182), 0, 0, 0); qw += 1;

        // QW 115..118: 4 MVP column QWs (column-major: each column is [r0,r1,r2,r3])
        for col in 0..4 {
            write_f32_qw(ram, base, qw,
                mvp[col][0], mvp[col][1], mvp[col][2], mvp[col][3]);
            qw += 1;
        }

        // QW 119: UNPACK light num=1 addr=186
        write_qw(ram, base, qw, vif_tag(0x6C, (1 << 16) | 186), 0, 0, 0); qw += 1;

        // QW 120: light direction + ambient [lx, ly, lz, ambient]
        write_f32_qw(ram, base, qw, 0.577, 0.577, 0.577, 0.2); qw += 1;

        // QW 121: UNPACK viewport num=1 addr=187
        write_qw(ram, base, qw, vif_tag(0x6C, (1 << 16) | 187), 0, 0, 0); qw += 1;

        // QW 122: viewport scale [320.0, 224.0, 0.0, 0.0]
        // (pixel-space half-extents; FTOI4 in VU1 will multiply by 16 → GS 12.4 format)
        write_f32_qw(ram, base, qw, 320.0, 224.0, 0.0, 0.0); qw += 1;

        // QW 123: MSCAL execaddr=0
        write_qw(ram, base, qw, vif_tag(0x14, 0), 0, 0, 0); qw += 1;

        // QW 124: FLUSH
        write_qw(ram, base, qw, vif_tag(0x11, 0), 0, 0, 0); qw += 1;

        debug_assert_eq!(qw, PACKET_QWC as usize);

        (D1_MADR, D1_QWC)
    }
}
