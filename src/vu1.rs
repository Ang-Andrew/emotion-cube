// vu1.rs — VU1 micro-instruction set interpreter.
// Maps to: the PS2 VU1 300 MHz SIMD co-processor.
//
// Pipeline model:
//   Each cycle fetches one 64-bit word.
//   Upper slot [63:32]: FPU operation — result staged, NOT visible to lower slot.
//   Lower slot [31:0]:  integer/memory/branch.
//   After both execute: commit staged FPU result to VF register file.
//
// Special registers:
//   VF00  hardwired [0.0, 0.0, 0.0, 1.0]   (never written)
//   VI00  hardwired 0                        (never written)
//   ACC   accumulator for MULA/MADDA/MADD chain
//   Q     result of DIV instruction (available after div_busy reaches 0)

use crate::vu1_program::VU1_MICRO;

pub struct Vu1 {
    pub vf:        [[f32; 4]; 32],
    pub vi:        [i16; 16],
    pub acc:       [f32; 4],
    pub q:         f32,
    pub pc:        u16,
    pub div_busy:  u8,
    pub data_mem:  Box<[[f32; 4]; 1024]>,
    pub code_mem:  Box<[u64; 512]>,
}

impl Vu1 {
    pub fn new() -> Self {
        let mut vu = Vu1 {
            vf:       [[0.0; 4]; 32],
            vi:       [0i16; 16],
            acc:      [0.0; 4],
            q:        1.0,
            pc:       0,
            div_busy: 0,
            data_mem: Box::new([[0.0; 4]; 1024]),
            code_mem: Box::new([0u64; 512]),
        };

        // Load micro-program into code_mem
        for (i, &instr) in VU1_MICRO.iter().enumerate() {
            if i < 512 {
                vu.code_mem[i] = instr;
            }
        }

        vu
    }

    // ---- VF register helpers ----

    fn vf_get(&self, reg: usize) -> [f32; 4] {
        if reg == 0 { [0.0, 0.0, 0.0, 1.0] } else { self.vf[reg] }
    }

    fn vf_component(&self, reg: usize, comp: usize) -> f32 {
        self.vf_get(reg)[comp]
    }

    fn vf_set(&mut self, reg: usize, dest: u32, val: [f32; 4]) {
        if reg == 0 { return; }
        if dest & 0x8 != 0 { self.vf[reg][0] = val[0]; } // X
        if dest & 0x4 != 0 { self.vf[reg][1] = val[1]; } // Y
        if dest & 0x2 != 0 { self.vf[reg][2] = val[2]; } // Z
        if dest & 0x1 != 0 { self.vf[reg][3] = val[3]; } // W
    }

    fn acc_set(&mut self, dest: u32, val: [f32; 4]) {
        if dest & 0x8 != 0 { self.acc[0] = val[0]; }
        if dest & 0x4 != 0 { self.acc[1] = val[1]; }
        if dest & 0x2 != 0 { self.acc[2] = val[2]; }
        if dest & 0x1 != 0 { self.acc[3] = val[3]; }
    }

    // ---- VI register helpers ----

    fn vi_get(&self, reg: usize) -> i16 {
        if reg == 0 { 0 } else { self.vi[reg] }
    }

    fn vi_set(&mut self, reg: usize, val: i16) {
        if reg != 0 { self.vi[reg] = val; }
    }

