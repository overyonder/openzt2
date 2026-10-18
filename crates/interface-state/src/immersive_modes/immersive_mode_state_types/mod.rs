use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImmersiveMode {
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
pub struct ActiveImmersiveMode {
    pub mode: ImmersiveMode,
    pub subject: Option<Entity>,
    pub camera: Entity,
    pub restore_camera_on_exit: bool,
}

/// The animal-training child activity of a first-person controller.
#[derive(Component, Debug, Clone, Copy)]
pub struct FirstPersonTraining {
    pub subject: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlledEntity {
    pub controller: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquippedInteractionTool {
    pub policy: AssetId,
}
