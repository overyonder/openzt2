use bevy::prelude::*;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;

use super::{
    fence_segment_prefab_selection::{
        select_authored_fence_segment_prefab_for_adjacent_topology,
        select_reciprocal_fence_curve_partners,
    },
    topology_graph_types::{FenceEdge, TopologyGrid, TopologyIndex, TopologyNode},
    topology_grid_geometry::calculate_topology_edge_world_transform,
    topology_presentation_types::{
        FenceSegmentPrefab, FenceTerrainEndpointPresentationCells, TopologyPresentation,
    },
};

pub(super) fn refresh_fence_segment_prefabs_after_topology_index_changes(
    mut commands: Commands,
    topology_index: Res<TopologyIndex>,
    active_definitions: Res<WorldDefinitions>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    nodes: Query<&TopologyNode>,
    grid: Res<TopologyGrid>,
    mut transforms: Query<&mut Transform, With<FenceEdge>>,
    mut fences: Query<(
        Entity,
        &FenceEdge,
        Option<&mut FenceSegmentPrefab>,
        Option<&TopologyPresentation>,
    )>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    if !topology_index.is_changed() {
        return;
    }
    let segments = fences
        .iter()
        .filter_map(|(entity, edge, _, _)| {
            Some((
                entity,
                (
                    edge.definition,
                    nodes.get(edge.a).ok()?.cell,
                    nodes.get(edge.b).ok()?.cell,
                ),
            ))
        })
        .collect::<Vec<_>>();
    let partners = select_reciprocal_fence_curve_partners(
        &segments
            .iter()
            .map(|(_, segment)| *segment)
            .collect::<Vec<_>>(),
    );
    let partners = segments
        .iter()
        .zip(partners)
        .map(|((entity, _), partner)| (*entity, partner))
        .collect::<bevy::platform::collections::HashMap<_, _>>();
    for (fence_entity, edge, current_prefab, presentation) in &mut fences {
        let (Ok(first_node), Ok(second_node)) = (nodes.get(edge.a), nodes.get(edge.b)) else {
            continue;
        };
        let Some(definition) = definitions.find_fence(edge.definition) else {
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
                commands
                    .entity(fence_entity)
                    .insert(FenceTerrainEndpointPresentationCells { first, second });
            }
        }
        if current_prefab
            .as_deref()
            .is_some_and(|current_prefab| *current_prefab == selected_prefab)
        {
            continue;
        }
        if let Some(presentation) = presentation {
            if let Some(presentation_root) = presentation.0 {
                commands.entity(presentation_root).despawn();
            }
            commands
                .entity(fence_entity)
                .remove::<TopologyPresentation>();
        }
        if selected_prefab.asset == openzt2_game_data::AssetId::default() {
            commands.entity(fence_entity).remove::<FenceSegmentPrefab>();
        } else {
            commands.entity(fence_entity).insert(selected_prefab);
        }
    }
}
