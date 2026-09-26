use bevy::prelude::*;

/// The two authored upgrade positions available on a show platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShowPlatformUpgradeKind {
    Canopy,
    Television,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectedShowPlatformUpgrade(pub(crate) ShowPlatformUpgradeKind);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstalledShowPlatformUpgrade(pub(crate) ShowPlatformUpgradeKind);

/// Gameplay ownership relation for an installed upgrade whose visual parent
/// may be a named socket entity beneath the stage prefab.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InstalledShowPlatformUpgradeStage(pub(crate) Entity);

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RestoreShowPlatformUpgrade {
    pub(crate) stage: Entity,
    pub(crate) kind: ShowPlatformUpgradeKind,
    pub(crate) persistent_id: crate::plugins::world_spawn::persistent_id_types::PersistentId,
}

/// A saved upgrade waiting for its stage, prefab and socket to load.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingShowPlatformUpgradeRestore {
    pub(crate) stage: Entity,
    pub(crate) kind: ShowPlatformUpgradeKind,
    pub(crate) persistent_id: crate::plugins::world_spawn::persistent_id_types::PersistentId,
}

/// The direction requested when the construction transaction is first
/// committed. Undo and redo derive the effective direction from this operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShowPlatformUpgradeEditOperation {
    Install,
    Remove,
}

/// A reversible platform upgrade stored on a construction transaction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowPlatformUpgradeEdit {
    pub(crate) stage: Entity,
    pub(crate) kind: ShowPlatformUpgradeKind,
    pub(crate) operation: ShowPlatformUpgradeEditOperation,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CommitShowPlatformUpgradeEdit {
    pub(crate) transaction: Entity,
    pub(crate) application:
        crate::plugins::construction::construction_transaction_types::EditApplication,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShowPlatformUpgradeEditAcknowledged {
    pub(crate) transaction: Entity,
    pub(crate) application:
        crate::plugins::construction::construction_transaction_types::EditApplication,
    pub(crate) accepted: bool,
}
