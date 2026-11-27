use bevy::prelude::*;

/// Root and visibility choice for the main-menu mod controls.
#[derive(Component, Default)]
pub(super) struct ModManagerOverlayRoot {
    pub(super) is_open: bool,
}

#[derive(Component)]
pub(super) struct ModManagerOverlayButton {
    pub(super) overlay: Entity,
    pub(super) open: bool,
}

/// Full-canvas input owner shown while one archive change is settling.
#[derive(Component)]
pub(super) struct ModManagerReloadInputBlocker;

/// Index into the asset source's priority-ordered archive status collection.
#[derive(Component)]
pub struct AssetArchiveEnabledStateCheckbox {
    pub(super) archive_status_index: usize,
}

#[derive(Component)]
pub(super) struct ReloadArchiveAtSamePath(pub(super) usize);
