use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImmersiveMode {
    GuestView,
    FirstPerson,
    FossilSearch,
    FossilAssembly,
    Photo,
    ShowEdit,
    SuperStaff,
}

/// The active relationship lives on its controller entity. The subject and
/// camera are ordinary ECS entity links.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActiveImmersiveMode {
    pub(crate) mode: ImmersiveMode,
    pub(crate) subject: Option<Entity>,
    pub(crate) camera: Entity,
    pub(crate) restore_camera_on_exit: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingImmersiveEntry {
    pub(crate) mode: ImmersiveMode,
    pub(crate) subject: Option<Entity>,
    pub(crate) failure: Option<super::immersive_mode_message_types::ModeEntryFailure>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlledEntity {
    pub(crate) controller: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EquippedInteractionTool {
    pub(crate) policy: AssetId,
}
