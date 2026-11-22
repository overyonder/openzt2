use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[component(immutable)]
pub struct PersistentId(pub u64);

#[derive(Component, Debug, PartialEq, Eq)]
pub struct PersistentIdAllocator {
    next: u64,
}

impl Default for PersistentIdAllocator {
    fn default() -> Self {
        Self { next: 1 }
    }
}

impl PersistentIdAllocator {
    pub const fn from_snapshot(next: u64) -> Result<Self, PersistentIdError> {
        if next == 0 {
            Err(PersistentIdError::Zero)
        } else {
            Ok(Self { next })
        }
    }

    pub fn allocate(&mut self) -> Result<PersistentId, PersistentIdError> {
        let following = self
            .next
            .checked_add(1)
            .ok_or(PersistentIdError::Exhausted)?;
        let id = PersistentId(self.next);
        self.next = following;
        Ok(id)
    }

    pub fn reserve_imported(&mut self, id: PersistentId) -> Result<(), PersistentIdError> {
        if id.0 == 0 {
            return Err(PersistentIdError::Zero);
        }
        let following = id.0.checked_add(1).ok_or(PersistentIdError::Exhausted)?;
        self.next = self.next.max(following);
        Ok(())
    }

    pub const fn next(&self) -> u64 {
        self.next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistentIdError {
    WrongWorld,
    Zero,
    Exhausted,
}
