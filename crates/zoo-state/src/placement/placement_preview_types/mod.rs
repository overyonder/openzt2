use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;

/// A selected placed object being repositioned by the ordinary object-placement
/// preview and transaction path. The marker retains only the source entity relationship.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelocatingPlacedObject;

/// Requested authored eighth-turn applied by the object-placement relocation
/// transaction rather than by mutating the placed transform outside history.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementRotationRequest(pub i8);

/// A non-construction interaction asking placement to present and validate one
/// authored object. The requesting domain owns the eventual transaction.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementPreviewRequest(pub AssetId);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectPlacementPreviewPermissionFacts {
    pub unlocked: bool,
    pub affordable: bool,
    pub topology_valid: bool,
    pub headroom_valid: bool,
    pub prefab_ready: bool,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ObjectPlacementPreviewPrefabHydrated;

#[derive(Component, Debug, Clone, Copy)]
pub struct ObjectPlacementPreviewMotion {
    pub current: Vec3,
    pub start: Vec3,
    pub target: Vec3,
    pub progress: f32,
    pub progress_per_second: f32,
}

impl ObjectPlacementPreviewMotion {
    pub fn progress_per_second(weight: f32) -> f32 {
        const BASE_INTERPOLATION_RATE: f32 = 17.0;
        if weight > 0.0 {
            BASE_INTERPOLATION_RATE / weight
        } else {
            BASE_INTERPOLATION_RATE
        }
    }

    pub fn advance(&mut self, delta_seconds: f32) {
        self.progress = (self.progress + self.progress_per_second * delta_seconds).min(1.0);
        self.current = self.start.lerp(self.target, self.progress);
    }
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ObjectPlacementPreviewModelPresentation;

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ObjectPlacementPreviewRenderable;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectPlacementPreviewOwner(pub Entity);

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ObjectPlacementPreviewDecal;

#[derive(Component, Debug, Clone)]
pub struct ObjectPlacementPreviewMaterials {
    pub footprint_valid: Handle<StandardMaterial>,
    pub footprint_invalid: Handle<StandardMaterial>,
    pub grid_cardinal: [Handle<StandardMaterial>; 3],
    pub grid_diagonal: [Handle<StandardMaterial>; 3],
    pub grid_radius: [f32; 3],
    pub model_valid: Color,
    pub model_invalid: Color,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct ObjectPlacementPreviewVisualState {
    pub eighth_turns: u8,
    pub valid: bool,
    pub footprint_translations: Vec<Vec3>,
    pub decal_rotation: Quat,
    pub footprint_material: Handle<StandardMaterial>,
    pub grid_material: Handle<StandardMaterial>,
    pub grid_radius: f32,
}

#[derive(Component, Debug, Clone)]
pub struct ObjectPlacementPrefabSource(pub Handle<ScenePrefabAsset>);

#[derive(Resource, Debug, Default)]
pub struct ObjectPlacementCellCollectionScratch {
    pub cells: Vec<IVec2>,
}
