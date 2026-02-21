// vif1.rs — VIF1 (VPU Interface 1) packet parser.
// Maps to: PS2 VIF1 unpacking VIF1 DMA packets into VU1 data memory.
// Implements: STCYCL, UNPACK V4-32, MSCAL, FLUSH tags.

use std::collections::VecDeque;

pub struct Vif1 {
    pub fifo:          VecDeque<u128>,
    cl:                u8,
    wl:                u8,
    unpack_active:     bool,
    unpack_addr:       u16,   // VU datamem destination (in QWs)
    unpack_count:      u16,   // remaining QWs to write
    pub mscal_addr:    Option<u16>,
}

impl Vif1 {
    pub fn new() -> Self {
        Vif1 {
            fifo:          VecDeque::new(),
            cl:            1,
            wl:            1,
            unpack_active: false,
            unpack_addr:   0,
            unpack_count:  0,
            mscal_addr:    None,
        }
    }

    /// Drain the FIFO, parse VIF tags, write unpacked data into VU1 data memory.
    pub fn process(&mut self, vu_mem: &mut [[f32; 4]; 1024]) {
        while let Some(qw) = self.fifo.pop_front() {
            if self.unpack_active {
                // This QW is data for the active UNPACK.
                // V4-32: four f32 values, little-endian.
                let bytes = qw.to_le_bytes();
                let x = f32::from_le_bytes(bytes[0..4].try_into().unwrap());
                let y = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
                let z = f32::from_le_bytes(bytes[8..12].try_into().unwrap());
                let w = f32::from_le_bytes(bytes[12..16].try_into().unwrap());

                if (self.unpack_addr as usize) < 1024 {
                    vu_mem[self.unpack_addr as usize] = [x, y, z, w];
                }
                self.unpack_addr  = self.unpack_addr.wrapping_add(1);
                self.unpack_count -= 1;

                if self.unpack_count == 0 {
                    self.unpack_active = false;
                }
            } else {
                // Parse VIF tag from the low 32 bits of the QW.
                let tag = (qw & 0xFFFF_FFFF) as u32;
                let cmd = (tag >> 24) & 0xFF;

                match cmd {
                    0x01 => {
                        // STCYCL: bits [15:8] = wl, bits [7:0] = cl
                        self.wl = ((tag >> 8) & 0xFF) as u8;
                        self.cl = (tag & 0xFF) as u8;
                    }
                    0x6C => {
                        // UNPACK V4-32
                        // bits [23:16] = NUM (number of QWs to write)
                        // bits [9:0]   = ADDR (VU datamem destination in QWs)
                        let num  = ((tag >> 16) & 0xFF) as u16;
                        let addr = (tag & 0x3FF) as u16;
                        if num > 0 {
                            self.unpack_active = true;
                            self.unpack_addr   = addr;
                            self.unpack_count  = num;
                        }
                    }
                    0x14 => {
                        // MSCAL: start VU micro-program at exec_addr
                        // bits [15:0] = execaddr
                        let exec_addr = (tag & 0xFFFF) as u16;
                        self.mscal_addr = Some(exec_addr);
                    }
                    0x11 => {
                        // FLUSH: wait for VIF/VU to finish — we're synchronous, no-op
                    }
                    _ => {
                        // Unknown/NOP — ignore
                    }
                }
            }
        }
    }
}
