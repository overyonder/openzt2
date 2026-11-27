use std::cmp::Ordering;

/// A finite dollar value. Economy arithmetic rounds each movement to f32.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[repr(transparent)]
pub struct Money(f32);

impl Eq for Money {}

impl Ord for Money {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.partial_cmp(&other.0).expect("money is finite")
    }
}

impl PartialOrd for Money {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Money {
    pub const ZERO: Self = Self(0.0);

    pub fn from_dollars(amount: f32) -> Option<Self> {
        amount.is_finite().then_some(Self(amount))
    }

    /// Reads a price from an authored integer-cent field.
    pub fn from_cents(cents: i64) -> Self {
        Self((cents as f64 * 0.01) as f32)
    }

    pub const fn dollars(self) -> f32 {
        self.0
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Self::from_dollars(self.0 + other.0)
    }

    pub fn checked_sub(self, other: Self) -> Option<Self> {
        Self::from_dollars(self.0 - other.0)
    }

    pub fn checked_scale(self, factor: f32) -> Option<Self> {
        Self::from_dollars(self.0 * factor)
    }

    pub fn saturating_add(self, other: Self) -> Self {
        Self((self.0 + other.0).clamp(-f32::MAX, f32::MAX))
    }

    pub fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0.0
    }
}

impl std::ops::Neg for Money {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}
