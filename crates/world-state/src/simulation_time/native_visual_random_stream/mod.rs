use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;

/// Process-wide visual random draws, including both unit-interval endpoints.
#[derive(Resource, Debug)]
pub struct NativeVisualRandomStream {
    state: u32,
}

impl Default for NativeVisualRandomStream {
    fn default() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(u32::MAX, |time| time.as_secs() as u32);
        Self::from_seed(seed)
    }
}

impl NativeVisualRandomStream {
    pub const fn from_seed(seed: u32) -> Self {
        Self { state: seed }
    }

    pub fn reseed(&mut self, seed: u32) {
        self.state = seed;
    }

    pub fn next_u15(&mut self) -> u16 {
        self.state = self.state.wrapping_mul(214_013).wrapping_add(2_531_011);
        ((self.state >> 16) & 0x7fff) as u16
    }

    pub fn unit_f64(&mut self) -> f64 {
        f64::from(self.next_u15()) / 32_767.0
    }

    pub fn unit_f32(&mut self) -> f32 {
        f32::from(self.next_u15()) / 32_767.0
    }
}

#[cfg(test)]
mod tests {
    use super::NativeVisualRandomStream;

    #[test]
    fn seeded_stream_matches_the_crt_sequence() {
        let mut random = NativeVisualRandomStream::from_seed(1);
        assert_eq!(
            std::array::from_fn::<_, 5, _>(|_| random.next_u15()),
            [41, 18_467, 6334, 26_500, 19_169]
        );
    }
}
