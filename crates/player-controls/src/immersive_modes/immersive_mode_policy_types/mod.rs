use bevy::prelude::*;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeActionFlags;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AllowedInteractionActions {
    pub(crate) allowed_actions: ImmersiveModeActionFlags,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct InteractionCursor {
    pub(crate) image: Handle<Image>,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct InteractionPrefab {
    pub(crate) definition: AssetId,
    pub(crate) handle: Handle<openzt2_game_assets::scene_prefab::ScenePrefabAsset>,
}

#[derive(Component)]
#[relationship(relationship_target = InteractionCursorVisuals)]
pub(super) struct InteractionCursorVisual(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = InteractionCursorVisual, linked_spawn)]
pub(super) struct InteractionCursorVisuals(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = InteractionPrefabVisuals)]
pub(super) struct InteractionPrefabVisual(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = InteractionPrefabVisual, linked_spawn)]
pub(super) struct InteractionPrefabVisuals(Vec<Entity>);
