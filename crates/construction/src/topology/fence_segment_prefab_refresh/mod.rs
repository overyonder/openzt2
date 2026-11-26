use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::world_objects::WorldObjectInformationViewClass;
use openzt2_progression::progression::AwardProgramEffect;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::topology::{
    topology_edit_types::FenceConnectivityChanged,
    topology_graph_types::{
        FenceEnd, FenceSegment, FenceStart, FencesEndingHere, FencesStartingHere,
    },
};

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use crate::topology::topology_presentation_types::{
    EmptyTopologyPresentation, TopologyPresentationRoot,
};
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_reciprocal_fence_curve_partners;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceSegmentPrefab;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::gate_operation_types::Gate;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

pub(super) fn replace_zoo_wall_definitions(
    mut effects: MessageReader<AwardProgramEffect>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    mut walls: Query<(&WorldMember, &mut FenceEdge)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for effect in effects.read() {
        let AwardProgramEffect::ReplaceZooWalls { root, definition } = *effect else {
            continue;
        };
        if definitions.find_fence(definition).is_none() {
            continue;
        }
        for (world, mut wall) in &mut walls {
            if world.root != root || wall.definition == definition {
                continue;
            }
            let zoo_wall = definitions
                .find_fence(wall.definition)
                .and_then(|previous| definitions.find_object(previous.object))
                .is_some_and(|object| {
                    object.view_class == Some(WorldObjectInformationViewClass::ZooWall)
                });
            if zoo_wall {
                wall.definition = definition;
            }
        }
    }
}

pub(super) fn refresh_fence_segment_prefabs_after_topology_index_changes(
    mut commands: Commands,
    topology_index: Single<&TopologyIndex>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    nodes: Query<&TopologyNode>,
    mut changes: MessageReader<FenceConnectivityChanged>,
    incident: Query<(Option<&FencesStartingHere>, Option<&FencesEndingHere>)>,
    changed_edges: Query<Entity, Or<(Changed<FenceEdge>, Changed<FenceStart>, Changed<FenceEnd>)>>,
    edges: Query<FenceSegment>,
    grid: Single<Ref<TopologyGrid>>,
    mut transforms: Query<&mut Transform, With<FenceEdge>>,
    mut fences: Query<(
        Entity,
        FenceSegment,
        Option<&mut FenceSegmentPrefab>,
        Option<&mut FenceTerrainEndpointPresentationCells>,
        Option<&Children>,
        Has<Gate>,
    )>,
    presentation_roots: Query<(), With<TopologyPresentationRoot>>,
) {
    let topology_index = topology_index.into_inner();
    let grid = grid.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut changed = changed_edges.iter().collect::<EntityHashSet>();
    let affected_nodes = changes
        .read()
        .flat_map(|event| [event.0.lo, event.0.hi])
        .filter_map(|cell| topology_index.nodes.get(&cell).copied())
        .collect::<EntityHashSet>();
    changed.extend(
        affected_nodes
            .into_iter()
            .flat_map(|node| incident_edges(node, &incident)),
    );
    let dirty = if active_definitions.is_changed() || grid.is_changed() {
        fences
            .iter()
            .map(|(entity, ..)| entity)
            .collect::<EntityHashSet>()
    } else {
        adjacent_fences(&changed, &edges, &incident)
    };
    if dirty.is_empty() {
        return;
    }
    // A candidate's other end must also be present to check reciprocity.
    let neighbourhood = adjacent_fences(
        &adjacent_fences(&dirty, &edges, &incident),
        &edges,
        &incident,
    );
    let segments = neighbourhood
        .into_iter()
        .filter_map(|entity| {
            let edge = edges.get(entity).ok()?;
            Some((
                entity,
                (
                    edge.definition,
                    nodes.get(edge.a()).ok()?.cell,
                    nodes.get(edge.b()).ok()?.cell,
                ),
            ))
        })
        .collect::<Vec<_>>();
    let pairings = select_reciprocal_fence_curve_partners(
        &segments
            .iter()
            .map(|(_, segment)| *segment)
            .collect::<Vec<_>>(),
    );
    let partners = segments
        .into_iter()
        .zip(pairings)
        .map(|((entity, _), pairing)| (entity, pairing))
        .collect::<bevy::ecs::entity::EntityHashMap<_>>();
    for fence_entity in dirty {
        let Ok((_, edge, current_prefab, endpoint_cells, children, gate)) =
            fences.get_mut(fence_entity)
        else {
            continue;
        };
        let (Ok(first_node), Ok(second_node)) = (nodes.get(edge.a()), nodes.get(edge.b())) else {
            continue;
        };
        // A gated edge shows its fence's gate variant, e.g. chainlinkgate.
        let definition = if gate {
            definitions.find_gate_fence(edge.definition)
        } else {
            definitions.find_fence(edge.definition)
        };
        let Some(definition) = definition else {
            continue;
        };
        let (first, second, next_cell) = partners.get(&fence_entity).copied().flatten().map_or(
            (first_node.cell, second_node.cell, None),
            |(first, second, next)| (first, second, Some(next)),
        );
        let selected_prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
            definition, first, second, next_cell,
        );
        if let Ok(mut transform) = transforms.get_mut(fence_entity) {
            let desired = calculate_topology_edge_world_transform(first, second, *grid);
            if transform.translation.x != desired.translation.x
                || transform.translation.z != desired.translation.z
                || transform.rotation != desired.rotation
            {
                *transform = desired;
                let next_cells = FenceTerrainEndpointPresentationCells { first, second };
                if let Some(mut endpoint_cells) = endpoint_cells {
                    endpoint_cells.set_if_neq(next_cells);
                } else {
                    commands.entity(fence_entity).insert(next_cells);
                }
            }
        }
        if current_prefab
            .as_deref()
            .is_some_and(|current_prefab| *current_prefab == selected_prefab)
        {
            continue;
        }
        for child in children.into_iter().flat_map(|children| children.iter()) {
            if presentation_roots.contains(child) {
                commands.entity(child).try_despawn();
            }
        }
        commands
            .entity(fence_entity)
            .remove::<EmptyTopologyPresentation>();
        if selected_prefab.asset == openzt2_game_data::AssetId::default() {
            if current_prefab.is_some() {
                commands.entity(fence_entity).remove::<FenceSegmentPrefab>();
            }
        } else if let Some(mut current_prefab) = current_prefab {
            *current_prefab = selected_prefab;
        } else {
            commands.entity(fence_entity).insert(selected_prefab);
        }
    }
}

pub(super) fn incident_edges<'a>(
    node: Entity,
    incident: &'a Query<(Option<&FencesStartingHere>, Option<&FencesEndingHere>)>,
) -> impl Iterator<Item = Entity> + 'a {
    incident
        .get(node)
        .ok()
        .into_iter()
        .flat_map(|(starting, ending)| {
            starting
                .into_iter()
                .flat_map(|edges| edges.iter())
                .chain(ending.into_iter().flat_map(|edges| edges.iter()))
        })
}

pub(super) fn adjacent_fences(
    starting: &EntityHashSet,
    edges: &Query<FenceSegment>,
    incident: &Query<(Option<&FencesStartingHere>, Option<&FencesEndingHere>)>,
) -> EntityHashSet {
    starting
        .iter()
        .copied()
        .chain(
            starting
                .iter()
                .filter_map(|&entity| edges.get(entity).ok())
                .flat_map(|edge| [edge.a(), edge.b()])
                .flat_map(|node| incident_edges(node, incident)),
        )
        .collect()
}
