use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;

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

pub(super) fn place_requested_fossil_pieces(
    mut requests: MessageReader<PlaceFossilPieceInAssemblyRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    index: Res<FossilDefinitionIndex>,
    mut commands: Commands,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
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
        let authored_slots: Vec<AssetId> = set
            .pieces
            .iter()
            .filter_map(|id| index.piece(*id).map(|piece| piece.slot))
            .collect();
        let placement = FossilSlotPlacement {
            table: request.assembly_entity,
            piece: piece.id,
            set: piece.set,
            slot: slot.id,
            transform: Mat4::from_cols_array_2d(&slot.transform),
            authored_slots,
            required_pieces: set.pieces.len(),
        };
        // The next request must see the slot and piece claimed by this commit.
        commands.queue(move |world: &mut World| placement.commit(world));
    }
}

struct FossilSlotPlacement {
    table: Entity,
    piece: AssetId,
    set: AssetId,
    slot: AssetId,
    transform: Mat4,
    authored_slots: Vec<AssetId>,
    required_pieces: usize,
}

impl FossilSlotPlacement {
    fn commit(self, world: &mut World) {
        let Some(member) = world.get::<WorldMember>(self.table) else {
            return;
        };
        let Some(table_pose) = world.get::<GlobalTransform>(self.table) else {
            return;
        };
        if world.get::<FossilAssemblyTable>(self.table).is_none()
            || !world
                .get::<FossilPlacementSurface>(self.table)
                .is_some_and(|surface| surface.accepts_fossil_pieces)
            || world
                .get::<FossilSetAssembly>(self.table)
                .is_some_and(|assembly| assembly.0 != self.set)
        {
            return;
        }
        let Some(piece) = world
            .get::<WorldFossilPieces>(member.root)
            .and_then(|pieces| {
                pieces.iter().find(|piece| {
                    world
                        .get::<CollectedFossilPiece>(*piece)
                        .is_some_and(|piece| piece.0 == self.piece)
                        && world.get::<FossilPieceInSlot>(*piece).is_none()
                })
            })
        else {
            return;
        };
        let pose = Transform::from_matrix(table_pose.to_matrix() * self.transform);
        if world.get::<FossilSetAssembly>(self.table).is_none() {
            world
                .entity_mut(self.table)
                .insert(FossilSetAssembly(self.set));
            let mut slots = bevy::platform::collections::HashSet::<AssetId>::default();
            for slot in self.authored_slots {
                if slots.insert(slot) {
                    world.spawn((FossilAssemblySlot(slot), FossilAssemblySlotOf(self.table)));
                }
            }
        }
        let Some(slot) = world
            .get::<FossilAssemblySlots>(self.table)
            .and_then(|slots| {
                slots.iter().find(|slot| {
                    world
                        .get::<FossilAssemblySlot>(*slot)
                        .is_some_and(|slot| slot.0 == self.slot)
                })
            })
        else {
            return;
        };
        if world.get::<FossilAssemblySlotOccupant>(slot).is_some() {
            return;
        }
        world.entity_mut(piece).insert(FossilPieceInSlot(slot));
        if let Some(mut transform) = world.get_mut::<Transform>(piece) {
            transform.set_if_neq(pose);
        }
        let complete = world
            .get::<FossilAssemblySlots>(self.table)
            .is_some_and(|slots| {
                slots
                    .iter()
                    .filter(|slot| world.get::<FossilAssemblySlotOccupant>(*slot).is_some())
                    .count()
                    == self.required_pieces
            });
        if complete && world.get::<FossilAssemblyCompleted>(self.table).is_none() {
            world.entity_mut(self.table).insert(FossilAssemblyCompleted);
            world.write_message(CompletedFossilSetAssembly {
                assembly_entity: self.table,
                fossil_set_identifier: self.set,
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
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for completion in completed.read() {
        let Some(set) = definitions.find_fossil_set(completion.fossil_set_identifier) else {
            continue;
        };
        if set.completion_unlock != AssetId::default() {
            unlocks.write(ApplyCatalogueDefinitionUnlockRequest {
                operation: completion.assembly_entity,
                definition: set.completion_unlock,
            });
        }
    }
}
