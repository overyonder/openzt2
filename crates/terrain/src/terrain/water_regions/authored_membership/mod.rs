use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackSegment;
use std::sync::Arc;

use bevy::ecs::system::SystemParam;
use bevy::{ecs::{entity_disabling::Disabled, query::Allow}, platform::collections::HashSet};
use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_zoo_state::terrain::terrain_chunk_types::{EditedTerrainSamples, TerrainChunk};
use openzt2_zoo_state::terrain::terrain_water_presentation_types::{
    unconstrained_terrain_water_height, TerrainWaterMembershipIndex, TerrainWaterRegionQuery, TerrainWaterRegionSource, TerrainWaterSupportLimits,
};
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;

mod height_support;
mod selector_projection;
mod source_rows;

use openzt2_zoo_state::{
    aquatic::aquatic_simulation_types::TankGeometry,
    habitat::habitat_types::HabitatRegion,
    terrain::terrain_water_presentation_types::{
        TerrainWaterRegionEdit, TerrainWaterRegionRetired,
    },
    topology::{
        fence_segment_presentation_types::{
            FenceSegmentPrefab, FenceTerrainEndpointPresentationCells,
        },
        topology_graph_types::{FenceEdge, PathTile, TileSurface, TopologyNode},
    },
};

#[derive(SystemParam)]
struct ComponentChanges<'w, 's, T: Component> {
    changed: Query<'w, 's, Entity, Changed<T>>,
    removed: RemovedComponents<'w, 's, T>,
}

impl<T: Component> ComponentChanges<'_, '_, T> {
    fn read(&mut self) -> bool {
        self.removed.read().count() > 0 || !self.changed.is_empty()
    }
}

#[derive(SystemParam)]
pub(in crate::terrain) struct MembershipInputs<'w, 's> {
    paths: ComponentChanges<'w, 's, PathTile>,
    surfaces: ComponentChanges<'w, 's, TileSurface>,
    tanks: ComponentChanges<'w, 's, TankGeometry>,
    habitats: ComponentChanges<'w, 's, HabitatRegion>,
    nodes: ComponentChanges<'w, 's, TopologyNode>,
    fences: ComponentChanges<'w, 's, FenceEdge>,
    prefabs: ComponentChanges<'w, 's, FenceSegmentPrefab>,
    endpoints: ComponentChanges<'w, 's, FenceTerrainEndpointPresentationCells>,
    chunks: ComponentChanges<'w, 's, TerrainChunk>,
    edits: ComponentChanges<'w, 's, EditedTerrainSamples>,
    removed_indexes: RemovedComponents<'w, 's, TerrainWaterMembershipIndex>,
    tracks: ComponentChanges<'w, 's, TrackSegment>,
    parents: ComponentChanges<'w, 's, ChildOf>,
    definitions: ComponentChanges<'w, 's, DefinitionId>,
    moved_supports: Query<'w, 's, (), Changed<Transform>>,
    region_facts: Query<
        'w,
        's,
        (
            Entity,
            Option<&'static TerrainWaterRegionSource>,
            Option<&'static TerrainWaterRegionEdit>,
        ),
        Or<(
            Changed<TerrainWaterRegionSource>,
            Changed<TerrainWaterRegionEdit>,
            Changed<TerrainWaterRegionRetired>,
        )>,
    >,
    discarded_regions: MessageReader<'w, 's, WaterRegionFactsDiscarded>,
    terrain_assets: MessageReader<'w, 's, AssetEvent<TerrainAsset>>,
}

#[derive(Component)]
pub(in crate::terrain) struct AuthoredWaterMembership(TerrainWaterMembershipIndex);

#[derive(Component)]
pub(in crate::terrain) struct TerrainWaterHeightLimits(Arc<TerrainWaterSupportLimits>);

#[derive(Component)]
pub(in crate::terrain) struct TerrainWaterSelectorProjection(Arc<SelectorProjection>);

type SkySupport = (Entity, openzt2_game_data::AssetId, Option<Transform>);

#[derive(Component, PartialEq)]
pub(in crate::terrain) struct TerrainWaterSkySupports {
    dependencies: bevy::ecs::entity::EntityHashSet,
    supports: Option<Vec<SkySupport>>,
}

#[derive(Message)]
pub(in crate::terrain) struct WaterRegionFactsDiscarded {
    terrain: AssetId<TerrainAsset>,
    owner: Entity,
    authored_row: Option<usize>,
}

pub(in crate::terrain) fn invalidate_discarded_water_region_facts(
    discarded: On<
        Discard<(
            TerrainWaterRegionSource,
            TerrainWaterRegionEdit,
            TerrainWaterRegionRetired,
        )>,
    >,
    regions: Query<
        (
            Option<&TerrainWaterRegionSource>,
            Option<&TerrainWaterRegionEdit>,
        ),
        Allow<Disabled>,
    >,
    mut changes: MessageWriter<WaterRegionFactsDiscarded>,
) {
    if let Ok((source, edit)) = regions.get(discarded.entity) {
        if let Some(source) = source {
            changes.write(WaterRegionFactsDiscarded {
                terrain: source.terrain().id(),
                owner: discarded.entity,
                authored_row: Some(source.source_region_index()),
            });
        }
        if let Some(edit) = edit {
            changes.write(WaterRegionFactsDiscarded {
                terrain: edit.terrain.id(),
                owner: discarded.entity,
                authored_row: None,
            });
        }
    }
}

/// Points whose water membership changed, or `None` when the whole terrain's
/// membership was rebuilt.
#[derive(Message, Debug, Clone)]
pub(in crate::terrain) struct TerrainWaterMembershipChanged {
    pub(in crate::terrain) terrain: bevy::asset::AssetId<TerrainAsset>,
    pub(in crate::terrain) points: Option<Vec<UVec2>>,
    pub(in crate::terrain) regions: Option<bevy::ecs::entity::EntityHashSet>,
}

impl TerrainWaterMembershipChanged {
    pub(in crate::terrain) fn affects_region(
        &self,
        terrain: AssetId<TerrainAsset>,
        owner: Entity,
    ) -> bool {
        self.terrain == terrain
            && self
                .regions
                .as_ref()
                .is_none_or(|regions| regions.contains(&owner))
    }

    pub(in crate::terrain) fn affects_chunk(&self, chunk: &TerrainChunk) -> bool {
        self.terrain == chunk.asset.id()
            && self.points.as_ref().is_none_or(|points| {
                let cells = i32::from(chunk.side.saturating_sub(1));
                let minimum = chunk.source_coord * cells;
                let maximum = minimum + IVec2::splat(cells);
                points.iter().any(|point| {
                    point.as_ivec2().cmpge(minimum).all() && point.as_ivec2().cmple(maximum).all()
                })
            })
    }
}

