//! Authored fence-prefab presentation for the active topology preview.

use bevy::prelude::*;

use crate::assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_interaction_types::ConstructionPreview;
use crate::plugins::construction::construction_interaction_types::PlacementValidity;
use crate::plugins::terrain::terrain_chunk_types::EditedTerrainSamples;
use crate::plugins::terrain::terrain_chunk_types::TerrainChunk;
use crate::plugins::terrain::terrain_chunk_types::TerrainIndex;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::world_spawn::prefab_model_tint::PrefabModelTint;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabRenderable;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    fence_segment_prefab_selection::{
        select_authored_fence_segment_prefab_for_adjacent_topology,
        select_reciprocal_fence_curve_partners,
    },
    topology_construction_planning::calculate_fence_construction_plan,
    topology_construction_preview_types::FencePreview,
    topology_graph_types::{TopologyGrid, TopologyIndex},
    topology_grid_geometry::calculate_topology_edge_world_transform,
    topology_presentation_types::{
        FenceSegmentPrefab, FenceTerrainEndpointPresentationCells, TopologyPresentation,
    },
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FenceConstructionPreviewPresentationSegment {
    preview: Entity,
    segment_index: usize,
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
        (Entity, &FencePreview, &WorldMember),
        (With<ConstructionPreview>, Changed<FencePreview>),
    >,
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
        let cells = calculate_fence_construction_plan(
            preview,
            &index,
            *grid,
            definitions,
            &terrain,
            &terrain_assets,
            &terrain_chunks,
        )
        .map(|(cells, _, _)| cells)
        .unwrap_or_default();
        let definition = definitions.find_fence(preview.definition);
        let segments = cells
            .windows(2)
            .map(|pair| (preview.definition, pair[0], pair[1]))
            .collect::<Vec<_>>();
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
