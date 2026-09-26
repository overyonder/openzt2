use avian3d::{
    parry::{shape::SharedShape, utils::Array2},
    prelude::{Collider, RigidBody},
};
use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;

use super::{
    terrain_change_tracking_types::TerrainDirty,
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_collision_rebuilding_types::{
        TerrainChunkCollisionSurface, TerrainCollisionBudget, TerrainCollisionPending,
        TerrainCollisionRevision,
    },
    terrain_sample_grid_queries::read_terrain_sample_height_centimetres,
};

pub(super) fn rebuild_pending_terrain_chunk_collisions_within_tick_budget(
    mut commands: Commands,
    budget: Res<TerrainCollisionBudget>,
    assets: Res<Assets<TerrainAsset>>,
    mut chunks: Query<
        (
            Entity,
            &TerrainChunk,
            Option<&EditedTerrainSamples>,
            Option<&TerrainDirty>,
            Option<&mut TerrainCollisionRevision>,
            Option<&RigidBody>,
            Option<&Children>,
        ),
        With<TerrainCollisionPending>,
    >,
    mut collision_surfaces: Query<&mut Collider, With<TerrainChunkCollisionSurface>>,
) {
    let mut rebuilt = 0;
    for (entity, chunk, edited, dirty, applied, body, children) in &mut chunks {
        if rebuilt >= budget.max_rebuilds_per_tick {
            break;
        }
        let collision_surface = children.and_then(|children| {
            children
                .iter()
                .find(|child| collision_surfaces.contains(*child))
        });
        let target = dirty.map_or(0, |dirty| dirty.revision);
        if applied
            .as_ref()
            .is_some_and(|applied| applied.0 >= target && collision_surface.is_some())
        {
            commands.entity(entity).remove::<TerrainCollisionPending>();
            continue;
        }
        let Some(asset) = assets.get(&chunk.asset) else {
            continue;
        };
        let side = chunk.side as usize;
        let heights = (0..side)
            .flat_map(|x| (0..side).map(move |z| (x, z)))
            .map(|(x, z)| {
                read_terrain_sample_height_centimetres(chunk, asset, edited, x, z)
                    .map(|height| f32::from(height) * 0.01)
            })
            .collect::<Option<Vec<_>>>();
        let Some(heights) = heights else { continue };
        let span = chunk.spacing_m * f32::from(chunk.side - 1);
        let next = Collider::from(SharedShape::heightfield(
            Array2::new(side, side, heights),
            Vec3::new(span, 1.0, span),
        ));
        if let Some(collision_surface) = collision_surface {
            let mut collider = collision_surfaces
                .get_mut(collision_surface)
                .expect("the terrain collision child was queried above");
            *collider = next;
        } else {
            commands.spawn((
                TerrainChunkCollisionSurface,
                ChildOf(entity),
                Transform::from_xyz(span * 0.5, 0.0, span * 0.5),
                next,
            ));
        }
        if let Some(mut applied) = applied {
            applied.0 = target;
        } else {
            commands
                .entity(entity)
                .insert(TerrainCollisionRevision(target));
        }
        if body.is_none() {
            commands.entity(entity).insert(RigidBody::Static);
        }
        commands.entity(entity).remove::<Collider>();
        commands.entity(entity).remove::<TerrainCollisionPending>();
        rebuilt += 1;
    }
}