struct SelectorProjection {
    codes: Arc<[Option<[u8; 4]>]>,
    probes: Arc<[[Box<[[Vec2; 2]]>; 8]]>,
    touched: HashSet<UVec2>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::terrain) enum WaterMembershipProjectionStatus {
    HeightSupportUnresolved,
    DefinitionsPending,
    TerrainPending,
    SourceOwnerMissing(usize),
    SourceAcceptanceUnresolved,
    EditedAffiliationUnresolved,
    SelectorProjectionUnresolved,
    Ready,
}

fn report_membership_status(
    previous: &mut Option<(
        bevy::asset::AssetId<TerrainAsset>,
        WaterMembershipProjectionStatus,
    )>,
    terrain: bevy::asset::AssetId<TerrainAsset>,
    status: WaterMembershipProjectionStatus,
) {
    if *previous != Some((terrain, status)) {
        info!(target: "openzt2_water_membership", ?terrain, ?status, "water membership projection status changed");
        *previous = Some((terrain, status));
    }
}

/// An absent index denotes unresolved ownership, not a dry terrain.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(in crate::terrain) fn project_terrain_water_membership(
    mut commands: Commands,
    assets: Res<Assets<TerrainAsset>>,
    supports: height_support::TerrainHeightSupports,
    chunks: Query<(Entity, &TerrainChunk, Option<&EditedTerrainSamples>)>,
    (
        mut indexes,
        mut authored_membership,
        mut support_limits,
        mut selectors,
        mut sky_supports,
        mut source_rows,
    ): (
        Query<&mut TerrainWaterMembershipIndex>,
        Query<&mut AuthoredWaterMembership>,
        Query<&mut TerrainWaterHeightLimits>,
        Query<&mut TerrainWaterSelectorProjection>,
        Query<&mut TerrainWaterSkySupports>,
        Query<&mut source_rows::AuthoredWaterRowGroups>,
    ),
    mut membership_changed: MessageWriter<TerrainWaterMembershipChanged>,
    regions: Query<(Entity, &TerrainWaterRegionSource)>,
    live_regions: TerrainWaterRegionQuery,
    mut changes: MessageReader<openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged>,
    active_definitions: Res<WorldDefinitions>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    selector_fences: selector_projection::SelectorFences,
    topology: Res<TopologyGrid>,
    mut inputs: MembershipInputs,
    mut diagnostic: Local<
        Option<(
            bevy::asset::AssetId<TerrainAsset>,
            WaterMembershipProjectionStatus,
        )>,
    >,
) {
    let changes: Vec<_> = changes.read().copied().collect();
    let changed_chunks: Vec<_> = changes.iter().map(|change| change.chunk).collect();
    let mut changed_regions = bevy::platform::collections::HashMap::<
        AssetId<TerrainAsset>,
        bevy::ecs::entity::EntityHashSet,
    >::default();
    let mut changed_authored =
        bevy::platform::collections::HashMap::<AssetId<TerrainAsset>, HashSet<usize>>::default();
    for discarded in inputs.discarded_regions.read() {
        changed_regions
            .entry(discarded.terrain)
            .or_default()
            .insert(discarded.owner);
        if let Some(row) = discarded.authored_row {
            changed_authored
                .entry(discarded.terrain)
                .or_default()
                .insert(row);
        }
    }
    for (owner, source, edit) in &inputs.region_facts {
        if let Some(source) = source {
            changed_authored
                .entry(source.terrain().id())
                .or_default()
                .insert(source.source_region_index());
            changed_regions
                .entry(source.terrain().id())
                .or_default()
                .insert(owner);
        }
        if let Some(edit) = edit {
            changed_regions
                .entry(edit.terrain.id())
                .or_default()
                .insert(owner);
        }
    }
    let changed_assets: HashSet<_> = inputs
        .terrain_assets
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Added { id }
            | AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::LoadedWithDependencies { id } => Some(id),
            AssetEvent::Unused { .. } => None,
        })
        .collect();
    let Some((root_entity, root, _)) = chunks
        .iter()
        .find(|(_, chunk, _)| chunk.source_coord == IVec2::ZERO)
    else {
        *diagnostic = None;
        return;
    };
    let Some(definitions) = active_definitions.get(&definition_assets) else {
        report_membership_status(
            &mut diagnostic,
            root.asset.id(),
            WaterMembershipProjectionStatus::DefinitionsPending,
        );
        return;
    };
    let chunks_changed = inputs.chunks.read();
    let edits_removed = inputs.edits.removed.read().count() > 0;
    let edits_changed = !inputs.edits.changed.is_empty();
    let terrain_changed = chunks_changed | edits_removed | edits_changed;
    // Read every removal cursor, including when another input already changed.
    let fence_geometry_changed = inputs.nodes.read()
        | inputs.fences.read()
        | inputs.prefabs.read()
        | inputs.endpoints.read();
    let height_geometry_changed =
        inputs.paths.read() | inputs.surfaces.read() | inputs.tanks.read() | inputs.habitats.read();
    let geometry_changed = fence_geometry_changed || height_geometry_changed;
    // Replacing an index is not a removal.
    let index_removed = inputs
        .removed_indexes
        .read()
        .any(|entity| !indexes.contains(entity));
    let existing_sky = sky_supports.get(root_entity).ok();
    let changed_dependencies = inputs
        .parents
        .changed
        .iter()
        .chain(inputs.parents.removed.read())
        .chain(inputs.definitions.changed.iter())
        .chain(inputs.definitions.removed.read())
        .any(|entity| existing_sky.is_some_and(|sky| sky.dependencies.contains(&entity)));
    // Advance both removal readers even when a relevant dependency was found.
    inputs.parents.removed.clear();
    inputs.definitions.removed.clear();
    let sky_stale = inputs.tracks.read()
        || changed_dependencies
        || definition_assets.is_changed()
        || active_definitions.is_changed()
        || existing_sky.is_none()
        || existing_sky.is_some_and(|sky| {
            sky.dependencies
                .iter()
                .any(|&entity| inputs.moved_supports.contains(entity))
        });
    let sky_footprints_changed =
        existing_sky.is_some_and(|sky| supports.scene_geometry_changed(&sky.dependencies));
    let next_sky = sky_stale.then(|| supports.sky_supports(definitions));
    let sky_changed = sky_footprints_changed
        || next_sky.as_ref().is_some_and(|next| {
            Some(&next.supports) != existing_sky.map(|existing| &existing.supports)
        });
    let dependencies_changed = next_sky
        .as_ref()
        .is_some_and(|next| Some(next) != existing_sky);
    let sky = next_sky
        .as_ref()
        .or(existing_sky)
        .and_then(|sky| sky.supports.as_deref());
    if !geometry_changed
        && changed_regions.is_empty()
        && !terrain_changed
        && !index_removed
        && changed_chunks.is_empty()
        && changed_assets.is_empty()
        && !definition_assets.is_changed()
        && !active_definitions.is_changed()
        && !sky_changed
        && !dependencies_changed
        && !topology.is_changed()
    {
        return;
    }
    let selectors_stale = fence_geometry_changed
        || changed_assets.contains(&root.asset.id())
        || definition_assets.is_changed()
        || active_definitions.is_changed()
        || topology.is_changed();
    let supports_stale =
        selectors_stale || height_geometry_changed || chunks_changed || sky_changed;
    let curb = openzt2_game_data::AssetId::from_key("curb");
    let empty_selector_topology = !selector_fences.iter().any(|(_, edge, _, _)| {
        definitions
            .find_fence(edge.definition)
            .and_then(|fence| definitions.find_object(fence.object))
            .or_else(|| definitions.find_object(edge.definition))
            .is_none_or(|object| !object.behavior_type_identifiers.contains(&curb))
    });
    let local_edit = !index_removed
        && !chunks_changed
        && !edits_removed
        && (!edits_changed || !changes.is_empty())
        && (geometry_changed
            || sky_changed
            || topology.is_changed()
            || !changes.is_empty()
            || changed_regions.contains_key(&root.asset.id()))
        && !changed_assets.contains(&root.asset.id())
        && !definition_assets.is_changed()
        && !active_definitions.is_changed();
    if local_edit {
        if let (Ok(mut source), Ok(mut index), Ok(row_groups), Some(asset)) = (
            authored_membership.get_mut(root_entity),
            indexes.get_mut(root_entity),
            source_rows.get(root_entity),
            assets.get(&root.asset),
        ) {
            let id = root.asset.id();
            let records = changed_regions.get(&id).cloned().unwrap_or_default();
            let atlas = asset.canonical_terrain_grid();
            let patch_geometry = (|| {
                let previous = &support_limits.get(root_entity).ok()?.0;
                let next = if supports_stale {
                    Arc::new(supports.point_limits(asset, definitions, &topology, sky?)?)
                } else {
                    previous.clone()
                };
                let mut support_points = HashSet::new();
                if supports_stale {
                    for &key in previous.keys().chain(next.keys()) {
                        if previous.get(&key) != next.get(&key) {
                            support_points.insert(UVec2::new(key % atlas.width, key / atlas.width));
                        }
                    }
                }
                let previous_selectors = &selectors.get(root_entity).ok()?.0;
                let next_selectors = if selectors_stale {
                    Arc::new(selector_projection::construct_selector_projection(
                        asset,
                        definitions,
                        &topology,
                        &selector_fences,
                    )?)
                } else {
                    previous_selectors.clone()
                };
                let mut selector_tiles = Vec::new();
                if selectors_stale {
                    let width = (atlas.width - 1) / 4;
                    for &tile in next_selectors.touched.union(&previous_selectors.touched) {
                        let index = (tile.y * width + tile.x) as usize;
                        if next_selectors.codes[index] != previous_selectors.codes[index]
                            || next_selectors.probes[index] != previous_selectors.probes[index]
                        {
                            selector_tiles.push(tile);
                        }
                    }
                }
                Some((next, next_selectors, support_points, selector_tiles))
            })();
            if let Some((limits, selector_geometry, support_points, selector_tiles)) =
                patch_geometry
            {
                let mut affected_rows = changed_authored.get(&id).cloned().unwrap_or_default();
                let mut impacted_points = HashSet::new();
                let mut impacted_tiles = HashSet::new();
                for point in support_points {
                    for tile in
                        neighbouring_tiles(point, (atlas.width - 1) / 4, (atlas.height - 1) / 4)
                    {
                        impacted_tiles.insert(tile);
                        affected_rows.extend(row_groups.connecting_row(tile));
                        for y in 0..=4 {
                            for x in 0..=4 {
                                impacted_points.insert(tile * 4 + UVec2::new(x, y));
                            }
                        }
                    }
                }
                let changed_rows = (!affected_rows.is_empty()).then_some(&affected_rows);
                let authored_patch = changed_rows.map_or(Some(None), |changed_rows| {
                    let rows = row_groups.affected_rows(changed_rows);
                    let owners = source_row_owners(&regions, asset, id);
                    if rows.iter().any(|&row| {
                        owners[row].is_none()
                            && !atlas.water_regions[row].row_major_cell_indices.is_empty()
                    }) {
                        return None;
                    }
                    settle_authored_rows(
                        asset,
                        &owners,
                        rows.into_iter(),
                        &limits,
                        &live_regions,
                        source.0.tank_minimum_depth(),
                    )
                    .map(Some)
                });
                if let Some(authored_patch) = authored_patch {
                    let mut affected_regions = records.clone();
                    if let Some((points, tiles)) = authored_patch {
                        impacted_points.extend(
                            points
                                .keys()
                                .copied()
                                .map(|point| UVec2::new(point % atlas.width, point / atlas.width)),
                        );
                        impacted_tiles.extend(tiles.keys().copied());
                        let next = points.into_iter().map(|(point, owner)| {
                            (UVec2::new(point % atlas.width, point / atlas.width), owner)
                        });
                        if source.0.set_authored_points(next).is_none() {
                            return;
                        }
                        for (tile, sides) in tiles {
                            source.0.set_side_owners(tile, sides);
                        }
                    }
                    for owner in &records {
                        impacted_points.extend(index.accepted_points(*owner));
                        impacted_tiles.extend(index.render_tiles(*owner));
                    }
                    let edits = changes.iter().filter_map(|change| {
                        let (_, chunk, edited) = chunks.get(change.chunk).ok()?;
                        (chunk.asset.id() == id).then_some((chunk, edited, change.min, change.max))
                    });
                    if supports_stale {
                        source.0.replace_height_support_limits(limits.clone());
                        index
                            .bypass_change_detection()
                            .replace_height_support_limits(limits.clone());
                    }
                    let whole_selector_topology =
                        empty_selector_topology != index.empty_selector_topology();
                    if selectors_stale {
                        if index
                            .bypass_change_detection()
                            .replace_selector_geometry(
                                selector_geometry.codes.clone(),
                                selector_geometry.probes.clone(),
                                empty_selector_topology,
                            )
                            .is_none()
                        {
                            return;
                        }
                    }
                    if let Some(mut points) = update_edited_membership(
                        asset,
                        &source.0,
                        index.bypass_change_detection(),
                        edits,
                        chunks.iter().filter_map(|(_, chunk, edited)| {
                            (chunk.asset.id() == id).then_some((chunk, edited))
                        }),
                        &live_regions,
                        impacted_points,
                        impacted_tiles,
                        &mut affected_regions,
                    ) {
                        for tile in selector_tiles {
                            for point in [tile * 4, tile * 4 + UVec2::splat(4)] {
                                index.collect_owners_near_point(point, &mut affected_regions);
                                points.push(point);
                            }
                        }
                        if supports_stale {
                            let mut current = support_limits
                                .get_mut(root_entity)
                                .expect("prepared height limits");
                            current.0 = limits;
                        }
                        if selectors_stale {
                            let mut current =
                                selectors.get_mut(root_entity).expect("prepared selectors");
                            current.0 = selector_geometry;
                        }
                        if let Some(next) = next_sky {
                            if let Ok(mut current) = sky_supports.get_mut(root_entity) {
                                current.set_if_neq(next);
                            } else {
                                commands.entity(root_entity).insert(next);
                            }
                        }
                        if whole_selector_topology
                            || !points.is_empty()
                            || !affected_regions.is_empty()
                        {
                            index.set_changed();
                            membership_changed.write(TerrainWaterMembershipChanged {
                                terrain: id,
                                points: (!whole_selector_topology).then_some(points),
                                regions: (!whole_selector_topology).then_some(affected_regions),
                            });
                        }
                        return;
                    }
                }
            }
        }
    }
    for (entity, chunk, _) in &chunks {
        if chunk.source_coord != IVec2::ZERO {
            continue;
        }
        let index = indexes.get(entity).ok();
        let regions_changed = changed_regions.contains_key(&chunk.asset.id());
        let has_edits = chunks.iter().any(|(_, candidate, edited)| {
            candidate.asset.id() == chunk.asset.id() && edited.is_some()
        });
        let Some(asset) = assets.get(&chunk.asset) else {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::TerrainPending,
            );
            if index.is_some() {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            continue;
        };
        let limits_span = info_span!("water_membership_height_limits").entered();
        let height_limits =
            if let Some(existing) = support_limits.get(entity).ok().filter(|_| !supports_stale) {
                Some(existing.0.clone())
            } else {
                sky.and_then(|sky| supports.point_limits(asset, definitions, &topology, sky))
                    .map(Arc::new)
            };
        drop(limits_span);
        let Some(height_limits) = height_limits else {
            if support_limits.contains(entity) {
                commands.entity(entity).remove::<TerrainWaterHeightLimits>();
            }
            commands
                .entity(entity)
                .remove::<TerrainWaterMembershipIndex>();
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::HeightSupportUnresolved,
            );
            continue;
        };
        if let Ok(mut existing) = support_limits.get_mut(entity) {
            if !Arc::ptr_eq(&existing.0, &height_limits) {
                existing.0 = height_limits.clone();
            }
        } else {
            commands
                .entity(entity)
                .insert(TerrainWaterHeightLimits(height_limits.clone()));
        }
        let selector = selectors
            .get(entity)
            .ok()
            .filter(|_| !selectors_stale)
            .map(|selector| Arc::clone(&selector.0))
            .or_else(|| {
                let _span = info_span!("construct_water_selector_projection").entered();
                let selector = Arc::new(selector_projection::construct_selector_projection(
                    asset,
                    definitions,
                    &topology,
                    &selector_fences,
                )?);
                if let Ok(mut current) = selectors.get_mut(entity) {
                    current.0 = Arc::clone(&selector);
                } else {
                    commands
                        .entity(entity)
                        .insert(TerrainWaterSelectorProjection(Arc::clone(&selector)));
                }
                Some(selector)
            });
        let Some(selector) = selector else {
            if selectors.contains(entity) {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterSelectorProjection>();
            }
            if index.is_some() {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SelectorProjectionUnresolved,
            );
            continue;
        };
        let stale = index.is_some();
        if let Some(index) = index {
            if terrain_changed
                || regions_changed
                || chunks.iter().any(|(entity, candidate, _)| {
                    candidate.asset == chunk.asset && changed_chunks.contains(&entity)
                })
                || index.empty_selector_topology() != empty_selector_topology
                || !index.selector_projection_matches(&selector.codes)
                || !index.fence_probes_match(&selector.probes)
                || !index.height_support_limits_match(&height_limits)
                || changed_assets.contains(&chunk.asset.id())
                || active_definitions.is_changed()
                || definition_assets.is_changed()
            {
                // Rebuilt below; the new index replaces it, or it is removed
                // if the rebuild fails.
            } else {
                report_membership_status(
                    &mut diagnostic,
                    chunk.asset.id(),
                    WaterMembershipProjectionStatus::Ready,
                );
                continue;
            }
        }
        let grid = asset.canonical_terrain_grid();
        let owners = source_row_owners(&regions, asset, chunk.asset.id());
        if let Some(row) = owners
            .iter()
            .zip(&grid.water_regions)
            .position(|(owner, row)| owner.is_none() && !row.row_major_cell_indices.is_empty())
        {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SourceOwnerMissing(row),
            );
            if stale {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            continue;
        }
        if !source_rows.contains(entity) || changed_assets.contains(&chunk.asset.id()) {
            let next = source_rows::AuthoredWaterRowGroups::new(asset);
            if let Ok(mut current) = source_rows.get_mut(entity) {
                *current = next;
            } else {
                commands.entity(entity).insert(next);
            }
        }
        let settle_span = info_span!("settle_source_water_membership").entered();
        let Some(source) = settle_source_membership(
            chunk.asset.id(),
            asset,
            &owners,
            empty_selector_topology,
            height_limits,
            &live_regions,
            definitions
                .tank_depth_policy()
                .map(|policy| policy.minimum_water_depth_cm as f32 * 0.01),
        ) else {
            report_membership_status(
                &mut diagnostic,
                chunk.asset.id(),
                WaterMembershipProjectionStatus::SourceAcceptanceUnresolved,
            );
            if stale {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            continue;
        };
        drop(settle_span);
        let mut index = source.clone();
        if has_edits {
            let _span = info_span!("project_edited_water_membership").entered();
            let Some(edited_index) = project_edited_membership(
                asset,
                &source,
                chunks.iter().filter_map(|(_, candidate, edited)| {
                    (candidate.asset == chunk.asset).then_some((candidate, edited))
                }),
                &live_regions,
            ) else {
                report_membership_status(
                    &mut diagnostic,
                    chunk.asset.id(),
                    WaterMembershipProjectionStatus::EditedAffiliationUnresolved,
                );
                if stale {
                    commands
                        .entity(entity)
                        .remove::<TerrainWaterMembershipIndex>();
                }
                continue;
            };
            index = edited_index;
        }
        if index
            .replace_selector_geometry(
                selector.codes.clone(),
                selector.probes.clone(),
                empty_selector_topology,
            )
            .is_none()
        {
            if stale {
                commands
                    .entity(entity)
                    .remove::<TerrainWaterMembershipIndex>();
            }
            continue;
        }
        let previous = indexes.get(entity).ok();
        let points = previous.and_then(|previous| index.changed_points_since(previous));
        let affected_regions = points.as_ref().zip(previous).map(|(points, previous)| {
            let mut owners = changed_regions
                .remove(&chunk.asset.id())
                .unwrap_or_default();
            for &point in points {
                previous.collect_owners_near_point(point, &mut owners);
                index.collect_owners_near_point(point, &mut owners);
            }
            owners
        });
        if let Ok(mut authored) = authored_membership.get_mut(entity) {
            authored.0 = source;
        } else {
            commands
                .entity(entity)
                .insert(AuthoredWaterMembership(source));
        }
        let change = TerrainWaterMembershipChanged {
            terrain: chunk.asset.id(),
            points,
            regions: affected_regions,
        };
        if let Ok(mut current) = indexes.get_mut(entity) {
            *current = index;
            membership_changed.write(change);
        } else {
            commands.entity(entity).insert(index);
            commands.write_message(change);
        }
        report_membership_status(
            &mut diagnostic,
            chunk.asset.id(),
            WaterMembershipProjectionStatus::Ready,
        );
    }
    if let Some(next) = next_sky {
        if let Ok(mut current) = sky_supports.get_mut(root_entity) {
            current.set_if_neq(next);
        } else {
            commands.entity(root_entity).insert(next);
        }
    }
}

