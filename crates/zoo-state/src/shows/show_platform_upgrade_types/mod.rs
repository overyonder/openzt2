use bevy::prelude::*;

/// The two authored upgrade positions available on a show platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowPlatformUpgradeKind {
    Canopy,
    Television,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectedShowPlatformUpgrade(pub ShowPlatformUpgradeKind);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstalledShowPlatformUpgrade(pub ShowPlatformUpgradeKind);

/// Gameplay ownership relation for an installed upgrade whose visual parent
/// may be a named socket entity beneath the stage prefab.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstalledShowPlatformUpgradeStage(pub Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestoreShowPlatformUpgrade {
    pub stage: Entity,
    pub kind: ShowPlatformUpgradeKind,
    pub persistent_id: openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
}

/// A saved upgrade waiting for its stage, prefab and socket to load.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingShowPlatformUpgradeRestore {
    pub stage: Entity,
    pub kind: ShowPlatformUpgradeKind,
    pub persistent_id: openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
}

/// The direction requested when the construction transaction is first
/// committed. Undo and redo derive the effective direction from this operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowPlatformUpgradeEditOperation {
    Install,
    Remove,
}

/// A reversible platform upgrade stored on a construction transaction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowPlatformUpgradeEdit {
    pub stage: Entity,
    pub kind: ShowPlatformUpgradeKind,
    pub operation: ShowPlatformUpgradeEditOperation,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitShowPlatformUpgradeEdit {
    pub transaction: Entity,
    pub application: crate::construction::construction_transaction_types::EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShowPlatformUpgradeEditAcknowledged {
    pub transaction: Entity,
    pub application: crate::construction::construction_transaction_types::EditApplication,
    pub accepted: bool,
}
