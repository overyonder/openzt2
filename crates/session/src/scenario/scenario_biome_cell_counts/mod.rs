use std::{cmp::Reverse, collections::BTreeMap};

use bevy::{
    ecs::{entity::EntityHashSet, system::SystemParam},
    prelude::*,
};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_sample_grid_queries::{
        calculate_row_major_terrain_sample_index, read_authored_terrain_sample,
    },
};

const SAMPLES_PER_BIOME_CELL: usize = 4;
const BIOME_CELL_CENTRE: usize = SAMPLES_PER_BIOME_CELL / 2;

#[derive(Component, Default, PartialEq, Eq)]
struct TerrainChunkBiomeCellCounts(BTreeMap<AssetId, u64>);

#[derive(Component, Default, PartialEq, Eq)]
pub(super) struct WorldBiomeCellCounts(BTreeMap<AssetId, u64>);

impl WorldBiomeCellCounts {
    pub(super) fn count(&self, key: &str) -> u64 {
        self.0
            .get(&AssetId::from_key(&key.to_ascii_lowercase()))
            .copied()
            .unwrap_or_default()
    }
}

pub(super) fn register(app: &mut App) {
    app.register_required_components::<TerrainChunk, TerrainChunkBiomeCellCounts>()
        .register_required_components::<WorldRoot, WorldBiomeCellCounts>()
        .add_systems(
            Update,
            update_world_biome_counts
                .after(openzt2_world_state::application_schedule::GameSet::Intent)
                .before(openzt2_interface_state::ui::UiSet::DomainProjection),
        );
}

#[derive(SystemParam)]
struct TerrainBiomeCountInputs<'w, 's> {
    chunks: Query<
        'w,
        's,
        (
            Entity,
            &'static WorldMember,
            &'static TerrainChunk,
            Option<&'static EditedTerrainSamples>,
            &'static mut TerrainChunkBiomeCellCounts,
        ),
    >,
    changed: Query<
        'w,
        's,
        Entity,
        Or<(
            Changed<TerrainChunk>,
            Changed<EditedTerrainSamples>,
            Changed<WorldMember>,
        )>,
    >,
    removed_edits: RemovedComponents<'w, 's, EditedTerrainSamples>,
    terrain_events: MessageReader<'w, 's, AssetEvent<TerrainAsset>>,
    terrain_assets: Res<'w, Assets<TerrainAsset>>,
}

fn update_world_biome_counts(
    mut inputs: TerrainBiomeCountInputs,
    mut worlds: Query<(&TerrainIndex, &mut WorldBiomeCellCounts)>,
    changed_indexes: Query<Entity, Changed<TerrainIndex>>,
) {
    let changed_assets: std::collections::HashSet<_> = inputs
        .terrain_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Added { id } | AssetEvent::Modified { id } | AssetEvent::Removed { id } => {
                Some(*id)
            }
            _ => None,
        })
        .collect();
    let mut dirty_chunks: EntityHashSet = inputs.changed.iter().collect();
    dirty_chunks.extend(inputs.removed_edits.read());
    if !changed_assets.is_empty() {
        dirty_chunks.extend(inputs.chunks.iter().filter_map(|(entity, _, chunk, _, _)| {
            changed_assets.contains(&chunk.asset.id()).then_some(entity)
        }));
    }

    let mut dirty_roots: EntityHashSet = changed_indexes.iter().collect();
    for entity in dirty_chunks {
        let Ok((_, member, chunk, edited, mut counts)) = inputs.chunks.get_mut(entity) else {
            continue;
        };
        let next = inputs
            .terrain_assets
            .get(&chunk.asset)
            .map_or_else(TerrainChunkBiomeCellCounts::default, |asset| {
                count_authored_biome_cells_in_chunk(chunk, asset, edited)
            });
        if counts.set_if_neq(next) {
            dirty_roots.insert(member.root);
        }
    }
    for root in dirty_roots {
        let Ok((index, mut totals)) = worlds.get_mut(root) else {
            continue;
        };
        let mut next = WorldBiomeCellCounts::default();
        for (_, _, _, _, cells) in inputs
            .chunks
            .iter_many(index.chunks.values().copied())
            .flatten()
        {
            for (&biome, &count) in &cells.0 {
                *next.0.entry(biome).or_default() += count;
            }
        }
        totals.set_if_neq(next);
    }
}

fn count_authored_biome_cells_in_chunk(
    chunk: &TerrainChunk,
    asset: &TerrainAsset,
    edited: Option<&EditedTerrainSamples>,
) -> TerrainChunkBiomeCellCounts {
    let cells = usize::from(chunk.side.saturating_sub(1));
    let [Ok(sector_x), Ok(sector_y)] = chunk.source_coord.to_array().map(usize::try_from) else {
        return TerrainChunkBiomeCellCounts::default();
    };
    let first_centre = |sector: usize| {
        (BIOME_CELL_CENTRE + SAMPLES_PER_BIOME_CELL - sector * cells % SAMPLES_PER_BIOME_CELL)
            % SAMPLES_PER_BIOME_CELL
    };
    let biome_keys: Vec<_> = asset
        .canonical_terrain_grid()
        .biomes
        .iter()
        .map(|biome| AssetId::from_key(&biome.name.to_ascii_lowercase()))
        .collect();
    let mut counts = TerrainChunkBiomeCellCounts::default();
    for source_y in (first_centre(sector_y)..cells).step_by(SAMPLES_PER_BIOME_CELL) {
        // Local sample rows run in the opposite direction to the source rows.
        let z = cells - source_y;
        for x in (first_centre(sector_x)..cells).step_by(SAMPLES_PER_BIOME_CELL) {
            let index = if let Some(weights) = edited.and_then(|edit| {
                calculate_row_major_terrain_sample_index(chunk.side, x, z)
                    .and_then(|sample| edit.blend_weights.get(sample * 16..(sample + 1) * 16))
            }) {
                weights
                    .iter()
                    .take(biome_keys.len())
                    .enumerate()
                    .max_by_key(|(index, weight)| (**weight, Reverse(*index)))
                    .map(|(index, _)| index)
            } else {
                read_authored_terrain_sample(chunk, asset, x, z)
                    .map(|sample| usize::from(sample.biome_index))
            };
            if let Some(biome) = index.and_then(|index| biome_keys.get(index)) {
                *counts.0.entry(*biome).or_default() += 1;
            }
        }
    }
    counts
}