fn source_row_owners(
    regions: &Query<(Entity, &TerrainWaterRegionSource)>,
    asset: &TerrainAsset,
    terrain: AssetId<TerrainAsset>,
) -> Vec<Option<Entity>> {
    let mut owners = vec![None; asset.canonical_terrain_grid().water_regions.len()];
    for (owner, source) in regions {
        if source.terrain().id() == terrain {
            if let Some(entry) = owners.get_mut(source.source_region_index()) {
                *entry = Some(owner);
            }
        }
    }
    owners
}

fn settle_source_membership(
    asset_id: bevy::asset::AssetId<TerrainAsset>,
    asset: &TerrainAsset,
    owners: &[Option<Entity>],
    empty_selector_topology: bool,
    height_limits: Arc<TerrainWaterSupportLimits>,
    live_regions: &TerrainWaterRegionQuery,
    tank_minimum_depth: Option<f32>,
) -> Option<TerrainWaterMembershipIndex> {
    let grid = asset.canonical_terrain_grid();
    let tile_width = grid.width.checked_sub(1)? / 4;
    let tile_height = grid.height.checked_sub(1)? / 4;
    if tile_width == 0
        || tile_height == 0
        || (grid.width - 1) % 4 != 0
        || (grid.height - 1) % 4 != 0
    {
        return None;
    }
    let (point_patch, side_patch) = settle_authored_rows(
        asset,
        owners,
        0..grid.water_regions.len(),
        &height_limits,
        live_regions,
        tank_minimum_depth,
    )?;
    let mut points = vec![None; grid.samples.len()];
    let mut sides = vec![[None; 4]; (tile_width * tile_height) as usize];
    for (point, owner) in point_patch {
        points[point as usize] = owner;
    }
    for (tile, owners) in side_patch {
        sides[(tile.y * tile_width + tile.x) as usize] = owners;
    }
    let mut index = TerrainWaterMembershipIndex::new(
        asset_id,
        grid.width,
        grid.height,
        points.into_boxed_slice(),
        sides.into_boxed_slice(),
        empty_selector_topology,
    )?;
    index.replace_height_support_limits(height_limits);
    Some(index.with_tank_minimum_depth(tank_minimum_depth))
}

