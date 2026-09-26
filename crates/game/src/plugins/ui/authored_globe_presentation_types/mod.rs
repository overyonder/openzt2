use bevy::prelude::*;
use openzt2_game_data::AssetId;

/// Model positions and interaction settings for the map-selection globe.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct UiGlobePresentation {
    pub(crate) primary_model: AssetId,
    pub(crate) primary_translation: Vec3,
    pub(crate) clouds_model: AssetId,
    pub(crate) clouds_translation: Vec3,
    pub(crate) dot_model: AssetId,
    pub(crate) dot_translation: Vec3,
    pub(crate) selected_dot_model: AssetId,
    pub(crate) selected_dot_translation: Vec3,
    pub(crate) pointer_model: AssetId,
    pub(crate) selection_rotate_speed: f32,
    pub(crate) mouse_increment: f32,
    pub(crate) mouse_down_friction: f32,
    pub(crate) mouse_up_friction: f32,
    pub(crate) friction_transition_seconds: f32,
    pub(crate) move_seconds: f32,
    pub(crate) scream_threshold: f32,
    pub(crate) scream_delay_seconds: f32,
    pub(crate) dot_highlight_rgba: [u8; 4],
    pub(crate) dot_highlight_cursor: AssetId,
}

/// Globe models loaded with the UI document.
#[derive(Component, Debug, Clone, Default)]
pub(crate) struct UiGlobeSceneAssets {
    pub(crate) primary: Option<Handle<crate::assets::scene_prefab::ScenePrefabAsset>>,
    pub(crate) clouds: Option<Handle<crate::assets::scene_prefab::ScenePrefabAsset>>,
    pub(crate) dot: Option<Handle<crate::assets::scene_prefab::ScenePrefabAsset>>,
    pub(crate) selected_dot: Option<Handle<crate::assets::scene_prefab::ScenePrefabAsset>>,
}

#[derive(Component, Debug, Clone)]
pub(crate) struct UiGlobeBiomeAsset(
    pub(crate) Handle<crate::assets::scene_prefab::ScenePrefabAsset>,
);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiGlobeBiomeModel {
    pub(crate) biome: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UiGlobeMarkerVisual {
    pub(crate) selected: bool,
}

/// Map location whose children show its normal and selected marker models.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiGlobeMarkerAnchor;

/// Pointer capture state for a gesture which began on the globe background.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UiGlobeDrag {
    pub(crate) active: bool,
}

#[derive(Component)]
pub(super) struct GlobeSelectionTurn {
    pub(super) from_yaw: f32,
    pub(super) from_pitch: f32,
    pub(super) to_yaw: f32,
    pub(super) to_pitch: f32,
    pub(super) elapsed_seconds: f32,
    pub(super) duration_seconds: f32,
}

#[derive(Component)]
pub(super) struct GlobeSelectedMarker(pub(super) Entity);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiGlobeMarkerProjected;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiGlobeModelsHydrated;

#[derive(Component, Debug, Clone)]
pub(super) struct UiGlobePrefab {
    pub(super) handle: Handle<crate::assets::scene_prefab::ScenePrefabAsset>,
    pub(super) primary: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct UiGlobePrimaryRenderable;

/// Rendered globe scene displayed inside the UI.
#[derive(Component, Debug, Clone)]
pub(super) struct UiGlobeRenderScene {
    pub(super) world: Entity,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiGlobeSceneOwner(pub(super) Entity);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct UiGlobeCamera {
    pub(super) model: Entity,
}
