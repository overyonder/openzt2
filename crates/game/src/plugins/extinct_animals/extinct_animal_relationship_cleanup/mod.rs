use bevy::prelude::*;

use super::fossil_collection_and_assembly_types::{
    FossilAssemblyTable, FossilPiecePlacedInAssemblySlot, FossilSetAssembly,
};

pub(super) fn remove_fossil_assembly_and_piece_links_after_assembly_table_removal(
    mut removed_tables: RemovedComponents<FossilAssemblyTable>,
    pieces: Query<(Entity, &FossilPiecePlacedInAssemblySlot)>,
    assemblies: Query<(Entity, &FossilSetAssembly)>,
    mut commands: Commands,
) {
    for entity in removed_tables.read() {
        if assemblies.get(entity).is_ok() {
            commands.entity(entity).remove::<FossilSetAssembly>();
        }
        for (piece_entity, placement) in &pieces {
            if placement.assembly_entity == entity {
                commands
                    .entity(piece_entity)
                    .remove::<FossilPiecePlacedInAssemblySlot>();
            }
        }
    }
}
