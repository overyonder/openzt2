use super::{
    object_placement_preview_permission_projection::ObjectPlacementHeadroomQueries,
    placement_model_headroom::PlacementModelHeadroom,
};
use crate::construction::construction_transaction_preparation::{
    SubmittedConstruction, SubmittedPreview,
};
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::world_definitions::object_placement::PlacementConstraints;
use openzt2_zoo_state::placement::object_placement_geometry::ScenePlacementFootprints;

use openzt2_game_assets::terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::terrain::terrain_world_sampling::sample_terrain;
use openzt2_zoo_state::terrain::terrain_world_sampling::terrain_chunk_at;
use openzt2_zoo_state::terrain::terrain_chunk_types::EditedTerrainSamples;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainChunk;
use openzt2_zoo_state::terrain::terrain_chunk_types::TerrainIndex;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use crate::placement::ground_path_placement_stomp::{
    GroundPathPlacementStomp, GroundPathStompRemovals,
};
use crate::placement::object_placement_request_routing::reject_object_placement_edit_preparation;
use crate::placement::stomped_object_edit_preparation::{
    object_stomp_prevents_placement_at_cell, prepare_stomped_object_removals, StompablePlacedObject,
};
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_origin_and_quarter_turns;
use openzt2_zoo_state::placement::object_placement_definition_queries::collect_occupied_placement_cells_for_transform;
use openzt2_zoo_state::placement::object_placement_definition_queries::resolve_object_placeable_definition;
use openzt2_zoo_state::placement::object_placement_validation::calculate_authored_object_placement_eighth_turns;
use openzt2_zoo_state::placement::object_placement_validation::validate_object_placement_footprint_cells;
use openzt2_zoo_state::placement::object_placement_validation::ObjectPlacementAuthoritativeFacts;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectDefinitionReference;
use openzt2_zoo_state::placement::placed_object_types::PlacedObjectFootprintOccupancy;
use openzt2_zoo_state::placement::placement_occupancy_index::PlacedObjectFootprintQueries;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPrefabSource;
use openzt2_zoo_state::placement::placement_preview_types::ObjectPlacementPreviewPermissionFacts;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPreparationRejected;
use openzt2_zoo_state::placement::placement_transaction_types::ObjectPlacementEditPrepared;
use openzt2_zoo_state::placement::placement_transaction_types::PlacedObjectRelocationSource;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEdit;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementEditMutationKind;
use openzt2_zoo_state::placement::placement_transaction_types::PreparedObjectPlacementMutation;
use openzt2_zoo_state::topology::topology_edit_types::TopologyEditPrepared;

