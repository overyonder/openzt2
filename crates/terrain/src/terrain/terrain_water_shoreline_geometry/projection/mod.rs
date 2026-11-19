use std::collections::BTreeMap;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionQuery,
    TerrainWaterRegionSource, TerrainWaterRegionView,
};
use openzt2_zoo_state::terrain::terrain_water_shoreline_geometry_types::TerrainShorelinePatch;
use openzt2_zoo_state::terrain::terrain_water_shoreline_presentation_types::TerrainShorelineDraws;

use crate::terrain::water_regions::authored_membership::TerrainWaterMembershipChanged;

use super::mesh_construction::construct_shoreline_mesh_groups;
use super::source_geometry::construct_source_region_shoreline_patches;
use super::ConstructedTerrainShorelineMeshGroup;

/// Derived meshes awaiting material dependencies, on their canonical row owner.
#[derive(Component)]
pub(crate) struct PreparedTerrainShorelineMeshGroups {
    pub(crate) groups: Vec<(Entity, usize, ConstructedTerrainShorelineMeshGroup)>,
    pub(crate) slots: Vec<(Entity, usize)>,
}

#[derive(Component)]
pub(crate) struct TerrainShorelineGeometryProjected;

pub(crate) fn project_changed_terrain_shoreline_geometry(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    graphics_settings: Res<GraphicsSettings>,
    indexes: Query<&TerrainWaterMembershipIndex>,
    mut membership: MessageReader<TerrainWaterMembershipChanged>,
    mut asset_events: MessageReader<AssetEvent<TerrainAsset>>,
    regions: TerrainWaterRegionQuery,
    changed_regions: Query<
        (),
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
        )>,
    >,
    mut projection_states: Query<
        (
            Entity,
            Option<&TerrainShorelineGeometryProjected>,
            Option<&mut PreparedTerrainShorelineMeshGroups>,
        ),
        Or<(
            With<TerrainShorelineGeometryProjected>,
            With<PreparedTerrainShorelineMeshGroups>,
        )>,
    >,
    chunks: Query<(Entity, Ref<TerrainChunk>)>,
    surfaces: Query<&TerrainShorelineDraws>,
) {
    let membership: Vec<_> = membership.read().collect();
    let changed_assets: Vec<_> = asset_events.read().collect();
    for (owner, _, _) in &projection_states {
        if regions.get(owner).is_none() {
            discard_row_meshes(&mut commands, owner, &surfaces);
            commands.entity(owner).remove::<(
                TerrainShorelineGeometryProjected,
                PreparedTerrainShorelineMeshGroups,
            )>();
        }
    }
    for (owner, source) in regions.iter() {
        let (_, projected, pending) = projection_states
            .get_mut(owner)
            .unwrap_or((owner, None, None));
        let index = indexes
            .iter()
            .find(|index| index.terrain() == source.terrain().id());
        let Some(index) = index else {
            if projected.is_some() || pending.is_some() {
                discard_row_meshes(&mut commands, owner, &surfaces);
                commands.entity(owner).remove::<(
                    TerrainShorelineGeometryProjected,
                    PreparedTerrainShorelineMeshGroups,
                )>();
            }
            continue;
        };
        let chunks_changed = chunks
            .iter()
            .any(|(_, chunk)| chunk.asset.id() == source.terrain().id() && chunk.is_changed());
        if projected.is_some()
            && !changed_regions.contains(owner)
            && !membership
                .iter()
                .any(|change| change.affects_region(source.terrain().id(), owner))
            && !changed_assets.iter().any(|event| {
                event.is_added(source.terrain().id())
                    || event.is_modified(source.terrain().id())
                    || event.is_loaded_with_dependencies(source.terrain().id())
            })
            && !graphics_settings.is_changed()
            && !chunks_changed
        {
            continue;
        }
        let Some(asset) = assets.get(source.terrain()) else {
            continue;
        };
        let Some(groups) = construct_chunk_groups(asset, index, owner, &source, &chunks) else {
            continue;
        };
        let slots = groups
            .iter()
            .map(|(parent, slot, _)| (*parent, *slot))
            .collect();
        let next = PreparedTerrainShorelineMeshGroups { groups, slots };
        if let Some(mut current) = pending {
            *current = next;
        } else {
            commands.entity(owner).insert(next);
        }
        if projected.is_none() {
            commands
                .entity(owner)
                .insert(TerrainShorelineGeometryProjected);
        }
    }
}

fn discard_row_meshes(
    commands: &mut Commands,
    owner: Entity,
    surfaces: &Query<&TerrainShorelineDraws>,
) {
    if let Ok(draws) = surfaces.get(owner) {
        for draw in draws.iter() {
            commands.entity(draw).try_despawn();
        }
    }
}

fn construct_chunk_groups(
    asset: &TerrainAsset,
    membership: &TerrainWaterMembershipIndex,
    owner: Entity,
    source: &TerrainWaterRegionView<'_>,
    chunks: &Query<(Entity, Ref<TerrainChunk>)>,
) -> Option<Vec<(Entity, usize, ConstructedTerrainShorelineMeshGroup)>> {
    let cells_per_chunk = u32::from(asset.terrain_chunk_cells_per_side()?);
    if cells_per_chunk == 0 || cells_per_chunk % 4 != 0 {
        return None;
    }
    let patches = construct_source_region_shoreline_patches(asset, membership, owner, source)?;
    let mut by_chunk = BTreeMap::<(u32, u32), Vec<TerrainShorelinePatch>>::new();
    for (tile, patch) in patches {
        let coord = tile * 4 / cells_per_chunk;
        by_chunk.entry((coord.y, coord.x)).or_default().push(patch);
    }
    let grid = asset.canonical_terrain_grid();
    let biome = source.biome(asset)?;
    let biome_index = u8::try_from(
        grid.biomes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, biome))?,
    )
    .ok()?;
    let plane = source.surface_height(asset)?;
    let mut groups = Vec::new();
    for ((y, x), patches) in by_chunk {
        let (parent, _) = chunks.iter().find(|(_, chunk)| {
            chunk.asset.id() == source.terrain().id()
                && chunk.source_coord == UVec2::new(x, y).as_ivec2()
        })?;
        groups.extend(
            construct_shoreline_mesh_groups(patches, biome_index, plane)
                .into_iter()
                .enumerate()
                .map(|(slot, group)| (parent, slot, group)),
        );
    }
    Some(groups)
}
