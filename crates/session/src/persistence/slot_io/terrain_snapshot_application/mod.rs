use std::collections::{BTreeMap, HashSet};

use bevy::prelude::*;
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk},
    terrain_collision_rebuilding_types::TerrainCollisionPending,
    terrain_edit_data_types::TerrainWaterAffiliation,
    terrain_edit_types::TerrainWaterChanged,
    terrain_water_presentation_types::{
        TerrainWaterMembershipIndex, TerrainWaterRegionEdit, TerrainWaterRegionRetired,
        TerrainWaterRegionSource,
    },
};

use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;
use super::terrain_snapshot_section_encoding::validate_terrain_snapshot_record;
use super::terrain_snapshot_types::TerrainSnapshotRecord;

pub(super) struct TerrainSnapshotApplication {
    pub changed_chunks: Vec<TerrainWaterChanged>,
}

pub(super) fn apply_terrain_snapshot_record<'a>(
    record: &TerrainSnapshotRecord,
    terrain: &Handle<TerrainAsset>,
    world_root: Entity,
    chunks: impl Iterator<Item = (Entity, &'a TerrainChunk)>,
    existing_sources: impl Iterator<Item = (Entity, &'a TerrainWaterRegionSource)>,
    commands: &mut Commands,
) -> Result<TerrainSnapshotApplication, Failure> {
    validate_terrain_snapshot_record(record)?;
    if commands.get_entity(world_root).is_err() {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    let grid = &record.grid;
    let expected_side = ((grid.width - 1) / grid.sector_columns + 1) as u16;
    let span = grid.units.cell_size_metres * f32::from(expected_side - 1);
    let mut chunk_by_coordinate = BTreeMap::new();
    let mut chunk_entities = HashSet::new();
    let mut terrain_origin = None;
    for (entity, chunk) in chunks {
        let coordinate = chunk.source_coord.to_array();
        if entity == world_root
            || chunk.asset.id() != terrain.id()
            || chunk.side != expected_side
            || coordinate[0] < 0
            || coordinate[1] < 0
            || coordinate[0] as u32 >= grid.sector_columns
            || coordinate[1] as u32 >= grid.sector_rows
            || !chunk.spacing_m.is_finite()
            || (chunk.spacing_m - grid.units.cell_size_metres).abs() > f32::EPSILON
            || !chunk.origin.is_finite()
            || !chunk_entities.insert(entity)
            || chunk_by_coordinate
                .insert(coordinate, (entity, chunk))
                .is_some()
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        if chunk.coord != IVec2::new(chunk.source_coord.x, -chunk.source_coord.y - 1)
            || commands.get_entity(entity).is_err()
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        let origin = chunk.origin - chunk.coord.as_vec2() * span;
        if !origin.is_finite()
            || terrain_origin
                .is_some_and(|expected: Vec2| (expected - origin).length_squared() > f32::EPSILON)
        {
            return Err(Failure::CorruptSnapshotSection);
        }
        terrain_origin = Some(origin);
    }
    if chunk_by_coordinate.len() as u64
        != u64::from(grid.sector_columns) * u64::from(grid.sector_rows)
    {
        return Err(Failure::SnapshotTransactionInFlight);
    }
    let mut water_region_owners = vec![None; grid.water_regions.len()];
    let mut owner_entities = HashSet::new();
    for (owner, source) in existing_sources {
        let row = source.source_region_index();
        if source.terrain().id() != terrain.id()
            || row >= water_region_owners.len()
            || owner == world_root
            || chunk_entities.contains(&owner)
            || !owner_entities.insert(owner)
            || water_region_owners[row].replace(owner).is_some()
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
        if commands.get_entity(owner).is_err() {
            return Err(Failure::BrokenPersistentEntityReference);
        }
    }
    for (row, region) in grid
        .water_regions
        .iter()
        .enumerate()
        .take(record.original_water_region_count as usize)
    {
        if !region.row_major_cell_indices.is_empty() && water_region_owners[row].is_none() {
            return Err(Failure::SnapshotTransactionInFlight);
        }
    }
    let retired_rows = record
        .retired_water_region_rows
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let exact_planes = record
        .edited_water_surface_heights
        .iter()
        .copied()
        .collect::<BTreeMap<_, _>>();
    let mut required_rows = exact_planes.keys().copied().collect::<HashSet<_>>();
    required_rows.extend(retired_rows.iter().copied());
    for chunk in &record.edited_chunks {
        for sample in &chunk.edited_water_owner_samples {
            let Some(TerrainWaterAffiliation::SourceRow(row)) =
                chunk.samples.water_affiliations[*sample as usize]
            else {
                return Err(Failure::BrokenPersistentEntityReference);
            };
            required_rows.insert(row);
        }
    }
    // No Commands are queued until every snapshot reference and projected chunk is checked.
    for (row, region) in grid.water_regions.iter().enumerate() {
        if water_region_owners[row].is_none()
            && (row >= record.original_water_region_count as usize
                || required_rows.contains(&(row as u32)))
        {
            water_region_owners[row] = Some(
                commands
                    .spawn((
                        TerrainWaterRegionSource::new(terrain.clone(), row),
                        ChildOf(world_root),
                    ))
                    .id(),
            );
        }
        let Some(owner) = water_region_owners[row] else {
            continue;
        };
        let mut entity = commands.entity(owner);
        if let Some(height) = exact_planes.get(&(row as u32)) {
            entity.insert(TerrainWaterRegionEdit {
                terrain: terrain.clone(),
                biome_index: region.water_style_index as usize,
                surface_height_metres: *height,
                depth_ratio: region.depth_ratio,
                uses_tank_minimum_depth: region.uses_tank_minimum_depth,
            });
        } else {
            entity.remove::<TerrainWaterRegionEdit>();
        }
        if retired_rows.contains(&(row as u32)) {
            entity.insert(TerrainWaterRegionRetired);
        } else {
            entity.remove::<TerrainWaterRegionRetired>();
        }
    }
    let overlays = record
        .edited_chunks
        .iter()
        .map(|chunk| (chunk.source_coord, chunk))
        .collect::<BTreeMap<_, _>>();
    let mut changed_chunks = Vec::with_capacity(chunk_by_coordinate.len());
    for (coordinate, (entity, chunk)) in chunk_by_coordinate {
        let mut target = commands.entity(entity);
        if let Some(overlay) = overlays.get(&coordinate) {
            let mut samples = overlay.samples.clone();
            for sample in &overlay.edited_water_owner_samples {
                let Some(TerrainWaterAffiliation::SourceRow(row)) =
                    samples.water_affiliations[*sample as usize]
                else {
                    unreachable!("validated source row");
                };
                let owner =
                    water_region_owners[row as usize].expect("required owner was allocated above");
                samples.water_affiliations[*sample as usize] =
                    Some(TerrainWaterAffiliation::Edited(owner));
            }
            target.insert(samples);
        } else {
            target.remove::<EditedTerrainSamples>();
        }
        target.remove::<TerrainWaterMembershipIndex>();
        target.insert(TerrainCollisionPending);
        changed_chunks.push(TerrainWaterChanged {
            chunk: entity,
            min: UVec2::ZERO,
            max: UVec2::splat(u32::from(chunk.side - 1)),
        });
    }
    Ok(TerrainSnapshotApplication {
        changed_chunks,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{
        terrain_snapshot_capture::capture_terrain_snapshot_record,
        terrain_snapshot_section_encoding::tests::painted_nonflat_water_snapshot,
    };
    use super::*;
    use bevy::ecs::{system::SystemState, world::CommandQueue};
    use openzt2_game_assets::terrain::terrain_test_asset::terrain_test_asset;
    use openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionQuery;

    #[test]
    fn commands_restore_water_owners_exact_overlays_and_authoritative_removal() {
        let mut record = painted_nonflat_water_snapshot();
        let terrain = Handle::default();
        let asset = terrain_test_asset(record.grid.clone());
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let chunk = TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::new(0, -1),
            asset: terrain.clone(),
            origin: Vec2::new(0.0, -8.0),
            spacing_m: 2.0,
            side: 5,
        };
        let chunk_entity = world.spawn((chunk.clone(), ChildOf(root))).id();
        let source = TerrainWaterRegionSource::new(terrain.clone(), 0);
        let source_owner = world
            .spawn((
                TerrainWaterRegionSource::new(terrain.clone(), 0),
                ChildOf(root),
            ))
            .id();
        let mut queue = CommandQueue::default();
        let application = apply_terrain_snapshot_record(
            &record,
            &terrain,
            root,
            std::iter::once((chunk_entity, &chunk)),
            std::iter::once((source_owner, &source)),
            &mut Commands::new(&mut queue, &world),
        )
        .unwrap();
        queue.apply(&mut world);
        let edited_owner = world
            .query::<(Entity, &TerrainWaterRegionSource)>()
            .iter(&world)
            .find_map(|(owner, source)| (source.source_region_index() == 1).then_some(owner))
            .unwrap();
        assert_eq!(world.get::<ChildOf>(edited_owner).unwrap().parent(), root);
        assert!(world
            .get::<TerrainWaterRegionRetired>(source_owner)
            .is_some());
        assert_eq!(
            world
                .get::<TerrainWaterRegionEdit>(edited_owner)
                .unwrap()
                .surface_height_metres,
            7.375
        );
        assert_eq!(
            application.changed_chunks,
            vec![TerrainWaterChanged {
                chunk: chunk_entity,
                min: UVec2::ZERO,
                max: UVec2::splat(4),
            }]
        );
        assert!(world.get::<TerrainCollisionPending>(chunk_entity).is_some());
        let restored = world.get::<EditedTerrainSamples>(chunk_entity).unwrap();
        assert_eq!(
            restored.blend_weights,
            record.edited_chunks[0].samples.blend_weights
        );
        assert_eq!(
            restored.water_affiliations[7],
            Some(TerrainWaterAffiliation::SourceRow(0))
        );
        assert_eq!(
            restored.water_affiliations[12],
            Some(TerrainWaterAffiliation::Unresolved)
        );
        assert_eq!(
            restored.water_affiliations[17],
            Some(TerrainWaterAffiliation::Edited(edited_owner))
        );
        {
            let mut state = SystemState::<(
                TerrainWaterRegionQuery,
                Query<(
                    Entity,
                    &TerrainWaterRegionSource,
                    Option<&TerrainWaterRegionRetired>,
                )>,
            )>::new(&mut world);
            let (regions, sources) = state.get(&world).unwrap();
            let recaptured = capture_terrain_snapshot_record(
                &terrain,
                &asset,
                std::iter::once((&chunk, world.get::<EditedTerrainSamples>(chunk_entity))),
                &regions,
                |owner| owner == source_owner || owner == edited_owner,
                sources
                    .iter()
                    .filter(|(_, _, retired)| retired.is_none())
                    .map(|(owner, source, _)| (owner, source)),
                sources
                    .iter()
                    .filter(|(_, _, retired)| retired.is_some())
                    .map(|(_, source, _)| source),
            )
            .unwrap();
            assert_eq!(recaptured.grid, record.grid);
            assert_eq!(recaptured.edited_chunks, record.edited_chunks);
            assert_eq!(
                recaptured.edited_water_surface_heights,
                record.edited_water_surface_heights
            );
            assert_eq!(
                recaptured.retired_water_region_rows,
                record.retired_water_region_rows
            );
        }
        record.edited_chunks.clear();
        let existing = [
            TerrainWaterRegionSource::new(terrain.clone(), 0),
            TerrainWaterRegionSource::new(terrain.clone(), 1),
        ];
        apply_terrain_snapshot_record(
            &record,
            &terrain,
            root,
            std::iter::once((chunk_entity, &chunk)),
            [(source_owner, &existing[0]), (edited_owner, &existing[1])].into_iter(),
            &mut Commands::new(&mut queue, &world),
        )
        .unwrap();
        queue.apply(&mut world);
        assert!(world.get::<EditedTerrainSamples>(chunk_entity).is_none());
    }

    #[test]
    fn invalid_projected_geometry_queues_no_partial_restoration() {
        let record = painted_nonflat_water_snapshot();
        let terrain = Handle::default();
        let mut world = World::new();
        let root = world.spawn_empty().id();
        let chunk = TerrainChunk {
            source_coord: IVec2::ZERO,
            coord: IVec2::new(0, -1),
            asset: terrain.clone(),
            origin: Vec2::new(0.0, -8.0),
            spacing_m: 2.0,
            side: 4,
        };
        let chunk_entity = world.spawn(chunk.clone()).id();
        let count = world.entities().len();
        let mut queue = CommandQueue::default();
        assert!(apply_terrain_snapshot_record(
            &record,
            &terrain,
            root,
            std::iter::once((chunk_entity, &chunk)),
            std::iter::empty(),
            &mut Commands::new(&mut queue, &world)
        )
        .is_err());
        queue.apply(&mut world);
        assert_eq!(world.entities().len(), count);
        assert!(world.get::<EditedTerrainSamples>(chunk_entity).is_none());
        assert!(world.get::<TerrainCollisionPending>(chunk_entity).is_none());
    }
}
