use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    fossil_collection_and_assembly_types::{
        CollectedFossilPiece, CollectedFossilPieceInventory, CompletedFossilSetAssembly,
        FossilAssemblyTable, FossilPiecePlacedInAssemblySlot, FossilPlacementSurface,
        FossilSetAssembly, PlaceFossilPieceInAssemblyRequest,
    },
    fossil_collection_rules::{
        find_fossil_assembly_slot_identifier_by_authored_index, find_fossil_piece_index_within_set,
        find_global_fossil_piece_index_by_definition_identifier,
    },
};

pub(super) fn place_requested_fossil_pieces_into_assembly_slots(
    mut requests: MessageReader<PlaceFossilPieceInAssemblyRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    collection: Res<CollectedFossilPieceInventory>,
    mut assemblies: Query<(
        Entity,
        &FossilPlacementSurface,
        &WorldMember,
        &GlobalTransform,
        &mut FossilSetAssembly,
    )>,
    pieces: Query<
        (Entity, &CollectedFossilPiece, &WorldMember),
        Without<FossilPiecePlacedInAssemblySlot>,
    >,
    mut commands: Commands,
    mut completed: MessageWriter<CompletedFossilSetAssembly>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((entity, surface, world, assembly_transform, mut assembly)) =
            assemblies.get_mut(request.assembly_entity)
        else {
            continue;
        };
        if entity != request.assembly_entity || !surface.accepts_fossil_pieces {
            continue;
        }
        let Some(set) = definitions.find_fossil_set(assembly.fossil_set_identifier()) else {
            continue;
        };
        let Some(piece) = definitions
            .fossil_pieces()
            .nth(usize::from(request.global_piece_index))
        else {
            continue;
        };
        let piece_id = piece.id;
        let Some(global_index) =
            find_global_fossil_piece_index_by_definition_identifier(definitions, piece_id)
        else {
            continue;
        };
        let Some(slot) = find_fossil_assembly_slot_identifier_by_authored_index(
            definitions,
            request.authored_slot_index,
        ) else {
            continue;
        };
        let Some(slot_definition) = definitions.find_fossil_slot(slot) else {
            continue;
        };
        let Some(local_index) = find_fossil_piece_index_within_set(&set.pieces, piece_id) else {
            continue;
        };
        let Ok(set_piece_count) = u16::try_from(set.pieces.len()) else {
            continue;
        };
        if piece.set != assembly.fossil_set_identifier()
            || piece.slot != slot
            || !collection.contains_global_fossil_piece_index(global_index)
            || assembly.contains_set_local_fossil_piece_index(local_index, set_piece_count)
        {
            continue;
        }
        let Some((piece_entity, _, _)) = pieces.iter().find(|(_, candidate, member)| {
            candidate.global_piece_index == request.global_piece_index
                && candidate.fossil_set_identifier == assembly.fossil_set_identifier()
                && member.root == world.root
        }) else {
            continue;
        };
        if assembly.mark_set_local_fossil_piece_placed(local_index, set_piece_count) != Some(true) {
            continue;
        }
        let slot_transform = Mat4::from_cols_array_2d(&std::array::from_fn(|column| {
            std::array::from_fn(|row| slot_definition.transform[column][row])
        }));
        commands.entity(piece_entity).insert((
            FossilPiecePlacedInAssemblySlot {
                assembly_entity: entity,
                fossil_slot_identifier: slot,
            },
            Transform::from_matrix(assembly_transform.to_matrix() * slot_transform),
        ));
        if assembly.placed_fossil_piece_count() == set_piece_count {
            completed.write(CompletedFossilSetAssembly {
                assembly_entity: entity,
                fossil_set_identifier: assembly.fossil_set_identifier(),
            });
        }
    }
}

/// The first piece placed on a table determines its fossil set.
pub(super) fn initialize_fossil_assembly_from_first_piece_placement_request(
    mut requests: MessageReader<PlaceFossilPieceInAssemblyRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    tables: Query<&FossilPlacementSurface, (With<FossilAssemblyTable>, Without<FossilSetAssembly>)>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok(surface) = tables.get(request.assembly_entity) else {
            continue;
        };
        if !surface.accepts_fossil_pieces {
            continue;
        }
        let Some(piece) = definitions
            .fossil_pieces()
            .nth(usize::from(request.global_piece_index))
        else {
            continue;
        };
        let Some(set) = definitions.find_fossil_set(piece.set) else {
            continue;
        };
        let Ok(piece_count) = u16::try_from(set.pieces.len()) else {
            continue;
        };
        commands
            .entity(request.assembly_entity)
            .insert(FossilSetAssembly::new(set.id, piece_count));
    }
}

/// Requests the completed fossil set's catalogue unlock. A zero ID grants none.
pub(super) fn request_authored_catalogue_unlocks_for_completed_fossil_sets(
    mut completed: MessageReader<CompletedFossilSetAssembly>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: MessageWriter<ApplyCatalogueDefinitionUnlockRequest>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for completion in completed.read() {
        let Some(set) = definitions.find_fossil_set(completion.fossil_set_identifier) else {
            continue;
        };
        let definition = set.completion_unlock;
        if definition != AssetId::default() {
            unlocks.write(ApplyCatalogueDefinitionUnlockRequest {
                operation: completion.assembly_entity,
                definition,
            });
        }
    }
}