/// Validates and records one requested placement or relocation transaction.
pub(super) fn prepare_object_placement_or_relocation_edit(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    previews: Query<(
        &SubmittedConstruction,
        &WorldMember,
        Option<&SubmittedPreview<ObjectPlacementPreviewPermissionFacts>>,
        Option<&SubmittedPreview<HabitatMember>>,
        Option<&SubmittedPreview<PlacedObjectRelocationSource>>,
        Option<&SubmittedPreview<ObjectPlacementPrefabSource>>,
    )>,
    relocating: Query<
        (
            &PersistentId,
            &PlacedObjectDefinitionReference,
            &Transform,
            &PlacedObjectFootprintOccupancy,
        ),
        With<RelocatingPlacedObject>,
    >,
    placed: Query<StompablePlacedObject>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    terrain_indexes: Query<&TerrainIndex>,
    terrain_assets: Res<Assets<TerrainAsset>>,
    chunks: Query<(&TerrainChunk, Option<&EditedTerrainSamples>)>,
    (placement, ground_paths, footprints): (
        PlacedObjectFootprintQueries,
        GroundPathPlacementStomp,
        Res<ScenePlacementFootprints>,
    ),
    mut allocators: Query<&mut PersistentIdAllocator>,
    mut scratch: Local<Vec<IVec2>>,
    (mut prepared, mut topology_prepared): (
        MessageWriter<ObjectPlacementEditPrepared>,
        MessageWriter<TopologyEditPrepared>,
    ),
    mut rejected: MessageWriter<ObjectPlacementEditPreparationRejected>,
    (headroom, models, prefabs): (
        ObjectPlacementHeadroomQueries,
        PlacementModelHeadroom,
        Res<Assets<ScenePrefabAsset>>,
    ),
) {
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        let Ok((submission, member, permission, habitat, relocation, prefab)) =
            previews.get(request.transaction)
        else {
            continue;
        };
        let ConstructionTool::Place(active_definition) = submission.tool else {
            continue;
        };
        let preview = &submission.preview;
        let permission = permission.map(|value| &value.0);
        let habitat = habitat.map(|value| value.0.habitat_entity);
        let relocation = relocation.map(|value| &value.0);
        let Some(prefab) = prefab.map(|value| &value.0) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Locked,
            );
            continue;
        };
        let Ok(terrain_index) = terrain_indexes.get(member.root) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::OutsideMap,
            );
            continue;
        };
        let Some(catalogue) = catalogue else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Locked,
            );
            continue;
        };
        if preview.definition != active_definition {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        }
        let Some(permission) = permission else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::Locked,
            );
            continue;
        };
        if !permission.prefab_ready {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        }
        let Some(definition) = resolve_object_placeable_definition(catalogue, preview.definition)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        };
        let Some(turns) =
            calculate_authored_object_placement_eighth_turns(definition, &preview.transform)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        };
        let geometry = footprints.resolve(definition, Some(&prefab.0));
        let terrain = |position: Vec2| {
            let entity = terrain_chunk_at(terrain_index, position)?;
            let (chunk, edited) = chunks.get(entity).ok()?;
            let asset = terrain_assets.get(&chunk.asset)?;
            sample_terrain(chunk, asset, edited, position)
        };
        let path_overlaps = ground_paths.overlaps(member.root, definition, catalogue);
        let Some(cells) = collect_occupied_placement_cells_for_transform(
            geometry,
            &preview.transform,
            &mut scratch,
        ) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        };
        let Some(clearance) =
            models.clearance(definition, prefabs.get(&prefab.0), &preview.transform)
        else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::NoHeadroom,
            );
            continue;
        };
        let y = preview.transform.translation.y;
        let flattening = definition
            .constraints
            .contains_all(PlacementConstraints::FLATTEN_TERRAIN_TO_PLACEMENT_HEIGHT)
            && (f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(&(y * 100.0));
        if clearance > f32::EPSILON
            && cells.iter().any(|cell| {
                let ray = Vec3::new(cell.x as f32, y + 0.01, cell.y as f32);
                headroom
                    .cast_occupied_cell_headroom_ray(
                        ray,
                        clearance,
                        submission.source,
                        relocation.map(|source| source.0),
                        flattening.then_some(member.root),
                        None,
                    )
                    .is_some()
            })
        {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::NoHeadroom,
            );
            continue;
        }
        let validity = validate_object_placement_footprint_cells(
            geometry,
            geometry.cells(turns),
            &preview.transform,
            terrain,
            |cell| {
                path_overlaps.prevents_placement(cell)
                    || object_stomp_prevents_placement_at_cell(
                        cell,
                        definition,
                        catalogue,
                        member.root,
                        &placement,
                        &placed,
                        |entity| Some(entity) == relocation.map(|source| source.0),
                    )
            },
            ObjectPlacementAuthoritativeFacts {
                habitat,
                unlocked: permission.unlocked,
                affordable: permission.affordable,
                topology_valid: permission.topology_valid,
                headroom_valid: true,
                surface_valid: permission.surface_valid,
            },
        );
        let PlacementValidity::Valid { cost } = validity else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                match validity {
                    PlacementValidity::Invalid(reason) => reason,
                    _ => PlacementFailure::InvalidTopology,
                },
            );
            continue;
        };
        let cells = cells.to_vec();
        let (id, previous_transform, previous_cells, mutation, applied_entity, cost) =
            if let Some(source) = relocation {
                let Ok((id, placeable, transform, occupancy)) = relocating.get(source.0) else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                if placeable.definition != preview.definition {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                }
                let Some(previous_definition) =
                    resolve_object_placeable_definition(catalogue, placeable.definition)
                else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::AuthoredRule(placeable.definition),
                    );
                    continue;
                };
                let Some(previous_cells) =
                    collect_occupied_placement_cells_for_origin_and_quarter_turns(
                        footprints
                            .resolve(previous_definition, Some(&prefab.0))
                            .cells(occupancy.eighth_turns),
                        occupancy.origin,
                        occupancy.eighth_turns / 2,
                        &mut scratch,
                    )
                    .map(<[IVec2]>::to_vec)
                else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                (
                    *id,
                    Some(*transform),
                    previous_cells,
                    PreparedObjectPlacementEditMutationKind::Relocate,
                    Some(source.0),
                    Money(0),
                )
            } else {
                let Some(id) = allocators
                    .get_mut(member.root)
                    .ok()
                    .and_then(|mut allocator| allocator.allocate().ok())
                else {
                    reject_object_placement_edit_preparation(
                        &mut rejected,
                        request.transaction,
                        PlacementFailure::InvalidTopology,
                    );
                    continue;
                };
                (
                    id,
                    None,
                    Vec::new(),
                    PreparedObjectPlacementEditMutationKind::Create,
                    None,
                    cost,
                )
            };
        let Some(mut mutations) = prepare_stomped_object_removals(
            &cells,
            definition,
            catalogue,
            member.root,
            &placement,
            &placed,
            |entity| Some(entity) == relocation.map(|source| source.0),
        ) else {
            reject_object_placement_edit_preparation(
                &mut rejected,
                request.transaction,
                PlacementFailure::InvalidTopology,
            );
            continue;
        };
        let mut removed_paths = GroundPathStompRemovals::default();
        removed_paths.collect(&path_overlaps, &cells);
        removed_paths.prepare(request.transaction, &mut commands, &mut topology_prepared);
        mutations.push(PreparedObjectPlacementMutation {
            entity: id,
            definition: preview.definition,
            transform: preview.transform,
            cells: cells.into_boxed_slice(),
            previous_transform,
            previous_cells: previous_cells.into_boxed_slice(),
            mutation,
            applied_entity,
            preview: Some(request.preview),
            prefab: prefab.0.clone(),
        });
        commands
            .entity(request.transaction)
            .insert(PreparedObjectPlacementEdit {
                mutations: mutations.into_boxed_slice(),
            });
        prepared.write(ObjectPlacementEditPrepared {
            transaction: request.transaction,
            cost,
        });
    }
}
