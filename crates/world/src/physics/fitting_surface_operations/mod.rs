//! Fitting-surface eligibility, sampling, composition, and terrain placement.

use avian3d::prelude::*;
use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::locomotion::locomotion_types::NavAgent;
use openzt2_zoo_state::terrain::dry_surface_eligibility::DrySurfaceEligibility;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;

use super::interaction_types::FallingReleasedObject;
use openzt2_world_state::physics::fitting_surface_types::TerrainFittingSurface;

/// Tests an authored fitting surface against one finite sampled normal.
pub(super) fn fitting_surface_accepts_sampled_normal(
    surface: &TerrainFittingSurface,
    normal: Vec3,
) -> bool {
    surface.normal_tolerance.is_finite()
        && (0.0..=1.0).contains(&surface.normal_tolerance)
        && normal.is_finite()
        && normal.length_squared() > f32::EPSILON
        && normal.normalize().dot(Vec3::Y) >= surface.normal_tolerance
}

#[derive(Component)]
#[relationship(relationship_target = TerrainChunkFittings)]
pub(super) struct FittingInTerrainChunk(Entity);

#[derive(Component)]
#[relationship_target(relationship = FittingInTerrainChunk)]
pub(super) struct TerrainChunkFittings(Vec<Entity>);

pub(super) type ChangedTerrainFitting = (
    With<TerrainFittingSurface>,
    Without<NavAgent>,
    Without<FallingReleasedObject>,
    Or<(
        Changed<Transform>,
        Changed<TerrainFittingSurface>,
        Changed<RigidBody>,
    )>,
);

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn fit_objects_with_changed_inputs_or_terrain(
    mut commands: Commands,
    indexes: Query<Ref<TerrainIndex>>,
    terrain_assets: Option<Res<Assets<TerrainAsset>>>,
    mut terrain_changed: MessageReader<TerrainChanged>,
    terrain_chunks: Query<(
        &TerrainChunk,
        &WorldMember,
        Option<&EditedTerrainSamples>,
        Option<&TerrainChunkFittings>,
    )>,
    water: Option<DrySurfaceEligibility>,
    agents_added: Query<Entity, (Added<NavAgent>, With<FittingInTerrainChunk>)>,
    mut agents_removed: RemovedComponents<NavAgent>,
    mut surfaces_removed: RemovedComponents<TerrainFittingSurface>,
    mut fittings: ParamSet<(
        Query<Entity, ChangedTerrainFitting>,
        Query<
            (
                Entity,
                &TerrainFittingSurface,
                &WorldMember,
                Option<&RigidBody>,
                &mut Transform,
                Option<&FittingInTerrainChunk>,
            ),
            (Without<NavAgent>, Without<FallingReleasedObject>),
        >,
    )>,
) {
    let Some(assets) = terrain_assets else {
        return;
    };
    for entity in agents_added.iter().chain(surfaces_removed.read()) {
        commands
            .entity(entity)
            .try_remove::<FittingInTerrainChunk>();
    }
    let mut candidates: bevy::ecs::entity::EntityHashSet = fittings.p0().iter().collect();
    let mut objects = fittings.p1();
    if indexes.iter().any(|index| index.is_changed()) {
        candidates.extend(objects.iter().filter_map(|(entity, _, member, ..)| {
            indexes
                .get(member.root)
                .is_ok_and(|index| index.is_changed())
                .then_some(entity)
        }));
    }
    candidates.extend(agents_removed.read());
    for edit in terrain_changed.read() {
        let Ok((chunk, member, _, _)) = terrain_chunks.get(edit.chunk) else {
            continue;
        };
        let Ok(index) = indexes.get(member.root) else {
            continue;
        };
        let min = chunk.origin + edit.min.as_vec2() * chunk.spacing_m;
        let max = chunk.origin + edit.max.as_vec2() * chunk.spacing_m;
        if index.chunk_span_m <= 0.0 {
            continue;
        }
        let min_chunk = ((min - index.origin) / index.chunk_span_m)
            .floor()
            .as_ivec2();
        let max_chunk = ((max - index.origin) / index.chunk_span_m)
            .floor()
            .as_ivec2();
        // Edit rectangles include samples shared by adjacent chunks.
        for z in min_chunk.y..=max_chunk.y {
            for x in min_chunk.x..=max_chunk.x {
                let Some(&chunk) = index.chunks.get(&IVec2::new(x, z)) else {
                    continue;
                };
                let Ok((_, _, _, Some(members))) = terrain_chunks.get(chunk) else {
                    continue;
                };
                use bevy::ecs::relationship::RelationshipTarget;
                for entity in members.iter() {
                    if let Ok((_, _, _, _, transform, _)) = objects.get(entity) {
                        let position = transform.translation.xz();
                        if position.cmpge(min).all() && position.cmple(max).all() {
                            candidates.insert(entity);
                        }
                    }
                }
            }
        }
    }
    for entity in candidates {
        let Ok((_, surface, member, body, mut transform, previous_chunk)) = objects.get_mut(entity)
        else {
            continue;
        };
        let position = transform.translation.xz();
        let chunk = if body == Some(&RigidBody::Dynamic) {
            None
        } else {
            indexes
                .get(member.root)
                .ok()
                .and_then(|index| terrain_chunk_at(&index, position))
        };
        if previous_chunk.map(|binding| binding.0) != chunk {
            if let Some(chunk) = chunk {
                commands.entity(entity).insert(FittingInTerrainChunk(chunk));
            } else {
                commands.entity(entity).remove::<FittingInTerrainChunk>();
            }
        }
        let Some(chunk) = chunk else {
            continue;
        };
        let Ok((chunk, _, edited, _)) = terrain_chunks.get(chunk) else {
            continue;
        };
        let Some(point) = assets
            .get(&chunk.asset)
            .and_then(|asset| sample_terrain(chunk, asset, edited, position))
        else {
            continue;
        };
        if !fitting_surface_accepts_sampled_normal(surface, point.normal) {
            continue;
        }
        let ground = point.height_m + surface.height_offset_m;
        // Water level changes alone do not refit settled placements.
        let height = surface
            .water_offset_m
            .zip(
                water
                    .as_ref()
                    .and_then(|water| water.water_surface_height(member.root, position)),
            )
            .map_or(ground, |(offset, water_height)| {
                ground.max(water_height + offset)
            });
        if transform.translation.y != height {
            transform.translation.y = height;
        }
    }
}
