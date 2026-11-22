//! Authored real-physics body crossings projected into water-wave impacts.

use avian3d::prelude::LinearVelocity;
use avian3d::prelude::RigidBody;
use avian3d::prelude::RigidBodyDisabled;
use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{TerrainWaterMembershipIndex, TerrainWaterRegionQuery};
use openzt2_zoo_state::terrain::terrain_water_geometric_wave_shader_state::{ActivateTerrainWaterImpactWave, WaterSurfaceSpraySample, WaterSurfaceWakeSample};
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};

use openzt2_game_data::world_definitions::world_objects::WorldObjectRealPhysicsWaterImpactDefinition;

const NATIVE_MINIMUM_WATER_IMPACT_STRENGTH: f32 = 0.100_000_001_49;

#[derive(Component)]
pub(super) struct WaterImpactBody {
    policy: WorldObjectRealPhysicsWaterImpactDefinition,
    previous_bottom: Option<Vec3>,
    previous_surface_height: Option<f32>,
}

pub(super) fn bind_authored_water_impact_bodies(
    mut commands: Commands,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active: Res<WorldDefinitions>,
    bodies: Query<(Entity, &DefinitionId, &RigidBody)>,
    changed: Query<
        (Entity, &DefinitionId, &RigidBody),
        Or<(Changed<DefinitionId>, Changed<RigidBody>)>,
    >,
    mut removed_definitions: RemovedComponents<DefinitionId>,
    mut removed_bodies: RemovedComponents<RigidBody>,
    mut bindings: Query<&mut WaterImpactBody>,
) {
    for entity in removed_definitions.read().chain(removed_bodies.read()) {
        if !bodies.contains(entity) {
            commands.entity(entity).try_remove::<WaterImpactBody>();
        }
    }
    let Some(definitions) = active.get(&definition_assets) else {
        return;
    };
    let candidates = active
        .is_changed()
        .then(|| bodies.iter())
        .into_iter()
        .flatten()
        .chain(
            (!active.is_changed())
                .then(|| changed.iter())
                .into_iter()
                .flatten(),
        );
    for (entity, definition, body) in candidates {
        let policy = (*body == RigidBody::Dynamic)
            .then(|| definitions.find_object(definition.0))
            .flatten()
            .and_then(|object| object.real_physics_water_impact)
            .filter(|policy| {
                policy.shape_radius_m.is_finite()
                    && policy.shape_radius_m >= 0.0
                    && policy.maximum_splash_speed_mps > 0.0
                    && policy.maximum_splash_speed_mps.is_finite()
                    && policy.maximum_splash_strength.is_finite()
            });
        match (policy, bindings.get_mut(entity)) {
            (Some(policy), Ok(mut binding)) if binding.policy != policy => {
                binding.policy = policy;
                binding.previous_bottom = None;
                binding.previous_surface_height = None;
            }
            (Some(policy), Err(_)) => {
                commands.entity(entity).insert(WaterImpactBody {
                    policy,
                    previous_bottom: None,
                    previous_surface_height: None,
                });
            }
            (None, Ok(_)) => {
                commands.entity(entity).remove::<WaterImpactBody>();
            }
            _ => {}
        }
    }
}

/// Emits a wave when a moving body crosses the water surface.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_authored_body_water_surface_crossings_into_impact_waves(
    terrain_index: Option<Res<TerrainIndex>>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    memberships: Query<&TerrainWaterMembershipIndex>,
    regions: TerrainWaterRegionQuery,
    chunk_worlds: Query<&WorldMember>,
    mut bodies: Query<
        (
            &Transform,
            &LinearVelocity,
            &WorldMember,
            &mut WaterImpactBody,
        ),
        Without<RigidBodyDisabled>,
    >,
    mut impacts: MessageWriter<ActivateTerrainWaterImpactWave>,
    mut spray: MessageWriter<WaterSurfaceSpraySample>,
    mut wakes: MessageWriter<WaterSurfaceWakeSample>,
) {
    let Some(terrain_index) = terrain_index else {
        return;
    };
    let world =
        water_membership_chunk(&terrain_index).and_then(|chunk| chunk_worlds.get(chunk).ok());
    for (transform, velocity, member, mut crossing) in &mut bodies {
        if !velocity.0.is_finite() || world != Some(member) {
            continue;
        }
        let authored_impact = crossing.policy;
        let current_body_bottom = transform.translation - Vec3::Y * authored_impact.shape_radius_m;
        let current_water_sample = sample_accepted_water_surface(
            &terrain_index,
            &terrain_assets,
            &memberships,
            &regions,
            current_body_bottom.xz(),
        );
        let current_water_height = current_water_sample.map(|(height, _)| height);
        let previous_body_bottom = crossing.previous_bottom.replace(current_body_bottom);
        let previous_water_height =
            std::mem::replace(&mut crossing.previous_surface_height, current_water_height);

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

pub(crate) fn water_membership_chunk(index: &TerrainIndex) -> Option<Entity> {
    // The first authored sector (0, 0) is at chunk (0, -1) in world XZ.
    index.chunks.get(&IVec2::new(0, -1)).copied()
}

pub(crate) fn sample_accepted_water_surface(
    terrain_index: &TerrainIndex,
    terrain_assets: &Assets<TerrainAsset>,
    memberships: &Query<&TerrainWaterMembershipIndex>,
    regions: &TerrainWaterRegionQuery,
    horizontal_position: Vec2,
) -> Option<(f32, Entity)> {
    terrain_chunk_at(terrain_index, horizontal_position)?;
    let membership = memberships
        .get(water_membership_chunk(terrain_index)?)
        .ok()?;
    let terrain = terrain_assets.get(membership.terrain())?;
    let region = membership.owner_at_world_xz(horizontal_position, terrain)??;
    let source = regions.get(region)?;
    (source.terrain().id() == membership.terrain())
        .then(|| source.surface_height(terrain))?
        .map(|height| (height, region))
}