type AuthoredWaterPointPatch = bevy::platform::collections::HashMap<u32, Option<Entity>>;
type AuthoredWaterSidePatch = bevy::platform::collections::HashMap<UVec2, [Option<Entity>; 4]>;

fn settle_authored_rows(
    asset: &TerrainAsset,
    owners: &[Option<Entity>],
    rows: impl Iterator<Item = usize>,
    height_limits: &TerrainWaterSupportLimits,
    live_regions: &TerrainWaterRegionQuery,
    tank_minimum_depth: Option<f32>,
) -> Option<(AuthoredWaterPointPatch, AuthoredWaterSidePatch)> {
    let grid = asset.canonical_terrain_grid();
    let tile_width = grid.width.checked_sub(1)? / 4;
    let tile_height = grid.height.checked_sub(1)? / 4;
    let mut points = AuthoredWaterPointPatch::default();
    let mut sides = AuthoredWaterSidePatch::default();
    for row_index in rows {
        let region = grid.water_regions.get(row_index)?;
        let active = owners
            .get(row_index)
            .copied()
            .flatten()
            .and_then(|owner| live_regions.get(owner).map(|view| (owner, view)));
        for &index in &region.row_major_cell_indices {
            if index as usize >= grid.samples.len() {
                return None;
            }
            points.entry(index).or_insert(None);
            let point = UVec2::new(index % grid.width, index / grid.width);
            for tile in neighbouring_tiles(point, tile_width, tile_height) {
                sides.entry(tile).or_insert([None; 4]);
            }
        }
        let Some((owner, view)) = active else {
            continue;
        };
        let biome = view.biome(asset)?;
        let plane = view.surface_height(asset)?;
        let high = if view.uses_tank_minimum_depth(asset)? {
            plane - tank_minimum_depth?
        } else {
            plane + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
        };
        let low = plane - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
        let mut refreshed_tiles = Vec::new();
        for &index in &region.row_major_cell_indices {
            let point = UVec2::new(index % grid.width, index / grid.width);
            asset.sample_authored_water_region_index(index as usize)?;
            let neighbours = neighbouring_tiles(point, tile_width, tile_height);
            let mut accepted = true;
            'tiles: for tile in neighbours.iter().copied() {
                let side_slots = sides.get_mut(&tile)?;
                // Side validation commits as it walks; a later failure does
                // not roll back side assignments made earlier in this call.
                for side in [0, 2, 1, 3] {
                    let bucket = TerrainWaterMembershipIndex::side_bucket(tile, side)?;
                    if !bucket.contains(&point) {
                        continue;
                    }
                    if side_slots[side].is_some_and(|current| current != owner) {
                        accepted = false;
                        break 'tiles;
                    }
                    if side_slots[side].is_none()
                        && (!side_accepts_path_exclusions(tile, side, |point| {
                            grid.samples[(point.y * grid.width + point.x) as usize]
                                .excludes_elevated_path
                        }) || !bucket.iter().all(|point| {
                            TerrainWaterMembershipIndex::height_limits_support_water_interval(
                                height_limits
                                    .get(&(point.y * grid.width + point.x))
                                    .copied()
                                    .unwrap_or_else(unconstrained_terrain_water_height),
                                *point,
                                low,
                                high,
                                asset,
                            )
                        }))
                    {
                        accepted = false;
                        break 'tiles;
                    }
                    side_slots[side] = Some(owner);
                }
            }
            points.insert(index, accepted.then_some(owner));
            if accepted {
                refreshed_tiles.extend(neighbours);
            }
        }
        refreshed_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
        refreshed_tiles.dedup();
        for tile in refreshed_tiles {
            let slots = sides.get_mut(&tile)?;
            for (side, slot) in slots.iter_mut().enumerate() {
                *slot = TerrainWaterMembershipIndex::side_bucket(tile, side)?
                    .into_iter()
                    .filter_map(|point| {
                        points
                            .get(&(point.y * grid.width + point.x))
                            .copied()
                            .flatten()
                    })
                    .last();
            }
            if slots[0] == slots[2] {
                if slots[1].is_none() {
                    slots[1] = slots[0];
                }
                if slots[3].is_none() {
                    slots[3] = slots[0];
                }
            }
            if slots[1] == slots[3] {
                if slots[0].is_none() {
                    slots[0] = slots[1];
                }
                if slots[2].is_none() {
                    slots[2] = slots[1];
                }
            }
            let flagged = |point: UVec2| {
                grid.samples[((tile.y * 4 + point.y) * grid.width + tile.x * 4 + point.x) as usize]
                    .water_surface_flag
            };
            let plane_for_owner = |owner: Entity| live_regions.get(owner)?.surface_height(asset);
            if flagged(UVec2::new(1, 3)) || flagged(UVec2::new(3, 1)) {
                merge_flagged_sides(slots, 2, 3, plane_for_owner);
                merge_flagged_sides(slots, 0, 1, plane_for_owner);
            }
            if flagged(UVec2::new(3, 3)) || flagged(UVec2::new(1, 1)) {
                merge_flagged_sides(slots, 2, 1, plane_for_owner);
                merge_flagged_sides(slots, 0, 3, plane_for_owner);
            }
            for side in 0..4 {
                if slots[side].is_some()
                    && slots[(side + 1) % 4].is_none()
                    && slots[(side + 3) % 4].is_none()
                {
                    slots[(side + 1) % 4] = slots[side];
                }
            }
        }
    }
    Some((points, sides))
}

