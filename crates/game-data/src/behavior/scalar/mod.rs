//! Authored fixed or stepped random behavior scalars, sampled by the actor owner.

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum BehaviorScalarQ16 {
    FixedQ16(i32),
    PriceEffectQ16([i32; 3]),
    RandomQ16 {
        minimum: i32,
        maximum: i32,
        step: i32,
    },
}

impl BehaviorScalarQ16 {
    /// Samples an authored stepped scalar using the actor-owned random stream.
    /// The draw callback receives an exclusive upper bound.
    #[must_use]
    pub fn sample_q16(self, draw: impl FnOnce(u32) -> u32) -> Option<i32> {
        self.sample_with_price_index_q16(None, draw)
    }

    /// Contextual expressions require the selected target transaction's price band.
    #[must_use]
    pub fn sample_with_price_index_q16(
        self,
        price_index: Option<usize>,
        draw: impl FnOnce(u32) -> u32,
    ) -> Option<i32> {
        match self {
            Self::FixedQ16(value) => Some(value),
            Self::PriceEffectQ16(values) => values.get(price_index?).copied(),
            Self::RandomQ16 {
                minimum,
                maximum,
                step,
            } => {
                if step <= 0 || maximum < minimum {
                    return Some(minimum);
                }
                let count = (i64::from(maximum) - i64::from(minimum)) / i64::from(step) + 1;
                let count = u32::try_from(count).unwrap_or(u32::MAX);
                let offset = i64::from(draw(count).min(count - 1)) * i64::from(step);
                Some(i32::try_from(i64::from(minimum) + offset).unwrap_or(maximum))
            }
        }
    }
}
