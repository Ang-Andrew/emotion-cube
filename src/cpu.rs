// Emotion Engine (MIPS R5900) stub.
// Maps to: the EE CPU running the PS2 game binary.
// We fake 300,000 cycles per frame (≈ 300 MHz / 1000 frames-per-second budget).

pub struct EmotionEngine {
    pub cycles: u64,
}

impl EmotionEngine {
    pub fn new() -> Self {
        EmotionEngine { cycles: 0 }
    }

    /// Simulate one frame's worth of EE execution.
    /// Returns the number of cycles stepped this frame.
    pub fn step(&mut self) -> u64 {
        self.cycles += 300_000;
        300_000
    }
}