fn project_edited_membership<'a>(
    asset: &TerrainAsset,
    base: &TerrainWaterMembershipIndex,
    chunks: impl Iterator<Item = (&'a TerrainChunk, Option<&'a EditedTerrainSamples>)>,
    regions: &TerrainWaterRegionQuery,
) -> Option<TerrainWaterMembershipIndex> {
    use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
    use openzt2_zoo_state::terrain::terrain_sample_grid_queries::{
        authored_terrain_sample_index, read_materialized_terrain_sample,
    };
    let grid = asset.canonical_terrain_grid();
    let tile_width = (grid.width - 1) / 4;
    let tile_height = (grid.height - 1) / 4;
    let mut points: Vec<_> = (0..grid.samples.len())
        .map(|index| {
            base.point_owner(UVec2::new(
                index as u32 % grid.width,
                index as u32 / grid.width,
            ))
        })
        .collect();
    let mut heights: Vec<_> = grid
        .samples
        .iter()
        .map(|point| grid.units.base_height_metres + point.height_relative_to_base_metres)
        .collect();
    let mut sides: Vec<_> = (0..tile_width * tile_height)
        .map(|index| {
            *base
                .side_owners(UVec2::new(index % tile_width, index / tile_width))
                .expect("source tile dimensions are validated")
        })
        .collect();
    let mut dirty_tiles = Vec::new();
    let mut edited_affiliation = vec![false; points.len()];
    for (chunk, edited) in chunks {
        let Some(edited) = edited else {
            continue;
        };
        for z in 0..usize::from(chunk.side) {
            for x in 0..usize::from(chunk.side) {
                let index = authored_terrain_sample_index(chunk, asset, x, z)?;
                let sample = read_materialized_terrain_sample(chunk, asset, Some(edited), x, z)?;
                let point = UVec2::new(index as u32 % grid.width, index as u32 / grid.width);
                let owner = match sample.water_affiliation {
                    Some(TerrainWaterAffiliation::SourceRow(_)) => base.point_owner(point),
                    Some(TerrainWaterAffiliation::Edited(owner)) => {
                        edited_affiliation[index] = true;
                        if let Some(region) = regions.get(owner) {
                            if region.terrain().id() != chunk.asset.id() {
                                return None;
                            }
                            Some(owner)
                        } else {
                            None
                        }
                    }
                    Some(TerrainWaterAffiliation::Unresolved) => return None,
                    None => None,
                };
                let height = f32::from(sample.height_cm) * 0.01;
                if points[index] != owner || heights[index] != height {
                    dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
                }
                points[index] = owner;
                heights[index] = height;
            }
        }
    }
    dirty_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
    dirty_tiles.dedup();
    for (index, owner) in points.iter_mut().enumerate() {
        let Some(current_owner) = *owner else {
            continue;
        };
        let point = UVec2::new(index as u32 % grid.width, index as u32 / grid.width);
        let Some(region) = regions.get(current_owner) else {
            *owner = None;
            dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
            continue;
        };
        if !edited_affiliation[index] {
            continue;
        }
        if !edited_point_accepted(asset, base, &region, point)? {
            *owner = None;
            dirty_tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
        }
    }
    dirty_tiles.sort_unstable_by_key(|tile| (tile.y, tile.x));
    dirty_tiles.dedup();
    for tile in dirty_tiles {
        sides[(tile.y * tile_width + tile.x) as usize] =
            resolve_tile_sides(asset, regions, tile, |point| {
                points[(point.y * grid.width + point.x) as usize]
            })?;
    }
    TerrainWaterMembershipIndex::new(
        base.terrain(),
        grid.width,
        grid.height,
        points.into_boxed_slice(),
        sides.into_boxed_slice(),
        base.empty_selector_topology(),
    )?
    .with_selector_projection_from(base)?
    .with_edited_point_heights(heights.into_boxed_slice())
}

