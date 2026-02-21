// vu1_program.rs — Hand-encoded VU1 micro-program.
// Maps to: the micro-program uploaded into VU1 instruction memory (code_mem).
//
// Each u64 = one VU1 instruction: upper[63:32] | lower[31:0]
//
// Upper slot (bits [63:32]) — FPU/VF operations:
//   [30]    = E-bit (end program after next instruction)
//   [27:24] = dest mask  (xyzw)
//   [23:19] = ft  (5 bits)
//   [18:14] = fs  (5 bits)
//   [13:9]  = fd  (5 bits; for accumulator ops fd=0)
//   [8:0]   = opcode (9 bits)
//
// Upper op9 encoding:
//   0x000+bc  ADDbc    VFfd.dest = VFfs.dest + VFft.bc
//   0x004+bc  SUBbc    VFfd.dest = VFfs.dest - VFft.bc
//   0x008+bc  MADDbc   VFfd.dest = ACC.dest + VFfs.dest * VFft.bc
//   0x010+bc  MAXbc    VFfd.dest = max(VFfs.dest, VFft.bc)
//   0x014+bc  MINIbc   VFfd.dest = min(VFfs.dest, VFft.bc)
//   0x018+bc  MULbc    VFfd.dest = VFfs.dest * VFft.bc
//   0x01C     MULq     VFfd.dest = VFfs.dest * Q
//   0x020+bc  MULAbc   ACC.dest = VFfs.dest * VFft.bc
//   0x038+bc  MADDAbc  ACC.dest += VFfs.dest * VFft.bc
//   0x070     DIV      Q = VFfs.fsf / VFft.ftf (fd[3:2]=fsf, fd[1:0]=ftf)
//   0x073     WAITQ    stall until Q ready
//   0x17C     FTOI4    VFfd[i] = round(VFfs[i]*16) as i32 (bit-cast to f32)
//   0x1FF     NOP
//
// Lower slot (bits [31:0]) — integer/memory/branch:
//   op6 = [31:26]
//   0x20 (0b100000) NOP  (canonical: 0x8000_0000)
//   0x27 (0b100111) IADDIU vt,vs,imm15:   VI[vt] = VI[vs] + sext(imm15)
//   0x23 (0b100011) IBNE  vs,vt,off11:    if VI[vs]!=VI[vt]: PC = PC+1+sext(off11)
//   0x32 (0b110010) XGKICK is:            return VI[is] (end micro-program)
//   0x3A (0b111010) LQI  ft,(is++):       VF[ft] = data_mem[VI[is]]; VI[is]++
//   0x3E (0b111110) SQI  fs,(it++):       data_mem[VI[it]] = VF[fs]; VI[it]++

// ---- Broadcast component constants ----
const X: u32 = 0;
const Y: u32 = 1;
const Z: u32 = 2;
const W: u32 = 3;

// ---- Destination mask ----
const DEST_XYZW: u32 = 0xF;
const DEST_XY:   u32 = 0b1100;
const DEST_X:    u32 = 0b1000;
const DEST_Y:    u32 = 0b0100;

// ---- Upper slot encoding ----

/// Generic bc-flavored upper op: op9 = op_base | bc
const fn ubc(dest: u32, fd: u32, fs: u32, ft: u32, op_base: u32, bc: u32) -> u32 {
    let op9 = op_base | bc;
    (dest << 24) | (ft << 19) | (fs << 14) | (fd << 9) | op9
}

const fn u_nop() -> u32 { 0x0000_01FF }  // op9=0x1FF, all regs 0

/// DIV Q, VFfs.fsf / VFft.ftf
/// op9=0x70, fd field encodes fsf[1:0] in bits [10:9] and ftf[1:0] in bits [12:11]
/// We use: fd[3:2]=fsf, fd[1:0]=ftf packed in the 5-bit fd field
const fn u_div(fs: u32, fsf: u32, ft: u32, ftf: u32) -> u32 {
    let fd_enc = (fsf << 2) | ftf;
    (ft << 19) | (fs << 14) | (fd_enc << 9) | 0x070
}

