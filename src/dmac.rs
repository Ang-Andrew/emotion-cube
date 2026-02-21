// dmac.rs — DMAC channel 1 (VIF1 path).
// Maps to: PS2 DMAC transferring QWs from EE RAM into VIF1 FIFO.
// Implements D1_MADR/D1_QWC/D1_CHCR register semantics.

use std::collections::VecDeque;

pub struct Dmac {
    pub d1_madr: u32,   // PS2 MMIO: 0x10009010 — DMA source address
    pub d1_qwc:  u32,   // PS2 MMIO: 0x10009020 — quadword count
    pub d1_chcr: u32,   // PS2 MMIO: 0x10009000 — channel control (bit 8 = STR)
}

impl Dmac {
    pub fn new() -> Self {
        Dmac { d1_madr: 0, d1_qwc: 0, d1_chcr: 0 }
    }

    /// Kick DMA channel 1: set MADR, QWC, and STR bit.
    pub fn kick(&mut self, madr: u32, qwc: u32) {
        self.d1_madr = madr;
        self.d1_qwc  = qwc;
        self.d1_chcr = 0x101; // STR=1, DIR=to-peripheral
    }

    /// Transfer all pending QWs from EE RAM into the VIF1 FIFO.
    /// Clears STR bit when done (DMA complete).
    pub fn transfer(&mut self, ram: &[u8], fifo: &mut VecDeque<u128>) {
        if self.d1_chcr & 0x100 == 0 {
            return; // STR not set
        }

        let base = self.d1_madr as usize;
        let qwc  = self.d1_qwc  as usize;
        let end  = base + qwc * 16;

        if end > ram.len() {
            self.d1_chcr &= !0x100;
            return;
        }

        for i in 0..qwc {
            let off = base + i * 16;
            let bytes: [u8; 16] = ram[off..off + 16].try_into().unwrap();
            // Little-endian u128
            let qw = u128::from_le_bytes(bytes);
            fifo.push_back(qw);
        }

        self.d1_chcr &= !0x100; // clear STR
    }
}
