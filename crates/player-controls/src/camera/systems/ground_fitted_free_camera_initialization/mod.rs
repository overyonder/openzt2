use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;

use super::super::camera_runtime_state_types::CameraDefinition;
use super::super::camera_runtime_state_types::CameraMode;
use super::super::camera_runtime_state_types::OverheadRig;
use super::super::camera_runtime_state_types::ZooCamera;
use super::camera_ground_fit_surface_sampling::sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood;

pub(crate) fn initialize_ground_fitted_free_camera_from_overhead_focus(
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut cameras: Query<
        (&mut Transform, &CameraMode, &CameraDefinition, &OverheadRig),
        (With<ZooCamera>, Changed<CameraMode>),
    >,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (mut transform, mode, selected_definition, overhead_rig) in &mut cameras {
        if *mode != CameraMode::Free {
            continue;
        }
        let Some(definition) = world_definitions.find_camera(selected_definition.0) else {
            continue;
        };
        let Some(fitting_surface_height) = sample_highest_terrain_or_water_fitting_surface_height_in_three_by_three_metre_neighborhood(
            overhead_rig.focus,
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
        ) else {
            continue;
        };
        let yaw = (definition.yaw_radians[0] + definition.yaw_radians[1]) * 0.5;
        let pitch = definition.initial_pitch_radians;
        *transform = Transform::from_xyz(
            overhead_rig.focus.x,
            fitting_surface_height + definition.offset_m[1],
            overhead_rig.focus.y,
        )
        .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch));
    }
}