/// MULq.dest VFfd, VFfs  (ft=0 implicit Q)
const fn u_mulq(dest: u32, fd: u32, fs: u32) -> u32 {
    (dest << 24) | (fs << 14) | (fd << 9) | 0x01C
}

const fn u_waitq() -> u32 { 0x073 }  // WAITQ: no registers, op9=0x73

/// FTOI4.dest VFfd, VFfs
const fn u_ftoi4(dest: u32, fd: u32, fs: u32) -> u32 {
    (dest << 24) | (fs << 14) | (fd << 9) | 0x17C
}

// ---- Lower slot encoding ----

const fn l_nop() -> u32 { 0x8000_0000 }

/// IADDIU VI[vt], VI[vs], imm15 (signed 15-bit immediate)
const fn l_iaddiu(vt: u32, vs: u32, imm: i16) -> u32 {
    let imm15 = (imm as u32) & 0x7FFF;
    (0x27 << 26) | (vt << 21) | (vs << 16) | imm15
}

/// IBNE VI[vs], VI[vt], off11 — branch if not equal; target = PC+1+sext(off11)
const fn l_ibne(vs: u32, vt: u32, off: i16) -> u32 {
    let off11 = (off as u32) & 0x7FF;
    (0x23 << 26) | (vs << 21) | (vt << 16) | off11
}

/// XGKICK VI[is] — kick GIF, end program
const fn l_xgkick(is: u32) -> u32 {
    (0x32 << 26) | (is << 16)
}

/// LQI VF[ft], (VI[is]++)
const fn l_lqi(ft: u32, is: u32) -> u32 {
    (0x3A << 26) | (ft << 21) | (is << 16)
}

/// SQI VF[fs], (VI[it]++)
const fn l_sqi(fs: u32, it: u32) -> u32 {
    (0x3E << 26) | (fs << 21) | (it << 11)
}

// ---- Assemble u64 instruction ----
const fn i(upper: u32, lower: u32) -> u64 {
    ((upper as u64) << 32) | (lower as u64)
}

// ========================================================================
// VU1 Micro-program
//
// VF register usage:
//   VF00  hardwired [0,0,0,1]
//   VF01-04  MVP columns (loaded from datamem[182..185])
//   VF05     light dir + ambient [lx,ly,lz,amb]
//   VF09     viewport scale [5120,3584,0,0]
//   VF10     current vertex position (xyzw)
//   VF11     current vertex normal  (xyz0)
//   VF12     current vertex color   (rgba)
//   VF15     clip pos → NDC → GS subpixel coords
//   VF16     diffuse lighting intensity
//   VF17     final modulated color
//
// VI register usage:
//   VI00  hardwired 0
//   VI01  pos input ptr   (datamem[0])
//   VI02  output write ptr (datamem[109], advances as we write)
//   VI03  loop counter    (36 → 0)
//   VI04  temp load ptr
//   VI05  XGKICK base ptr (datamem[109], fixed)
//   VI06  norm input ptr  (datamem[36])
//   VI07  color input ptr (datamem[72])
//
// Instruction count by section:
//   PC  0- 6: preamble (7 instructions)
//   PC  7-12: load constants (6 instructions)
//   PC 13-38: loop body (26 instructions per iteration)
//   PC 39:    IBNE branch-back (target PC=13, offset=-27)
//   PC 40:    XGKICK (end of program)
// ========================================================================

