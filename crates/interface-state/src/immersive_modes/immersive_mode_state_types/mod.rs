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

/// Policy for the admitted gameplay session.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveImmersiveMode {
    pub mode: ImmersiveMode,
    pub restore_camera_on_exit: bool,
}

/// The animal-training child activity of a first-person controller.
#[derive(Component, Debug, Clone, Copy)]
pub struct FirstPersonTraining {
    pub subject: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ImmersiveControlledSubjects)]
pub struct ControlledEntity {
    pub controller: Entity,
}

#[derive(Component)]
#[relationship_target(relationship = ControlledEntity)]
pub struct ImmersiveControlledSubjects(Vec<Entity>);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct EquippedInteractionTool {
    pub policy: AssetId,
}

/// Requests overhead-only UI to leave layout while the mode is active.
#[derive(Component)]
pub struct ImmersiveHudSuppression;

#[derive(Component)]
#[relationship(relationship_target = ImmersiveSubjectSessions)]
pub struct ImmersiveSubject(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ImmersiveSubject)]
pub struct ImmersiveSubjectSessions(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = ImmersiveCameraSessions)]
pub struct ImmersiveCamera(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = ImmersiveCamera)]
pub struct ImmersiveCameraSessions(Vec<Entity>);
