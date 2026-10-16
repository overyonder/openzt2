//! Authored timing of periodic animal and guest need adjustments.

#[derive(Clone, Copy, Debug, serde::Deserialize, Eq, PartialEq, serde::Serialize)]
pub struct NeedAdjustmentPolicy {
    pub enabled: bool,
    /// Exclusive upper bound for random delays, in 100 ms units.
    pub delay_steps: u32,
}