pub const VU1_MICRO: &[u64] = {
    // Opcode bases (op9 = base | bc):
    const ADD:   u32 = 0x000;
    const SUB:   u32 = 0x004;
    const MADD:  u32 = 0x008;
    const MAX:   u32 = 0x010;
    const MINI:  u32 = 0x014;
    const MUL:   u32 = 0x018;
    const MULA:  u32 = 0x020;
    const MADDA: u32 = 0x038;

    &[
        // ----------------------------------------------------------------
        // PC 0-6: Preamble — initialise VI register pointers
        // ----------------------------------------------------------------
        i(u_nop(), l_iaddiu(1, 0,   0)),   // VI01 = 0    (pos ptr)
        i(u_nop(), l_iaddiu(6, 0,  36)),   // VI06 = 36   (norm ptr)
        i(u_nop(), l_iaddiu(7, 0,  72)),   // VI07 = 72   (color ptr)
        i(u_nop(), l_iaddiu(2, 0, 109)),   // VI02 = 109  (output write ptr)
        i(u_nop(), l_iaddiu(5, 0, 108)),   // VI05 = 108  (XGKICK base = GIF tag addr)
        i(u_nop(), l_iaddiu(3, 0,  36)),   // VI03 = 36   (loop counter)
        i(u_nop(), l_iaddiu(4, 0, 182)),   // VI04 = 182  (const load ptr)

        // ----------------------------------------------------------------
        // PC 7-12: Load MVP / light / viewport from datamem[182..]
        // ----------------------------------------------------------------
        i(u_nop(), l_lqi(1,  4)),   // VF01 = datamem[182] — MVP col0
        i(u_nop(), l_lqi(2,  4)),   // VF02 = datamem[183] — MVP col1
        i(u_nop(), l_lqi(3,  4)),   // VF03 = datamem[184] — MVP col2
        i(u_nop(), l_lqi(4,  4)),   // VF04 = datamem[185] — MVP col3
        i(u_nop(), l_lqi(5,  4)),   // VF05 = datamem[186] — light [lx,ly,lz,amb]
        i(u_nop(), l_lqi(9,  4)),   // VF09 = datamem[187] — viewport [5120,3584,0,0]

        // ----------------------------------------------------------------
        // PC 13-15: Load per-vertex data
        // ----------------------------------------------------------------
        // LOOP (PC=13):
        i(u_nop(), l_lqi(10, 1)),   // VF10 = pos  [x,y,z,1]  VI01++
        i(u_nop(), l_lqi(11, 6)),   // VF11 = norm [nx,ny,nz,0]  VI06++
        i(u_nop(), l_lqi(12, 7)),   // VF12 = color [r,g,b,1]  VI07++

        // ----------------------------------------------------------------
        // PC 16-19: MVP matrix transform — clip = MVP × pos
        //   clip = VF04*pos.w + VF01*pos.x + VF02*pos.y + VF03*pos.z
        // ----------------------------------------------------------------
        i(ubc(DEST_XYZW, 0, 4, 10, MULA, W), l_nop()),  // ACC  = VF04 * VF10.w
        i(ubc(DEST_XYZW, 0, 1, 10, MADDA, X), l_nop()), // ACC += VF01 * VF10.x
        i(ubc(DEST_XYZW, 0, 2, 10, MADDA, Y), l_nop()), // ACC += VF02 * VF10.y
        i(ubc(DEST_XYZW, 15, 3, 10, MADD, Z), l_nop()), // VF15 = ACC + VF03*VF10.z

        // ----------------------------------------------------------------
        // PC 20: Start DIV (Q = 1/clip.w; 7-cycle latency)
        // VF00.w = 1.0 hardwired
        // ----------------------------------------------------------------
        i(u_div(0, W, 15, W), l_nop()),   // Q = VF00.w / VF15.w

        // ----------------------------------------------------------------
        // PC 21-27: Gouraud lighting (7 instructions fill DIV latency)
        //   dot(norm, light) → clamp → add ambient
        // ----------------------------------------------------------------
        i(ubc(DEST_XYZW, 0, 11,  5, MULA,  X), l_nop()), // ACC  = VF11 * VF05.x
        i(ubc(DEST_XYZW, 0, 11,  5, MADDA, Y), l_nop()), // ACC += VF11 * VF05.y
        i(ubc(DEST_XYZW, 16, 11, 5, MADD,  Z), l_nop()), // VF16 = ACC + VF11*VF05.z  (dot)
        i(ubc(DEST_XYZW, 16, 16, 0, MAX,   X), l_nop()), // VF16 = max(VF16, VF00.x=0) — clamp≥0
        i(ubc(DEST_XYZW, 16, 16, 0, MINI,  W), l_nop()), // VF16 = min(VF16, VF00.w=1) — clamp≤1
        i(ubc(DEST_XYZW, 16, 16, 5, ADD,   W), l_nop()), // VF16 += VF05.w (ambient=0.2)
        i(ubc(DEST_XYZW, 16, 16, 0, MINI,  W), l_nop()), // VF16 = min(VF16, 1.0) — final clamp

        // ----------------------------------------------------------------
        // PC 28: Modulate base color by lighting intensity
        // ----------------------------------------------------------------
        i(ubc(DEST_XYZW, 17, 12, 16, MUL, X), l_nop()),  // VF17 = VF12 * VF16.x

        // ----------------------------------------------------------------
        // PC 29: WAITQ — stall until DIV result is ready
        // ----------------------------------------------------------------
        i(u_waitq(), l_nop()),

        // ----------------------------------------------------------------
        // PC 30: Perspective divide — NDC = clip.xyz * Q
        // ----------------------------------------------------------------
        i(u_mulq(0b1110 /*xyz*/, 15, 15), l_nop()),  // VF15.xyz = VF15.xyz * Q

        // ----------------------------------------------------------------
        // PC 31-34: Viewport transform → GS subpixel coordinates
        //   gs_x = (ndcx + 1) * 5120  =  ndcx*5120 + 5120
        //   gs_y = (1 - ndcy) * 3584  =  3584 - ndcy*3584
        // ----------------------------------------------------------------
        i(ubc(DEST_X, 0,  15,  9, MULA, X), l_nop()),  // ACC.x = VF15.x * VF09.x
        // MADDw.x VF15, VF09, VF00.w  → VF15.x = ACC.x + VF09.x * 1.0
        i(ubc(DEST_X, 15, 9,   0, MADD, W), l_nop()),  // VF15.x = ACC.x + VF09.x
        i(ubc(DEST_Y, 15, 15,  9, MUL,  Y), l_nop()),  // VF15.y = VF15.y * VF09.y
        // SUBbc.y VF15, VF09, VF15.y  → VF15.y = VF09.y - VF15.y  (Y flip)
        i(ubc(DEST_Y, 15,  9, 15, SUB,  Y), l_nop()),  // VF15.y = 3584 - ndcy*3584

        // ----------------------------------------------------------------
        // PC 35: FTIO4 — convert VF15.xy to GS 12.4 fixed-point integers
        // ----------------------------------------------------------------
        i(u_ftoi4(DEST_XY, 15, 15), l_nop()),

        // ----------------------------------------------------------------
        // PC 36-38: Store output, decrement counter
        // ----------------------------------------------------------------
        i(u_nop(), l_sqi(15, 2)),            // data_mem[VI02++] = VF15 (GS coords)
        i(u_nop(), l_sqi(17, 2)),            // data_mem[VI02++] = VF17 (color)
        i(u_nop(), l_iaddiu(3, 3, -1)),      // VI03--

        // ----------------------------------------------------------------
        // PC 39: Branch back to loop start (PC=13) if counter not zero
        //   offset = 13 - (39+1) = -27
        // ----------------------------------------------------------------
        i(u_nop(), l_ibne(3, 0, -27)),

        // ----------------------------------------------------------------
        // PC 40: XGKICK — signal GIF DMA start, end micro-program
        // ----------------------------------------------------------------
        i(u_nop(), l_xgkick(5)),
    ]
};
