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
    pub(crate) handle: Handle<crate::assets::scene_prefab::ScenePrefabAsset>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InteractionCursorVisual {
    pub(crate) owner: Entity,
}

#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct InteractionPrefabVisual {
    pub(crate) owner: Entity,
    pub(crate) prefab: InteractionPrefab,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RestoreSimulationPause(pub(crate) bool);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModeHiddenHud {
    pub(crate) previous: Visibility,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PauseSimulationWhileActive;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HideHudWhileActive;