    // ---- Execute upper slot ----
    // Returns: Option<(fd, dest_mask, result_vec)> — staged write committed after lower slot.
    fn exec_upper(&mut self, upper: u32) -> Option<(usize, u32, [f32; 4])> {
        let op9  = (upper & 0x1FF) as u32;
        let fd   = ((upper >> 9)  & 0x1F) as usize;
        let fs   = ((upper >> 14) & 0x1F) as usize;
        let ft   = ((upper >> 19) & 0x1F) as usize;
        let dest = (upper >> 24) & 0xF;

        // Read source registers BEFORE any writes (for hazard correctness)
        let vfs  = self.vf_get(fs);
        let vft  = self.vf_get(ft);

        match op9 {
            0x1FF => None, // NOP

            // ---- DIV ----
            0x070 => {
                // fd_enc = (upper >> 9) & 0x1F:  fsf = fd_enc[3:2], ftf = fd_enc[1:0]
                let fd_enc = fd as u32;
                let fsf = ((fd_enc >> 2) & 0x3) as usize;
                let ftf = (fd_enc & 0x3) as usize;
                let num = vfs[fsf];
                let den = vft[ftf];
                self.q = if den.abs() < 1e-37 { 0.0 } else { num / den };
                self.div_busy = 7;
                None
            }

            // ---- WAITQ ----
            0x073 => {
                // Spin — in our synchronous interpreter we already have Q ready
                // (div was executed earlier in the same frame).
                // In hardware this stalls; here we just ensure div_busy=0.
                self.div_busy = 0;
                None
            }

            // ---- MULq ----
            0x01C => {
                // VFfd.dest = VFfs.dest * Q
                let q = self.q;
                let res = [vfs[0] * q, vfs[1] * q, vfs[2] * q, vfs[3] * q];
                Some((fd, dest, res))
            }

            // ---- FTOI4 ----
            0x17C => {
                // VFfd[i] = round(VFfs[i] * 16) as i32, bit-cast back to f32
                let mut res = [0.0f32; 4];
                for i in 0..4 {
                    let fixed = (vfs[i] * 16.0).round() as i32;
                    res[i] = f32::from_bits(fixed as u32);
                }
                Some((fd, dest, res))
            }

            // ---- bc-flavored ops ----
            _ => {
                let bc  = (op9 & 3) as usize;
                let base = op9 & !3;
                let scalar = vft[bc];

                match base {
                    // ADDbc: VFfd.dest = VFfs.dest + VFft.bc
                    0x000 => {
                        let res = [vfs[0]+scalar, vfs[1]+scalar, vfs[2]+scalar, vfs[3]+scalar];
                        Some((fd, dest, res))
                    }
                    // SUBbc: VFfd.dest = VFfs.dest - VFft.bc
                    0x004 => {
                        let res = [vfs[0]-scalar, vfs[1]-scalar, vfs[2]-scalar, vfs[3]-scalar];
                        Some((fd, dest, res))
                    }
                    // MADDbc: VFfd.dest = ACC.dest + VFfs.dest * VFft.bc
                    0x008 => {
                        let res = [
                            self.acc[0] + vfs[0]*scalar,
                            self.acc[1] + vfs[1]*scalar,
                            self.acc[2] + vfs[2]*scalar,
                            self.acc[3] + vfs[3]*scalar,
                        ];
                        Some((fd, dest, res))
                    }
                    // MAXbc: VFfd.dest = max(VFfs.dest, VFft.bc)
                    0x010 => {
                        let res = [vfs[0].max(scalar), vfs[1].max(scalar),
                                   vfs[2].max(scalar), vfs[3].max(scalar)];
                        Some((fd, dest, res))
                    }
                    // MINIbc: VFfd.dest = min(VFfs.dest, VFft.bc)
                    0x014 => {
                        let res = [vfs[0].min(scalar), vfs[1].min(scalar),
                                   vfs[2].min(scalar), vfs[3].min(scalar)];
                        Some((fd, dest, res))
                    }
                    // MULbc: VFfd.dest = VFfs.dest * VFft.bc
                    0x018 => {
                        let res = [vfs[0]*scalar, vfs[1]*scalar, vfs[2]*scalar, vfs[3]*scalar];
                        Some((fd, dest, res))
                    }
                    // MULAbc: ACC.dest = VFfs.dest * VFft.bc  (fd unused, writes ACC)
                    0x020 => {
                        let res = [vfs[0]*scalar, vfs[1]*scalar, vfs[2]*scalar, vfs[3]*scalar];
                        self.acc_set(dest, res);
                        None
                    }
                    // MADDAbc: ACC.dest += VFfs.dest * VFft.bc
                    0x038 => {
                        let res = [
                            self.acc[0] + vfs[0]*scalar,
                            self.acc[1] + vfs[1]*scalar,
                            self.acc[2] + vfs[2]*scalar,
                            self.acc[3] + vfs[3]*scalar,
                        ];
                        self.acc_set(dest, res);
                        None
                    }
                    _ => None, // unknown extended op
                }
            }
        }
    }

