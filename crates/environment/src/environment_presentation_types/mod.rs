use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentSky {
    pub(super) definition: AssetId,
}

/// Texture and material for an environmental visual.
#[derive(Component, Clone, Debug)]
pub(super) struct EnvironmentTexture {
    pub(super) image: Handle<Image>,
    pub(super) material: Option<Handle<StandardMaterial>>,
}

/// Marks an environmental model visual.
#[derive(Component, Clone, Debug)]
pub(super) struct EnvironmentModelVisual;

/// Root of an authored sky presentation which remains centred on the active
/// camera while retaining its own world orientation.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(super) struct EnvironmentCameraRelativeVisual {
    pub(super) scale_to_camera_far_plane: bool,
    pub(super) projected_scale: f32,
    pub(super) source_radius_m: Option<f32>,
}

impl EnvironmentCameraRelativeVisual {
    pub(super) fn sky_layer(projected_scale: f32) -> Self {
        Self {
            scale_to_camera_far_plane: true,
            projected_scale,
            source_radius_m: None,
        }
    }

    pub(super) fn sun_position_rig() -> Self {
        Self {
            scale_to_camera_far_plane: false,
            projected_scale: 1.0,
            source_radius_m: None,
        }
    }
}

/// Index of a timed layer in the environment's visual table.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EnvironmentVisualSampleIndex(pub(super) u32);

/// Links a timed visual to the Bevy environment entity that owns it. Visuals
/// may be parented to an authored model joint, so hierarchy depth cannot stand
/// in for this relationship.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = EnvironmentVisuals)]
pub(super) struct EnvironmentVisualOwner(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = EnvironmentVisualOwner, linked_spawn)]
pub(super) struct EnvironmentVisuals(Vec<Entity>);

/// One authored sun body awaiting its named joint in a shared position rig.
#[derive(Component)]
#[relationship(relationship_target = PendingSunRigVisuals)]
pub(super) struct EnvironmentSunPosition {
    #[relationship]
    pub(super) rig: Entity,
    pub(super) node: AssetId,
}

#[derive(Component)]
#[relationship_target(relationship = EnvironmentSunPosition)]
pub(super) struct PendingSunRigVisuals(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = EnvironmentFogCameras)]
pub(super) struct EnvironmentFog(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = EnvironmentFog)]
pub(super) struct EnvironmentFogCameras(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = WorldEnvironments)]
pub(super) struct EnvironmentOfWorld(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = EnvironmentOfWorld, linked_spawn)]
pub(super) struct WorldEnvironments(Vec<Entity>);

#[derive(Component)]
pub(super) struct EnvironmentRendererReady;

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct AboveWaterFog {
    pub(super) color: Color,
    pub(super) start: f32,
    pub(super) end: f32,
    pub(super) relative_to_far_plane: bool,
}

impl Default for AboveWaterFog {
    fn default() -> Self {
        Self {
            color: bevy::pbr::DistanceFog::default().color,
            start: 1_000_000.0,
            end: 1_000_001.0,
            relative_to_far_plane: false,
        }
    }
}

/// One-frame marker ensuring freshly hydrated renderer components receive the
/// current sample even when the clock has not advanced.
#[derive(Component, Debug, Clone, Copy, Default)]
pub(super) struct EnvironmentPresentationPending;
