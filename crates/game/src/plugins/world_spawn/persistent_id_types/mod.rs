use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PersistentId(pub(crate) u64);

#[derive(Resource, Debug, PartialEq, Eq)]
pub(crate) struct PersistentIdAllocator {
    root: Entity,
    next: u64,
}

impl PersistentIdAllocator {
    pub(crate) const fn new(root: Entity) -> Self {
        Self { root, next: 1 }
    }

    pub(crate) fn owns_world(&self, root: Entity) -> bool {
        self.root == root
    }

    pub(crate) fn allocate(&mut self, root: Entity) -> Result<PersistentId, PersistentIdError> {
        if root != self.root {
            return Err(PersistentIdError::WrongWorld);
        }
        let following = self
            .next
            .checked_add(1)
            .ok_or(PersistentIdError::Exhausted)?;
        let id = PersistentId(self.next);
        self.next = following;
        Ok(id)
    }

    pub(crate) fn reserve_imported(
        &mut self,
        root: Entity,
        id: PersistentId,
    ) -> Result<(), PersistentIdError> {
        if root != self.root {
            return Err(PersistentIdError::WrongWorld);
        }
        if id.0 == 0 {
            return Err(PersistentIdError::Zero);
        }
        let following = id.0.checked_add(1).ok_or(PersistentIdError::Exhausted)?;
        self.next = self.next.max(following);
        Ok(())
    }

    pub(crate) const fn next(&self) -> u64 {
        self.next
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistentIdError {
    WrongWorld,
    Zero,
    Exhausted,
}
