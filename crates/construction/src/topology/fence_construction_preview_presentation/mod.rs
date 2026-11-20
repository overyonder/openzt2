//! Authored fence-prefab presentation for the active topology preview.

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

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
    segment_index: usize,
}

#[derive(Component)]
#[relationship(relationship_target = FencePreviewSegments)]
pub(super) struct FencePreviewSegmentOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = FencePreviewSegmentOf, linked_spawn)]
pub(super) struct FencePreviewSegments(Vec<Entity>);

pub(super) fn remove_fence_segments_on_preview_removal(
    removed: On<Remove<FencePreview>>,
    previews: Query<&FencePreviewSegments>,
    mut commands: Commands,
) {
    if let Ok(segments) = previews.get(removed.entity) {
        for segment in segments.iter() {
            commands.entity(segment).try_despawn();
        }
    }
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
    previews: Query<
        (
            Entity,
            Ref<FencePreview>,
            &WorldMember,
            Option<&FencePreviewSegments>,
        ),
        With<ConstructionPreview>,
    >,
    fences: Query<&FenceEdge>,
    gate_hosts: GateHostEdges,
    nodes: Query<&TopologyNode>,
    mut presentation_segments: Query<(
        Entity,
        &FenceConstructionPreviewPresentationSegment,
        Option<&mut FenceSegmentPrefab>,
        Option<&TopologyPresentation>,
        &mut Transform,
        &mut FenceTerrainEndpointPresentationCells,
    )>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (owner, preview, world_member, existing_segments) in &previews {
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
                &gate_hosts,
            )
            .map(|(cells, ..)| cells)
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
        let segment_count = cells.len().saturating_sub(1);
        let mut entity_at_index = vec![None; segment_count];
        for entity in existing_segments
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((_, segment, ..)) = presentation_segments.get(entity) else {
                continue;
            };
            if let Some(slot) = entity_at_index.get_mut(segment.segment_index) {
                *slot = Some(entity);
            } else {
                commands.entity(entity).try_despawn();
            }
        }
        for (segment_index, pair) in cells.windows(2).enumerate() {
            let Some(definition) = definition else { break };
            let (first, second, next) = partners[segment_index]
                .map_or((pair[0], pair[1], None), |(first, second, next)| {
                    (first, second, Some(next))
                });
            let prefab = select_authored_fence_segment_prefab_for_adjacent_topology(
                definition, first, second, next,
            );
            let transform = calculate_topology_edge_world_transform(first, second, *grid);
            let cells = FenceTerrainEndpointPresentationCells { first, second };
            if let Some(entity) = entity_at_index[segment_index] {
                let Ok((
                    _,
                    _,
                    current_prefab,
                    presentation,
                    mut current_transform,
                    mut current_cells,
                )) = presentation_segments.get_mut(entity)
                else {
                    continue;
                };
                current_transform.set_if_neq(transform);
                current_cells.set_if_neq(cells);
                if current_prefab.as_deref() != Some(&prefab) {
                    if let Some(TopologyPresentation(Some(presentation_root))) = presentation {
                        commands.entity(*presentation_root).despawn();
                    }
                    commands.entity(entity).remove::<TopologyPresentation>();
                    if let Some(mut current_prefab) = current_prefab {
                        *current_prefab = prefab;
                    } else {
                        commands.entity(entity).insert(prefab);
                    }
                }
            } else {
                commands.spawn((
                    FenceConstructionPreviewPresentationSegment { segment_index },
                    FencePreviewSegmentOf(owner),
                    prefab,
                    cells,
                    transform,
                    Visibility::Inherited,
                    *world_member,
                ));
            }
        }
    }
}

pub(super) fn project_fence_construction_preview_validity_onto_prefab_model_tints(
    mut commands: Commands,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    previews: Query<&ConstructionPreview, With<FencePreview>>,
    presentation_segments: Query<(&FencePreviewSegmentOf, &TopologyPresentation)>,
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
            (previews.get(segment.0), presentation)
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
