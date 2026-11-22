use bevy::ecs::{relationship::RelationshipTarget, system::SystemParam};
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectedFossilPiece(pub AssetId);

#[derive(Component)]
#[relationship(relationship_target = WorldFossilPieces)]
pub struct FossilPieceOfWorld(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = FossilPieceOfWorld, linked_spawn)]
pub struct WorldFossilPieces(Vec<Entity>);

#[derive(SystemParam)]
pub struct FossilCollections<'w, 's> {
    worlds: Query<'w, 's, &'static WorldFossilPieces>,
    pieces: Query<'w, 's, &'static CollectedFossilPiece>,
}

impl FossilCollections<'_, '_> {
    pub fn contains(&self, world: Entity, piece: AssetId) -> bool {
        self.worlds.get(world).is_ok_and(|pieces| {
            self.pieces
                .iter_many(pieces.iter())
                .flatten()
                .any(|collected| collected.0 == piece)
        })
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FossilPlacementScope {
    pub puzzle_root: AssetId,
    pub entity_root: AssetId,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FossilPlacementSurface {
    pub accepts_fossil_pieces: bool,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FossilAssemblyTable {
    pub object_definition_identifier: AssetId,
}

#[derive(Component)]
pub struct FossilSetAssembly(pub AssetId);

impl FossilSetAssembly {
    pub fn fossil_set_identifier(&self) -> AssetId {
        self.0
    }
}

#[derive(Component)]
pub struct FossilAssemblySlot(pub AssetId);

#[derive(Component)]
#[relationship(relationship_target = FossilAssemblySlots)]
pub struct FossilAssemblySlotOf(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = FossilAssemblySlotOf, linked_spawn)]
pub struct FossilAssemblySlots(Vec<Entity>);

#[derive(Component)]
#[relationship(relationship_target = FossilAssemblySlotOccupant)]
pub struct FossilPieceInSlot(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = FossilPieceInSlot)]
pub struct FossilAssemblySlotOccupant(Entity);

#[derive(Component)]
pub struct FossilAssemblyCompleted;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceFossilPieceInAssemblyRequest {
    pub assembly_entity: Entity,
    pub global_piece_index: u16,
    pub authored_slot_index: u16,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletedFossilSetAssembly {
    pub assembly_entity: Entity,
    pub fossil_set_identifier: AssetId,
}
