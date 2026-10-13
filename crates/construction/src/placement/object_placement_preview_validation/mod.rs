use bevy::prelude::*;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::habitat::habitat_types::ContainmentChanged;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_zoo_state::terrain::terrain_edit_types::TerrainChanged;
use openzt2_zoo_state::topology::topology_graph_types::TopologyIndex;

use crate::placement::ground_path_object_placement_collision_queries::GroundPathTilesUnderObjectPlacement;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_definition_queries::select_authored_footprint_for_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::object_placement_cell_is_blocked_by_existing_object;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintOccupancyIndex;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_transaction_types::EvaluateObjectPlacementPreviewRequest;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;

// Asset reloads and terrain edits must invalidate previews even if the pointer
// and the projected boolean permission facts did not change.
#[allow(clippy::too_many_arguments)]
pub(super) fn request_changed_object_placement_preview_evaluations(
    mut containment: MessageReader<ContainmentChanged>,
    mut terrain_changes: MessageReader<TerrainChanged>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    placement: Res<PlacedObjectFootprintOccupancyIndex>,
    topology: Res<TopologyIndex>,
    previews: Query<(
        Entity,
        Ref<ConstructionPreview>,
        Ref<ObjectPlacementPreviewPermissionFacts>,
        Option<Ref<HabitatMember>>,
    )>,
    mut evaluate: MessageWriter<EvaluateObjectPlacementPreviewRequest>,
) {
    let authored_inputs_changed = terrain_changes.read().count() != 0
        || terrain_assets.is_changed()
        || definitions.is_changed()
        || active_definitions.is_changed();
    let containment_changed = containment
        .read()
        .any(|change| previews.get(change.affected_entity).is_ok());
    for (entity, preview, permission, habitat) in &previews {
        if !preview.is_changed()
            && !permission.is_changed()
            && habitat.is_none_or(|habitat| !habitat.is_changed())
            && !containment_changed
            && !placement.is_changed()
            && !topology.is_changed()
            && !authored_inputs_changed
        {
            continue;
        }
        evaluate.write(EvaluateObjectPlacementPreviewRequest {
            preview: entity,
            definition: preview.definition,
            transform: preview.transform,
        });
    }
}

pub(super) fn evaluate_requested_object_placement_previews(
    mut requests: MessageReader<EvaluateObjectPlacementPreviewRequest>,
    placement: Res<PlacedObjectFootprintOccupancyIndex>,
    ground_paths: GroundPathTilesUnderObjectPlacement,
    terrain_index: Res<TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    habitats: Query<&HabitatMember>,
    placed: Query<&PlacedObjectDefinitionReference>,
    mut previews: Query<(
        &mut ConstructionPreview,
        &mut Transform,
        Option<&ObjectPlacementPreviewPermissionFacts>,
        Option<&PlacedObjectRelocationSource>,
    )>,
) {
    let Some(catalogue) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((mut preview, mut transform, permission, relocation)) =
            previews.get_mut(request.preview)
        else {
            continue;
        };
        if preview.definition != request.definition {
            preview.definition = request.definition;
        }
        if preview.transform != request.transform {
            preview.transform = request.transform;
        }
        transform.set_if_neq(request.transform);
        let Some(permission) = permission else {
            if preview.validity != PlacementValidity::Pending {
                preview.validity = PlacementValidity::Pending;
            }
            continue;
        };
        if !permission.prefab_ready {
            if preview.validity != PlacementValidity::Pending {
                preview.validity = PlacementValidity::Pending;
            }
            continue;
        }
        let Some(definition) = resolve_object_placeable_definition(catalogue, request.definition)
        else {
            let invalid =
                PlacementValidity::Invalid(PlacementFailure::AuthoredRule(request.definition));
            if preview.validity != invalid {
                preview.validity = invalid;
            }
            continue;
        };
        let Some(turns) =
            calculate_authored_object_placement_eighth_turns(definition, &request.transform)
        else {
            let invalid =
                PlacementValidity::Invalid(PlacementFailure::AuthoredRule(request.definition));
            if preview.validity != invalid {
                preview.validity = invalid;
            }
            continue;
        };
        let footprint = select_authored_footprint_for_eighth_turns(definition, turns);
        let terrain = |position: Vec2| {
            let entity = terrain_chunk_at(&terrain_index, position)?;
            let (chunk, edited) = chunks.get(entity).ok()?;
            let asset = terrain_assets.get(&chunk.asset)?;
            sample_terrain(chunk, asset, edited, position)
        };
        let collides_with_ground_path =
            ground_paths.object_placement_cell_collides_with_ground_path(definition, catalogue);
        let validity = validate_object_placement_footprint_cells(
            definition,
            footprint,
            &request.transform,
            terrain,
            |cell| {
                collides_with_ground_path(cell)
                    || object_placement_cell_is_blocked_by_existing_object(
                        &placement,
                        cell,
                        relocation.map(|source| source.0),
                        definition,
                        catalogue,
                        |entity| {
                            placed
                                .get(entity)
                                .ok()
                                .map(|placeable| placeable.definition)
                        },
                    )
            },
            ObjectPlacementAuthoritativeFacts {
                habitat: habitats
                    .get(request.preview)
                    .ok()
                    .map(|member| member.habitat_entity),
                unlocked: permission.unlocked,
                affordable: permission.affordable,
                topology_valid: permission.topology_valid,
                headroom_valid: permission.headroom_valid,
            },
        );
        if preview.validity != validity {
            preview.validity = validity;
        }
    }
}
