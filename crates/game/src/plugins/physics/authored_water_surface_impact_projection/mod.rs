//! Authored real-physics body crossings projected into water-wave impacts.

use avian3d::prelude::{LinearVelocity, RigidBody};
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use crate::plugins::model_render::authored_effect_technique_pass_submission_order::AuthoredEffectTechniquePassSubmissionOrder;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_water_renderer_types::AuthoredTerrainWaterSurfaceEffectPass;
use crate::plugins::terrain::terrain_water_geometric_wave_shader_state::ActivateTerrainWaterImpactWave;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::fitting_surface_types::AuthoredWaterSurfaceCrossingProjectionState;

const NATIVE_MINIMUM_WATER_IMPACT_STRENGTH: f32 = 0.100_000_001_49;

/// Emits a wave when a moving body crosses the water surface.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_authored_body_water_surface_crossings_into_impact_waves(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_index: Option<Res<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    water_surface_passes: Query<(
        Entity,
        &AuthoredTerrainWaterSurfaceEffectPass,
        &AuthoredEffectTechniquePassSubmissionOrder,
    )>,
    mut bodies: Query<(
        Entity,
        &DefinitionId,
        &RigidBody,
        &Transform,
        &LinearVelocity,
        Option<&mut AuthoredWaterSurfaceCrossingProjectionState>,
    )>,
    mut impacts: MessageWriter<ActivateTerrainWaterImpactWave>,
) {
    let (Some(definitions), Some(terrain_index)) =
        (active_definitions.get(&definition_assets), terrain_index)
    else {
        return;
    };
    for (entity, definition, body, transform, velocity, crossing_state) in &mut bodies {
        if *body != RigidBody::Dynamic || !velocity.0.is_finite() {
            continue;
        }
        let Some(authored_impact) = authored_water_impact_definition(definitions, definition)
        else {
            continue;
        };
        let current_body_bottom = transform.translation - Vec3::Y * authored_impact.shape_radius_m;
        let current_water_height = sample_water_height(
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
            current_body_bottom.xz(),
        );
        let Some(mut crossing_state) = crossing_state else {
            commands
                .entity(entity)
                .insert(AuthoredWaterSurfaceCrossingProjectionState {
                    previous_body_bottom_world_position: Some(current_body_bottom),
                    previous_water_surface_height: current_water_height,
                });
            continue;
        };
        let previous_body_bottom = crossing_state.previous_body_bottom_world_position;
        let previous_water_height = crossing_state.previous_water_surface_height;
        crossing_state.previous_body_bottom_world_position = Some(current_body_bottom);
        crossing_state.previous_water_surface_height = current_water_height;

        let Some((previous_body_bottom, water_height)) =
            previous_body_bottom.zip(current_water_height)
        else {
            continue;
        };
        let was_above_water = previous_water_height.map_or(
            previous_body_bottom.y > water_height,
            |previous_water_height| previous_body_bottom.y > previous_water_height,
        );
        if !was_above_water
            || current_body_bottom.y > water_height
            || velocity.0.y >= 0.0
            || authored_impact.maximum_splash_speed_mps <= 0.0
        {
            continue;
        }
        let native_strength = authored_impact.maximum_splash_strength
            * (-velocity.0.y).min(authored_impact.maximum_splash_speed_mps)
            / authored_impact.maximum_splash_speed_mps;
        if !native_strength.is_finite() || native_strength <= NATIVE_MINIMUM_WATER_IMPACT_STRENGTH {
            continue;
        }
        let vertical_displacement = current_body_bottom.y - previous_body_bottom.y;
        let crossing_fraction = if vertical_displacement.abs() > f32::EPSILON {
            ((water_height - previous_body_bottom.y) / vertical_displacement).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let crossing_position = previous_body_bottom.lerp(current_body_bottom, crossing_fraction);
        for (surface, bounds, pass_order) in &water_surface_passes {
            if pass_order.is_first_authored_pass()
                && (bounds.surface_plane_height - water_height).abs() <= 0.01
                && crossing_position.x >= bounds.horizontal_minimum.x
                && crossing_position.x <= bounds.horizontal_maximum.x
                && crossing_position.z >= bounds.horizontal_minimum.y
                && crossing_position.z <= bounds.horizontal_maximum.y
            {
                impacts.write(ActivateTerrainWaterImpactWave {
                    surface,
                    source_position: crossing_position.xz(),
                    native_strength,
                });
            }
        }
    }
}

fn authored_water_impact_definition<'a>(
    definitions: WorldDefinitionsView<'a>,
    definition: &DefinitionId,
) -> Option<
    &'a openzt2_game_data::world_definitions::world_objects::WorldObjectRealPhysicsWaterImpactDefinition,
>{
    definitions
        .find_object(definition.0)
        .and_then(|object| object.real_physics_water_impact.as_ref())
}

fn sample_water_height(
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    horizontal_position: Vec2,
) -> Option<f32> {
    let chunk_entity = terrain_chunk_at(terrain_index, horizontal_position)?;
    let (chunk, edited) = terrain_chunks.get(chunk_entity).ok()?;
    let asset = terrain_assets.get(&chunk.asset)?;
    sample_terrain(chunk, asset, edited, horizontal_position)?.water_height_m
}
