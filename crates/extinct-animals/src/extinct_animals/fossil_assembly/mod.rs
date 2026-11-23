use std::collections::VecDeque;

use bevy::{
    ecs::{entity::EntityHashMap, relationship::RelationshipTarget},
    platform::collections::HashMap,
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::extinct_animals::fossil_collection_and_assembly_types::{
    CollectedFossilPiece, CompletedFossilSetAssembly, FossilAssemblyCompleted, FossilAssemblySlot,
    FossilAssemblySlotOccupant, FossilAssemblySlotOf, FossilAssemblySlots, FossilAssemblyTable,
    FossilPieceInSlot, FossilPlacementSurface, FossilSetAssembly, PlaceFossilPieceInAssemblyRequest,
    WorldFossilPieces,
};
use openzt2_zoo_state::progression::unlock_types::ApplyCatalogueDefinitionUnlockRequest;

use super::fossil_definition_index::FossilDefinitionIndex;

struct FossilTableClaims {
    set: AssetId,
    slots: HashMap<AssetId, (Entity, bool)>,
    occupied_count: usize,
    completed: bool,
}

pub(super) fn place_requested_fossil_pieces(
    mut requests: MessageReader<PlaceFossilPieceInAssemblyRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    index: Res<FossilDefinitionIndex>,
    tables: Query<
        (
            &WorldMember,
            &GlobalTransform,
            &FossilPlacementSurface,
            Option<&FossilSetAssembly>,
            Option<&FossilAssemblySlots>,
            Has<FossilAssemblyCompleted>,
        ),
        With<FossilAssemblyTable>,
    >,
    slots: Query<(&FossilAssemblySlot, Has<FossilAssemblySlotOccupant>)>,
    world_pieces: Query<&WorldFossilPieces>,
    available_pieces: Query<(Entity, &CollectedFossilPiece), Without<FossilPieceInSlot>>,
    mut piece_poses: Query<&mut Transform, With<CollectedFossilPiece>>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut pieces_by_world = EntityHashMap::<HashMap<AssetId, VecDeque<Entity>>>::default();
    let mut claims_by_table = EntityHashMap::<FossilTableClaims>::default();
    for request in requests.read() {
        let table = request.assembly_entity;
        let (Some(piece), Some(slot)) = (
            index.pieces.get(usize::from(request.global_piece_index)),
            index.slots.get(usize::from(request.authored_slot_index)),
        ) else {
            continue;
        };
        let Some(set) = definitions.find_fossil_set(piece.set) else {
            continue;
        };
        if piece.slot != slot.id || !set.pieces.contains(&piece.id) {
            continue;
        }
        let Ok((member, pose, surface, assembly, table_slots, completed)) = tables.get(table)
        else {
            continue;
        };
        if !surface.accepts_fossil_pieces || assembly.is_some_and(|assembly| assembly.0 != set.id) {
            continue;
        }
        let pieces = pieces_by_world.entry(member.root).or_insert_with(|| {
            let mut pieces = HashMap::<AssetId, VecDeque<Entity>>::default();
            for (entity, collected) in available_pieces
                .iter_many(
                    world_pieces
                        .get(member.root)
                        .into_iter()
                        .flat_map(RelationshipTarget::iter),
                )
                .flatten()
            {
                pieces.entry(collected.0).or_default().push_back(entity);
            }
            pieces
        });
        let Some(claimable) = pieces
            .get_mut(&piece.id)
            .filter(|pieces| !pieces.is_empty())
        else {
            continue;
        };
        let claims = claims_by_table.entry(table).or_insert_with(|| {
            let mut claims = FossilTableClaims {
                set: set.id,
                slots: HashMap::default(),
                occupied_count: 0,
                completed,
            };
            if assembly.is_some() {
                for entity in table_slots.into_iter().flat_map(RelationshipTarget::iter) {
                    if let Ok((slot, occupied)) = slots.get(entity) {
                        claims.occupied_count += usize::from(occupied);
                        claims.slots.entry(slot.0).or_insert((entity, occupied));
                    }
                }
            } else {
                commands.entity(table).insert(FossilSetAssembly(set.id));
                for slot in set
                    .pieces
                    .iter()
                    .filter_map(|id| index.piece(*id).map(|piece| piece.slot))
                {
                    claims.slots.entry(slot).or_insert_with(|| {
                        let entity = commands
                            .spawn((FossilAssemblySlot(slot), FossilAssemblySlotOf(table)))
                            .id();
                        (entity, false)
                    });
                }
            }
            claims
        });
        if claims.set != set.id {
            continue;
        }
        let Some((slot_entity, occupied)) = claims.slots.get_mut(&slot.id) else {
            continue;
        };
        if *occupied {
            continue;
        }
        let Some(piece_entity) = claimable.pop_front() else {
            continue;
        };
        *occupied = true;
        claims.occupied_count += 1;
        commands
            .entity(piece_entity)
            .insert(FossilPieceInSlot(*slot_entity));
        if let Ok(mut transform) = piece_poses.get_mut(piece_entity) {
            transform.set_if_neq(Transform::from_matrix(
                pose.to_matrix() * Mat4::from_cols_array_2d(&slot.transform),
            ));
        }
        if claims.occupied_count == set.pieces.len() && !claims.completed {
            claims.completed = true;
            commands.entity(table).insert(FossilAssemblyCompleted);
            commands.write_message(CompletedFossilSetAssembly {
                assembly_entity: table,
                fossil_set_identifier: set.id,
            });
        }
    }
}

pub(super) fn clear_assembly_when_table_marker_is_removed(
    removed: On<Remove<FossilAssemblyTable>>,
    mut commands: Commands,
) {
    let table = removed.entity;
    commands.queue(move |world: &mut World| {
        if let Ok(mut table) = world.get_entity_mut(table) {
            table.despawn_related::<FossilAssemblySlots>();
            table.remove::<(FossilSetAssembly, FossilAssemblyCompleted)>();
        }
    });
}

pub(super) fn unlock_catalogue_after_fossil_assembly(
    mut completed: MessageReader<CompletedFossilSetAssembly>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: MessageWriter<ApplyCatalogueDefinitionUnlockRequest>,
    members: Query<&WorldMember>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for completion in completed.read() {
        let Some(set) = definitions.find_fossil_set(completion.fossil_set_identifier) else {
            continue;
        };
        let Ok(member) = members.get(completion.assembly_entity) else {
            continue;
        };
        if set.completion_unlock != AssetId::default() {
            unlocks.write(ApplyCatalogueDefinitionUnlockRequest {
                operation: completion.assembly_entity,
                root: member.root,
                definition: set.completion_unlock,
            });
        }
    }
}
