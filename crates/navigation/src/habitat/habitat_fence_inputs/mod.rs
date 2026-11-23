use bevy::{
    ecs::{
        entity::EntityHashSet, entity_disabling::Disabled, relationship::RelationshipTarget,
        system::SystemParam, world::EntityWorldMut,
    },
    platform::collections::HashMap,
    prelude::*,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::world_definitions::fences_and_gates::FenceTraversalBlockingFlags;
use openzt2_zoo_state::{
    habitat::habitat_types::RebuildHabitatRegionsWithinTopologyBounds,
    topology::{
        gate_operation_types::Gate,
        topology_graph_types::{
            FenceEdge, FenceEnd, FenceSegment, FenceStart, FencesEndingHere, FencesStartingHere,
            TopologyNode,
        },
    },
};

use super::habitat_region_derivation::{
    expand_authored_fence_segments_into_cardinal_habitat_grid_boundaries, HabitatFenceSegment,
};

#[derive(Component)]
pub(super) struct HabitatBoundaryRaster(pub(super) Vec<HabitatFenceSegment>, Entity);

#[derive(Component, Default)]
pub(super) struct HabitatFenceLattice(HashMap<IVec2, Entity>);

#[derive(Component)]
pub(super) struct HabitatFenceLatticePoint {
    root: Entity,
    coordinate: IVec2,
}

#[derive(Component)]
#[relationship(relationship_target = IncidencesAtRasterPoint)]
struct AtRasterPoint(Entity);

#[derive(Component)]
#[relationship_target(relationship = AtRasterPoint, linked_spawn)]
pub(super) struct IncidencesAtRasterPoint(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = FenceRasterIncidences)]
struct RasterIncidenceOfFence(Entity);

#[derive(Component)]
#[relationship_target(relationship = RasterIncidenceOfFence, linked_spawn)]
pub(super) struct FenceRasterIncidences(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct HabitatFenceIncidences<'w, 's> {
    points: Query<'w, 's, &'static IncidencesAtRasterPoint>,
    links: Query<'w, 's, &'static RasterIncidenceOfFence>,
}

impl HabitatFenceIncidences<'_, '_> {
    pub(super) fn fences_at(
        &self,
        lattice: &HabitatFenceLattice,
        point: IVec2,
    ) -> impl Iterator<Item = Entity> + '_ {
        lattice
            .0
            .get(&point)
            .and_then(|&entity| self.points.get(entity).ok())
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .filter_map(move |link| self.links.get(link).ok().map(|fence| fence.0))
    }
}

pub(super) fn retire_empty_raster_point(
    removed: On<Remove<IncidencesAtRasterPoint>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .queue_silenced(|entity: EntityWorldMut| {
            if !entity.contains::<IncidencesAtRasterPoint>() {
                entity.despawn();
            }
        });
}

pub(super) fn discard_habitat_fence_lattice_point(
    discarded: On<Discard<HabitatFenceLatticePoint>>,
    points: Query<&HabitatFenceLatticePoint, Allow<Disabled>>,
    mut roots: Query<&mut HabitatFenceLattice, Allow<Disabled>>,
) {
    if let Ok(point) = points.get(discarded.entity) {
        if let Ok(mut root) = roots.get_mut(point.root) {
            if root.0.get(&point.coordinate) == Some(&discarded.entity) {
                root.0.remove(&point.coordinate);
            }
        }
    }
}

fn raster_nodes(raster: &HabitatBoundaryRaster) -> impl Iterator<Item = IVec2> + '_ {
    raster
        .0
        .iter()
        .flat_map(|edge| [edge.first_topology_node, edge.second_topology_node])
}

#[derive(Message)]
pub(super) struct HabitatFenceInputDiscarded(Entity);

pub(super) fn discard_habitat_fence_node(
    discarded: On<Discard<TopologyNode>>,
    links: Query<(Option<&FencesStartingHere>, Option<&FencesEndingHere>)>,
    mut changes: MessageWriter<HabitatFenceInputDiscarded>,
) {
    if let Ok((starts, ends)) = links.get(discarded.entity) {
        for fence in starts
            .into_iter()
            .flat_map(RelationshipTarget::iter)
            .chain(ends.into_iter().flat_map(RelationshipTarget::iter))
        {
            changes.write(HabitatFenceInputDiscarded(fence));
        }
    }
}

pub(super) fn discard_habitat_fence_raster(
    discarded: On<Discard<HabitatBoundaryRaster>>,
    rasters: Query<&HabitatBoundaryRaster>,
    incidences: Query<&FenceRasterIncidences>,
    mut commands: Commands,
    mut rebuild: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    if let Ok(raster) = rasters.get(discarded.entity) {
        if let Ok(links) = incidences.get(discarded.entity) {
            for link in links.iter() {
                commands.entity(link).try_despawn();
            }
        }
        if let Some(bounds) = raster_bounds(raster) {
            rebuild.write(RebuildHabitatRegionsWithinTopologyBounds(bounds));
        }
    }
}

