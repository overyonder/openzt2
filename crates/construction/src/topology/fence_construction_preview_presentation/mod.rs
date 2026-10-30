//! Authored fence-prefab presentation for the active topology preview.

use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabRenderable;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::topology::topology_construction_planning::calculate_fence_construction_plan;
use crate::topology::topology_construction_planning::{
    calculate_gate_replacement_plan, fence_definition_is_gate, GateHostEdges,
};
use crate::topology::topology_construction_preview_types::FencePreview;
use crate::topology::topology_presentation_types::TopologyPresentation;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_authored_fence_segment_prefab_for_adjacent_topology;
use openzt2_zoo_state::topology::fence_segment_prefab_selection::select_reciprocal_fence_curve_partners;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceSegmentPrefab;
use openzt2_zoo_state::topology::fence_segment_presentation_types::FenceTerrainEndpointPresentationCells;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
use openzt2_zoo_state::topology::topology_grid_geometry::calculate_topology_edge_world_transform;

#[cfg(test)]
mod tests;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FenceConstructionPreviewPresentationSegment {
    preview: Entity,
    segment_index: usize,
}

pub(super) fn synchronize_fence_preview_visibility_with_pointer_capture(
    capture: Res<openzt2_interface_state::ui::pointer_capture_types::UiPointerCapture>,
    cursors: Query<
        &openzt2_zoo_state::construction::construction_interaction_types::ConstructionCursor,
    >,
    mut segments: Query<&mut Visibility, With<FenceConstructionPreviewPresentationSegment>>,
) {
    let visible = !capture.over_ui && cursors.single().is_ok_and(|cursor| cursor.over_terrain);
    for mut visibility in &mut segments {
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reconcile_fence_construction_preview_presentation_segments(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    index: Res<TopologyIndex>,
    terrain: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    terrain_chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    previews: Query<(Entity, Ref<FencePreview>, &WorldMember), With<ConstructionPreview>>,
    fences: Query<&FenceEdge>,
    gate_hosts: GateHostEdges,
    nodes: Query<&TopologyNode>,
    all_previews: Query<(), With<FencePreview>>,
    presentation_segments: Query<(
        Entity,
        &FenceConstructionPreviewPresentationSegment,
        Option<&FenceSegmentPrefab>,
        Option<&TopologyPresentation>,
    )>,
) {
    for (entity, segment, _, presentation) in &presentation_segments {
        if all_previews.get(segment.preview).is_err() {
            if let Some(TopologyPresentation(Some(presentation_root))) = presentation {
                commands.entity(*presentation_root).despawn();
            }
            commands.entity(entity).despawn();
        }
    }

    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (owner, preview, world_member) in &previews {
        if !preview.is_changed() && !index.is_changed() {
            continue;
        }
        let gate = fence_definition_is_gate(definitions, preview.definition);
        // A gate previews as the gate variant of the fence it replaces.
        let mut definition = definitions.find_fence(preview.definition);
        let cells = if gate {
            calculate_gate_replacement_plan(&preview, &index, definitions, &gate_hosts).map(
                |(edge, _)| {
                    definition = gate_hosts
                        .get(edge)
                        .ok()
                        .and_then(|(host, ..)| definitions.find_gate_fence(host.definition));
                    vec![preview.from, preview.to]
                },
            )
        } else {
            calculate_fence_construction_plan(
                &preview,
                &index,
                *grid,
                definitions,
                &terrain,
                &terrain_assets,
                &terrain_chunks,
            )
            .map(|(cells, _, _)| cells)
        }
        .unwrap_or_default();
        let mut segments = cells
            .windows(2)
            .map(|pair| (preview.definition, pair[0], pair[1]))
            .collect::<Vec<_>>();
        // Pending segments must see the same junctions as the committed graph.
        // Keep them first so their presentation indexes still match the plan.
        segments.extend(fences.iter().filter_map(|edge| {
            Some((
                edge.definition,
                nodes.get(edge.a).ok()?.cell,
                nodes.get(edge.b).ok()?.cell,
            ))
        }));
        let partners = select_reciprocal_fence_curve_partners(&segments);
        for (segment_index, pair) in cells.windows(2).enumerate() {
            let Some(definition) = definition else { break };
            let (first, second, next) = partners[segment_index]
                .map_or((pair[0], pair[1], None), |(first, second, next)| {
                    (first, second, Some(next))
                });
            let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                definition, first, second, next,
            );
            let existing = presentation_segments.iter().find(|(_, segment, _, _)| {
                segment.preview == owner && segment.segment_index == segment_index
            });
            if let Some((entity, _, current_prefab, presentation)) = existing {
                commands.entity(entity).insert((
                    calculate_topology_edge_world_transform(first, second, *grid),
                    FenceTerrainEndpointPresentationCells { first, second },
                ));
                if current_prefab.is_none_or(|current| *current != prefab) {
                    if let Some(TopologyPresentation(Some(presentation_root))) = presentation {
                        commands.entity(*presentation_root).despawn();
                    }
                    commands
                        .entity(entity)
                        .remove::<TopologyPresentation>()
                        .insert(prefab);
                }
            } else {
                commands.spawn((
                    FenceConstructionPreviewPresentationSegment {
                        preview: owner,
                        segment_index,
                    },
                    prefab,
                    FenceTerrainEndpointPresentationCells { first, second },
                    calculate_topology_edge_world_transform(first, second, *grid),
                    Visibility::Inherited,
                    *world_member,
                ));
            }
        }
        for (entity, segment, _, presentation) in &presentation_segments {
            if segment.preview == owner && segment.segment_index >= cells.len().saturating_sub(1) {
                if let Some(TopologyPresentation(Some(presentation_root))) = presentation {
                    commands.entity(*presentation_root).despawn();
                }
                commands.entity(entity).despawn();
            }
        }
    }
}

pub(super) fn project_fence_construction_preview_validity_onto_prefab_model_tints(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    previews: Query<&ConstructionPreview, With<FencePreview>>,
    presentation_segments: Query<(
        &FenceConstructionPreviewPresentationSegment,
        &TopologyPresentation,
    )>,
    children: Query<&Children>,
    mut renderables: Query<(Entity, Option<&mut PrefabModelTint>), With<PrefabRenderable>>,
) {
    let Some(policy) = roots.iter().find_map(|root| {
        let document = documents.get(&root.document)?;
        matches!(
            &document.canonical_ui_document().role,
            openzt2_game_data::ui_document::document::UiDocumentRole::InGameHud
        )
        .then_some(
            &document
                .canonical_ui_document()
                .construction_placement_preview,
        )
    }) else {
        return;
    };
    for (segment, presentation) in &presentation_segments {
        let (Ok(preview), TopologyPresentation(Some(presentation_root))) =
            (previews.get(segment.preview), presentation)
        else {
            continue;
        };
        let authored_srgba = if matches!(preview.validity, PlacementValidity::Valid { .. }) {
            policy.fence_valid_srgba
        } else {
            policy.fence_invalid_srgba
        };
        let tint = Color::srgba_u8(
            authored_srgba[0],
            authored_srgba[1],
            authored_srgba[2],
            authored_srgba[3],
        );
        for entity in children.iter_descendants_depth_first::<Children>(*presentation_root) {
            let Ok((entity, current)) = renderables.get_mut(entity) else {
                continue;
            };
            if let Some(mut current) = current {
                if current.0 != tint {
                    current.0 = tint;
                }
            } else {
                commands.entity(entity).insert(PrefabModelTint(tint));
            }
        }
    }
}
