use std::time::Duration;

/// Retains scaled clock time below the integer clip/event clock's resolution.
#[derive(Clone, Copy, Debug, Default)]
pub struct FractionalAnimationMilliseconds {
    remainder: f64,
}

impl FractionalAnimationMilliseconds {
    pub fn clear(&mut self) {
        self.remainder = 0.0;
    }

    pub fn advance(&mut self, delta: Duration, rate: f64) -> u32 {
        if !rate.is_finite() || rate <= 0.0 {
            return 0;
        }
        let milliseconds = delta.as_secs_f64() * 1000.0 * rate + self.remainder;
        if milliseconds >= f64::from(u32::MAX) {
            self.remainder = 0.0;
            return u32::MAX;
        }
        let whole = milliseconds.floor();
        self.remainder = milliseconds - whole;
        whole as u32
    }
}