fn raster_bounds(raster: &HabitatBoundaryRaster) -> Option<IRect> {
    let mut nodes = raster_nodes(raster);
    let first = nodes.next()?;
    Some(
        nodes.fold(IRect::from_corners(first, first), |bounds, node| {
            bounds.union_point(node)
        }),
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn project_changed_habitat_fence_inputs(
    mut commands: Commands,
    catalogue: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut roots: Query<(Entity, &mut HabitatFenceLattice), With<WorldRoot>>,
    mut fences: Query<(
        Entity,
        FenceSegment,
        Option<&Gate>,
        Option<&mut HabitatBoundaryRaster>,
        Option<&WorldMember>,
    )>,
    incidences: Query<&FenceRasterIncidences>,
    changed_fences: Query<
        Entity,
        (
            With<FenceEdge>,
            Or<(
                Changed<FenceEdge>,
                Changed<FenceStart>,
                Changed<FenceEnd>,
                Changed<Gate>,
                Changed<WorldMember>,
            )>,
        ),
    >,
    unprepared_fences: Query<Entity, (With<FenceEdge>, Without<HabitatBoundaryRaster>)>,
    nodes: Query<&TopologyNode>,
    changed_nodes: Query<
        (Option<&FencesStartingHere>, Option<&FencesEndingHere>),
        Changed<TopologyNode>,
    >,
    mut removed_gates: RemovedComponents<Gate>,
    mut discarded_nodes: MessageReader<HabitatFenceInputDiscarded>,
    mut removed_fences: RemovedComponents<FenceEdge>,
    mut rebuild: MessageWriter<RebuildHabitatRegionsWithinTopologyBounds>,
) {
    let Some(definitions) = catalogue.get(&assets) else {
        return;
    };
    let mut affected: EntityHashSet = changed_fences
        .iter()
        .chain(unprepared_fences.iter())
        .chain(removed_gates.read())
        .chain(discarded_nodes.read().map(|change| change.0))
        .collect();
    for (starts, ends) in &changed_nodes {
        affected.extend(
            starts
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .chain(ends.into_iter().flat_map(RelationshipTarget::iter)),
        );
    }
    if catalogue.is_changed() || assets.is_changed() {
        affected.extend(fences.iter().map(|(entity, ..)| entity));
    }
    for fence in removed_fences.read() {
        if !fences.contains(fence) {
            commands.entity(fence).try_remove::<HabitatBoundaryRaster>();
        }
    }
    for entity in affected {
        let Ok((_, edge, gate, old, member)) = fences.get_mut(entity) else {
            continue;
        };
        let world = match member {
            Some(member) => roots.get_mut(member.root).ok(),
            None => roots.single_mut().ok(),
        };
        let Some((root, mut lattice)) = world else {
            continue;
        };
        let (Ok(a), Ok(b)) = (nodes.get(edge.a()), nodes.get(edge.b())) else {
            if old.is_some() {
                commands.entity(entity).remove::<HabitatBoundaryRaster>();
            }
            continue;
        };
        let blocks = definitions
            .find_fence(edge.definition)
            .is_some_and(|definition| {
                definition
                    .blocks
                    .contains_all(FenceTraversalBlockingFlags::ANIMAL)
            });
        let gate_open = gate.is_some_and(|gate| gate.open)
            && definitions
                .find_gate_fence(edge.definition)
                .is_none_or(|definition| {
                    definition
                        .gate_policy
                        .open_passage_types
                        .as_ref()
                        .is_none_or(|exceptions| {
                            exceptions.iter().any(|exception| {
                                exception
                                    .traversal_types
                                    .contains_all(FenceTraversalBlockingFlags::ANIMAL)
                            })
                        })
                });
        let next = HabitatBoundaryRaster(
            expand_authored_fence_segments_into_cardinal_habitat_grid_boundaries(&[
                HabitatFenceSegment {
                    fence_entity: entity,
                    first_topology_node: a.cell.truncate(),
                    second_topology_node: b.cell.truncate(),
                    boundary_is_breached: !blocks || gate_open,
                },
            ]),
            root,
        );
        if old
            .as_deref()
            .is_some_and(|old| old.0 == next.0 && old.1 == root)
        {
            continue;
        }
        // Install new links first so a shared lattice point stays occupied.
        let old_links = incidences.get(entity).ok();
        let mut linked = bevy::platform::collections::HashSet::new();
        for coordinate in raster_nodes(&next).filter(|coordinate| linked.insert(*coordinate)) {
            let point = *lattice.0.entry(coordinate).or_insert_with(|| {
                commands
                    .spawn((HabitatFenceLatticePoint { root, coordinate }, ChildOf(root)))
                    .id()
            });
            commands.spawn((AtRasterPoint(point), RasterIncidenceOfFence(entity)));
        }
        for link in old_links.into_iter().flat_map(RelationshipTarget::iter) {
            commands.entity(link).try_despawn();
        }

        let mut bounds = raster_bounds(&next);
        if let Some(mut old) = old {
            if let Some(before) = raster_bounds(&old) {
                bounds = Some(bounds.map_or(before, |bounds| bounds.union(before)));
            }
            *old = next;
        } else {
            commands.entity(entity).insert(next);
        }
        if let Some(bounds) = bounds {
            rebuild.write(RebuildHabitatRegionsWithinTopologyBounds(bounds));
        }
    }
}
