//! Fitting-surface eligibility, sampling, composition, and terrain placement.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::plugins::locomotion::locomotion_types::NavAgent;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::terrain::terrain_edit_types::TerrainChanged;
use crate::plugins::terrain::terrain_world_sampling::sample_terrain;
use crate::plugins::terrain::terrain_world_sampling::terrain_chunk_at;

use super::fitting_surface_types::TerrainFittingSurface;

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

/// Fits only authored non-agent bodies to the loaded terrain surface. Avian
/// remains the sole owner of collision and integration; this is a changed-only
/// placement policy applied before its transform preparation.
#[allow(clippy::type_complexity)]
pub(super) fn fit_eligible_static_objects_to_changed_terrain_surfaces(
    terrain_index: Option<Res<TerrainIndex>>,
    terrain_assets: Option<Res<Assets<TerrainAsset>>>,
    mut terrain_changed: MessageReader<TerrainChanged>,
    terrain_chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>)>,
    mut objects: Query<
        (
            Ref<TerrainFittingSurface>,
            Option<&RigidBody>,
            &mut Transform,
        ),
        Without<NavAgent>,
    >,
) {
    let (Some(terrain_index), Some(terrain_assets)) = (terrain_index, terrain_assets) else {
        return;
    };
    let changed_regions = terrain_changed
        .read()
        .filter_map(|change| {
            let (_, chunk, _) = terrain_chunks.get(change.chunk).ok()?;
            Some((
                chunk.origin + change.min.as_vec2() * chunk.spacing_m,
                chunk.origin + change.max.as_vec2() * chunk.spacing_m,
            ))
        })
        .collect::<Vec<_>>();
    for (surface, body, mut transform) in &mut objects {
        let world_xz = transform.translation.xz();
        if !transform.is_changed()
            && !surface.is_changed()
            && !changed_regions
                .iter()
                .any(|(min, max)| world_xz.cmpge(*min).all() && world_xz.cmple(*max).all())
        {
            continue;
        }
        if body.is_some_and(|body| *body == RigidBody::Dynamic) {
            continue;
        }
        let Some(chunk_entity) = terrain_chunk_at(&terrain_index, world_xz) else {
            continue;
        };
        let Ok((_, chunk, edited)) = terrain_chunks.get(chunk_entity) else {
            continue;
        };
        let Some(asset) = terrain_assets.get(&chunk.asset) else {
            continue;
        };
        let Some(point) = sample_terrain(chunk, asset, edited, world_xz) else {
            continue;
        };
        if fitting_surface_accepts_sampled_normal(&surface, point.normal) {
            transform.translation.y = point.height_m;
        }
    }
}