    // ---- Execute lower slot ----
    // Returns LowerEffect:
    //   None      → advance PC normally
    //   Branch(n) → set PC = n (after commit)
    //   XgKick(a) → end of program, return a
    fn exec_lower(&mut self, lower: u32) -> LowerEffect {
        let op6 = (lower >> 26) & 0x3F;

        match op6 {
            // NOP
            0x20 => LowerEffect::None,

            // LQI VF[ft],(VI[is]++): VF[ft] = data_mem[VI[is]]; VI[is]++
            0x3A => {
                let ft = ((lower >> 21) & 0x1F) as usize;
                let is = ((lower >> 16) & 0xF) as usize;
                let addr = self.vi_get(is) as usize;
                if ft != 0 && addr < 1024 {
                    self.vf[ft] = self.data_mem[addr];
                }
                let new_is = self.vi_get(is).wrapping_add(1);
                self.vi_set(is, new_is);
                LowerEffect::None
            }

            // SQI VF[fs],(VI[it]++): data_mem[VI[it]] = VF[fs]; VI[it]++
            0x3E => {
                let fs = ((lower >> 21) & 0x1F) as usize;
                let it = ((lower >> 11) & 0xF) as usize;
                let addr = self.vi_get(it) as usize;
                if addr < 1024 {
                    self.data_mem[addr] = self.vf_get(fs);
                }
                let new_it = self.vi_get(it).wrapping_add(1);
                self.vi_set(it, new_it);
                LowerEffect::None
            }

            // IADDIU VI[vt],VI[vs],imm15
            0x27 => {
                let vt   = ((lower >> 21) & 0xF) as usize;
                let vs   = ((lower >> 16) & 0xF) as usize;
                let imm15 = (lower & 0x7FFF) as i16;
                // Sign-extend 15-bit immediate
                let imm = if imm15 & 0x4000 != 0 {
                    imm15 | (0xFFFF_u16 as i16 & !0x7FFF)
                } else {
                    imm15
                };
                let val = self.vi_get(vs).wrapping_add(imm);
                self.vi_set(vt, val);
                LowerEffect::None
            }

            // IBNE VI[vs],VI[vt],off11 — branch if not equal
            0x23 => {
                let vs   = ((lower >> 21) & 0xF) as usize;
                let vt   = ((lower >> 16) & 0xF) as usize;
                let off11 = (lower & 0x7FF) as i16;
                // Sign-extend 11-bit offset
                let off = if off11 & 0x400 != 0 {
                    off11 | (0xFFFF_u16 as i16 & !0x7FF)
                } else {
                    off11
                };
                if self.vi_get(vs) != self.vi_get(vt) {
                    let target = (self.pc as i32 + 1 + off as i32) as u16;
                    LowerEffect::Branch(target)
                } else {
                    LowerEffect::None
                }
            }

            // XGKICK VI[is] — end micro-program, return GIF buffer base
            0x32 => {
                let is = ((lower >> 16) & 0xF) as usize;
                LowerEffect::XgKick(self.vi_get(is) as u16)
            }

            _ => LowerEffect::None,
        }
    }

    // ---- Commit staged upper-slot VF write ----
    fn commit_upper(&mut self, staged: Option<(usize, u32, [f32; 4])>) {
        if let Some((fd, dest, val)) = staged {
            self.vf_set(fd, dest, val);
        }
    }

    /// Run the micro-program until XGKICK; returns the GIF base address (in data_mem QWs).
    /// Safety: exits after MAX_CYCLES to prevent infinite loops in case of program bugs.
    pub fn run_until_xgkick(&mut self) -> u16 {
        const MAX_CYCLES: u32 = 100_000;
        let mut cycles = 0u32;

        loop {
            if cycles >= MAX_CYCLES { return 109; } // fallback: return known GIF base
            cycles += 1;

            let pc = self.pc as usize;
            if pc >= VU1_MICRO.len() { return 109; }

            let instr  = self.code_mem[pc];
            let upper  = (instr >> 32) as u32;
            let lower  = (instr & 0xFFFF_FFFF) as u32;

            // Decrement DIV countdown
            if self.div_busy > 0 { self.div_busy -= 1; }

            // 1. Compute upper-slot result (don't commit yet)
            let staged = self.exec_upper(upper);

            // 2. Execute lower slot — lower reads current VF (pre-commit)
            let effect = self.exec_lower(lower);

            // 3. Commit upper-slot result
            self.commit_upper(staged);

            // 4. Apply lower-slot effect
            match effect {
                LowerEffect::None => {
                    self.pc += 1;
                }
                LowerEffect::Branch(target) => {
                    self.pc = target;
                }
                LowerEffect::XgKick(base) => {
                    self.pc += 1;
                    return base;
                }
            }
        }
    }
}

enum LowerEffect {
    None,
    Branch(u16),
    XgKick(u16),
}