/// Reassigns the samples inside each changed rectangle (inclusive local chunk
/// bounds) and the slope tiles around them, giving the same index as
/// projecting every edited chunk over `source`. Returns the points whose
/// owner or height changed and the corners of tiles whose sides changed, or
/// `None` when the change needs the whole projection.
#[allow(clippy::too_many_arguments)]
fn update_edited_membership<'a>(
    asset: &TerrainAsset,
    source: &TerrainWaterMembershipIndex,
    index: &mut TerrainWaterMembershipIndex,
    edits: impl Iterator<
        Item = (
            &'a TerrainChunk,
            Option<&'a EditedTerrainSamples>,
            UVec2,
            UVec2,
        ),
    >,
    chunks: impl Iterator<Item = (&'a TerrainChunk, Option<&'a EditedTerrainSamples>)>,
    regions: &TerrainWaterRegionQuery,
    mut points: HashSet<UVec2>,
    mut tiles: HashSet<UVec2>,
    owners: &mut bevy::ecs::entity::EntityHashSet,
) -> Option<Vec<UVec2>> {
    use openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainWaterAffiliation;
    use openzt2_zoo_state::terrain::terrain_sample_grid_queries::{
        authored_terrain_sample_index, read_materialized_terrain_sample,
    };
    let grid = asset.canonical_terrain_grid();
    let tile_width = (grid.width - 1) / 4;
    let tile_height = (grid.height - 1) / 4;
    let cells = u32::from(asset.terrain_chunk_cells_per_side()?);
    if cells == 0 {
        return None;
    }
    for (chunk, _, min, max) in edits {
        let last = u32::from(chunk.side).checked_sub(1)?;
        for z in min.y..=max.y.min(last) {
            for x in min.x..=max.x.min(last) {
                let point =
                    authored_terrain_sample_index(chunk, asset, x as usize, z as usize)? as u32;
                points.insert(UVec2::new(point % grid.width, point / grid.width));
            }
        }
    }
    let edited_chunks: bevy::platform::collections::HashMap<_, _> = chunks
        .enumerate()
        .filter_map(|(order, (chunk, edited))| Some((chunk.source_coord, (order, chunk, edited?))))
        .collect();
    let read_switch = |point: UVec2| {
        let coord = point / cells;
        let mut selected: Option<(usize, _)> = None;
        for dx in [0, u32::from(point.x > 0 && point.x % cells == 0)] {
            for dz in [0, u32::from(point.y > 0 && point.y % cells == 0)] {
                let candidate = coord - UVec2::new(dx, dz);
                if let Some(&(order, chunk, edited)) = edited_chunks.get(&candidate.as_ivec2()) {
                    if selected.is_none_or(|(previous, ..)| order > previous) {
                        let local = point - candidate * cells;
                        selected = Some((
                            order,
                            read_materialized_terrain_sample(
                                chunk,
                                asset,
                                Some(edited),
                                local.x as usize,
                                (cells - local.y) as usize,
                            )?,
                        ));
                    }
                }
            }
        }
        Some(selected.map(|(_, sample)| sample))
    };
    let owner_facts =
        |point, sample: openzt2_zoo_state::terrain::terrain_edit_data_types::TerrainSample| {
            let authored = source.point_owner(point);
            let height = f32::from(sample.height_cm) * 0.01;
            let height_changed = source.source_point_height(point, asset)? != height;
            let (owner, changed) = match sample.water_affiliation {
                Some(TerrainWaterAffiliation::SourceRow(_)) => (authored, false),
                Some(TerrainWaterAffiliation::Edited(owner)) => {
                    let projected = if let Some(region) = regions.get(owner) {
                        if region.terrain().id() != source.terrain() {
                            return None;
                        }
                        edited_point_accepted(asset, source, &region, point)?.then_some(owner)
                    } else {
                        None
                    };
                    (projected, Some(owner) != authored || projected != authored)
                }
                Some(TerrainWaterAffiliation::Unresolved) => return None,
                None => (None, authored.is_some()),
            };
            Some((
                owner.filter(|owner| regions.get(*owner).is_some()),
                height,
                changed || height_changed,
            ))
        };

    let mut authored = Vec::new();
    let mut edited = Vec::new();
    for point in points {
        index.collect_owners_near_point(point, owners);
        tiles.extend(neighbouring_tiles(point, tile_width, tile_height));
        if let Some(sample) = read_switch(point)? {
            let (owner, height, _) = owner_facts(point, sample)?;
            edited.push((point, owner, height));
        } else {
            authored.push((point, source.point_owner(point)));
        }
    }
    let mut changed = index.set_authored_points(authored)?;
    if !edited.is_empty() {
        changed.extend(index.set_edited_points(edited, asset)?);
    }
    for tile in tiles {
        index.collect_owners_near_point(tile * 4, owners);
        let mut overrides = false;
        'points: for y in 0..=4 {
            for x in 0..=4 {
                let point = tile * 4 + UVec2::new(x, y);
                if let Some(sample) = read_switch(point)? {
                    if owner_facts(point, sample)?.2 {
                        overrides = true;
                        break 'points;
                    }
                }
            }
        }
        let sides = if overrides {
            resolve_tile_sides(asset, regions, tile, |point| index.point_owner(point))?
        } else {
            *source.side_owners(tile)?
        };
        if index.set_side_owners(tile, sides)? {
            changed.extend([tile * 4, tile * 4 + UVec2::splat(4)]);
        }
    }
    changed.sort_unstable_by_key(|point| (point.y, point.x));
    changed.dedup();
    for &point in &changed {
        index.collect_owners_near_point(point, owners);
    }
    Some(changed)
}

