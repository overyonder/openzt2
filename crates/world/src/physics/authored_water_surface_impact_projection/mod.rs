//! Authored real-physics body crossings projected into water-wave impacts.

use avian3d::prelude::LinearVelocity;
use avian3d::prelude::RigidBody;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery};
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{ActivateTerrainWaterImpactWave, WaterSurfaceSpraySample, WaterSurfaceWakeSample};
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;

use openzt2_world_state::physics::fitting_surface_types::AuthoredWaterSurfaceCrossingProjectionState;

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
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    mut bodies: Query<(
        Entity,
        &DefinitionId,
        &RigidBody,
        &Transform,
        &LinearVelocity,
        Option<&mut AuthoredWaterSurfaceCrossingProjectionState>,
    )>,
    mut impacts: MessageWriter<ActivateTerrainWaterImpactWave>,
    mut spray: MessageWriter<WaterSurfaceSpraySample>,
    mut wakes: MessageWriter<WaterSurfaceWakeSample>,
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
        let current_water_sample = sample_accepted_water_surface(
            &terrain_index,
            &terrain_assets,
            &terrain_chunks,
            &memberships,
            &regions,
            current_body_bottom.xz(),
        );
        let current_water_height = current_water_sample.map(|(height, _)| height);
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
        let Some((_, surface)) = current_water_sample else {
            continue;
        };
        let source_position = Vec2::new(crossing_position.x, -crossing_position.z);
        spray.write(WaterSurfaceSpraySample {
            surface,
            source_position,
            direction: Vec3::Y,
            normal: Vec3::Y,
            native_strength,
        });
        wakes.write(WaterSurfaceWakeSample {
            surface,
            source_position,
            direction: Vec2::new(velocity.0.x, -velocity.0.z),
            native_strength,
        });
        impacts.write(ActivateTerrainWaterImpactWave {
            surface,
            source_position,
            native_strength,
        });
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

pub(super) fn sample_accepted_water_surface(
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    terrain_chunks: &Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    memberships: &Query<&TerrainWaterMembershipIndex>,
    regions: &TerrainWaterRegionQuery,
    horizontal_position: Vec2,
) -> Option<(f32, Entity)> {
    let chunk_entity = terrain_chunk_at(terrain_index, horizontal_position)?;
    let (chunk, _) = terrain_chunks.get(chunk_entity).ok()?;
    let asset = terrain_assets.get(&chunk.asset)?;
    let membership = memberships
        .iter()
        .find(|membership| membership.terrain() == chunk.asset.id())?;
    let region = membership.owner_at_world_xz(horizontal_position, asset)??;
    Some((regions.get(region)?.surface_height(asset)?, region))
}
