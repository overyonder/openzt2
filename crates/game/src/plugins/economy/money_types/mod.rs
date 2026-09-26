#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct Money(pub(crate) i64);

impl Money {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn checked_add(self, other: Self) -> Option<Self> {
        match self.0.checked_add(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub(crate) const fn checked_sub(self, other: Self) -> Option<Self> {
        match self.0.checked_sub(other.0) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub(crate) const fn is_positive(self) -> bool {
        self.0 > 0
    }
}