/// Whether an edited water point can join `region`: every slope tile side it
/// lies on must allow water over elevated paths and support the region's
/// water interval.
pub(in crate::terrain) fn edited_point_accepted(
    asset: &TerrainAsset,
    source: &TerrainWaterMembershipIndex,
    region: &openzt2_zoo_state::terrain::terrain_water_presentation_types::TerrainWaterRegionView,
    point: UVec2,
) -> Option<bool> {
    let grid = asset.canonical_terrain_grid();
    let biome = region.biome(asset)?;
    let plane = region.surface_height(asset)?;
    let low = plane - biome.deep_water_depth_mm.unwrap_or(0) as f32 * 0.001;
    let high = if region.uses_tank_minimum_depth(asset)? {
        plane - source.tank_minimum_depth()?
    } else {
        plane + biome.water_shore_offset_mm.unwrap_or(0) as f32 * 0.001
    };
    Some(
        neighbouring_tiles(point, (grid.width - 1) / 4, (grid.height - 1) / 4)
            .iter()
            .all(|tile| {
                (0..4).all(|side| {
                    let Some(bucket) = TerrainWaterMembershipIndex::side_bucket(*tile, side) else {
                        return false;
                    };
                    !bucket.contains(&point)
                        || (side_accepts_path_exclusions(*tile, side, |point| {
                            grid.samples[(point.y * grid.width + point.x) as usize]
                                .excludes_elevated_path
                        }) && bucket
                            .iter()
                            .all(|point| source.supports_water_interval(*point, low, high, asset)))
                })
            }),
    )
}

/// A slope tile's four side owners from the owners of the points along each
/// side, filled across the tile and merged over flagged water surfaces.
fn resolve_tile_sides(
    asset: &TerrainAsset,
    regions: &TerrainWaterRegionQuery,
    tile: UVec2,
    point_owner: impl Fn(UVec2) -> Option<Entity>,
) -> Option<[Option<Entity>; 4]> {
    let grid = asset.canonical_terrain_grid();
    let mut slots = [None; 4];
    for (side, slot) in slots.iter_mut().enumerate() {
        *slot = TerrainWaterMembershipIndex::side_bucket(tile, side)?
            .into_iter()
            .filter_map(&point_owner)
            .last();
    }
    for (first, opposite) in [(0, 2), (1, 3)] {
        if slots[first] == slots[opposite] {
            for adjacent in [(first + 1) % 4, (first + 3) % 4] {
                if slots[adjacent].is_none() {
                    slots[adjacent] = slots[first];
                }
            }
        }
    }
    let flagged = |local: UVec2| {
        grid.samples[((tile.y * 4 + local.y) * grid.width + tile.x * 4 + local.x) as usize]
            .water_surface_flag
    };
    let plane = |owner| regions.get(owner)?.surface_height(asset);
    if flagged(UVec2::new(1, 3)) || flagged(UVec2::new(3, 1)) {
        merge_flagged_sides(&mut slots, 2, 3, plane);
        merge_flagged_sides(&mut slots, 0, 1, plane);
    }
    if flagged(UVec2::new(3, 3)) || flagged(UVec2::new(1, 1)) {
        merge_flagged_sides(&mut slots, 2, 1, plane);
        merge_flagged_sides(&mut slots, 0, 3, plane);
    }
    for side in 0..4 {
        if slots[side].is_some()
            && slots[(side + 1) % 4].is_none()
            && slots[(side + 3) % 4].is_none()
        {
            slots[(side + 1) % 4] = slots[side];
        }
    }
    Some(slots)
}

fn neighbouring_tiles(point: UVec2, width: u32, height: u32) -> Vec<UVec2> {
    let tile = point / 4;
    let mut neighbours = vec![tile];
    if point.x % 4 == 0 && tile.x > 0 {
        neighbours.push(tile - UVec2::X);
    }
    if point.y % 4 == 0 && tile.y > 0 {
        neighbours.push(tile - UVec2::Y);
    }
    if point.x % 4 == 0 && point.y % 4 == 0 && tile.x > 0 && tile.y > 0 {
        neighbours.push(tile - UVec2::ONE);
    }
    neighbours.retain(|tile| tile.x < width && tile.y < height);
    neighbours
}

#[cfg(test)]
fn bucket_height_supported(
    bucket: &[UVec2; 9],
    flagged: impl Fn(UVec2) -> bool,
    supported_height: bool,
) -> bool {
    bucket
        .iter()
        .all(|point| flagged(*point) || supported_height)
}

fn side_accepts_path_exclusions(
    tile: UVec2,
    side: usize,
    excluded: impl Fn(UVec2) -> bool,
) -> bool {
    let origin = tile * 4;
    let [a, b, c, d, centre] = [
        UVec2::ZERO,
        UVec2::new(4, 0),
        UVec2::splat(4),
        UVec2::new(0, 4),
        UVec2::splat(2),
    ]
    .map(|point| excluded(origin + point));
    match side {
        0 => !(c || d || (!centre && a && b)),
        1 => !(b || d || (!centre && a && c)),
        2 => !(a || b || (!centre && c && d)),
        3 => !(a || c || (!centre && b && d)),
        _ => false,
    }
}

fn merge_flagged_sides(
    slots: &mut [Option<Entity>; 4],
    first: usize,
    second: usize,
    height: impl Fn(Entity) -> Option<f32>,
) {
    let winner = match (slots[first], slots[second]) {
        (Some(left), Some(right)) => {
            if height(left) > height(right) {
                Some(left)
            } else {
                Some(right)
            }
        }
        (Some(owner), None) | (None, Some(owner)) => Some(owner),
        (None, None) => None,
    };
    slots[first] = winner;
    slots[second] = winner;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_height_relations_do_not_require_a_water_support_adapter() {
        let mut point = openzt2_game_data::terrain::TerrainSample {
            height_relative_to_base_metres: 13.0,
            biome_index: 0,
            has_ground_cover: false,
            water_depth: openzt2_game_data::terrain::TerrainWaterDepth::Dry,
            water_surface_flag: false,
            excludes_elevated_path: false,
            support_floor_metres: Some(-1000.0),
            height_cap_metres: Some(1000.0),
            height_is_linked: false,
            has_child_height_link: true,
        };
        assert!(side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
        point.height_is_linked = true;
        assert!(side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
        point.excludes_elevated_path = true;
        assert!(!side_accepts_path_exclusions(UVec2::ZERO, 0, |_| point.excludes_elevated_path));
    }

    #[test]
    fn final_point_row_and_column_keep_the_interior_tile() {
        assert_eq!(neighbouring_tiles(UVec2::new(8, 8), 2, 2), vec![UVec2::ONE]);
        assert_eq!(neighbouring_tiles(UVec2::new(8, 2), 2, 2), vec![UVec2::X]);
        assert_eq!(neighbouring_tiles(UVec2::new(2, 8), 2, 2), vec![UVec2::Y]);
    }

    #[test]
    fn flagged_points_skip_height_support_but_other_bucket_points_do_not() {
        let bucket = TerrainWaterMembershipIndex::side_bucket(UVec2::ZERO, 0).expect("valid side");
        assert!(bucket_height_supported(&bucket, |_| true, false));
        assert!(!bucket_height_supported(
            &bucket,
            |point| point != UVec2::new(2, 2),
            false
        ));
        assert!(bucket_height_supported(&bucket, |_| false, true));
    }
}
