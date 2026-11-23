use bevy::{
    platform::collections::{HashMap, HashSet},
    prelude::*,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_game_data::world_definitions::extinct_animal_recovery::{
    FossilPieceDefinition, FossilSlotDefinition,
};

#[derive(Resource, Default)]
pub(super) struct FossilDefinitionIndex {
    piece_indices: HashMap<AssetId, usize>,
    pub(super) pieces: Vec<FossilPieceDefinition>,
    pub(super) slots: Vec<FossilSlotDefinition>,
    pub(super) sets: Vec<AssetId>,
}

impl FossilDefinitionIndex {
    pub(super) fn piece(&self, id: AssetId) -> Option<&FossilPieceDefinition> {
        self.pieces.get(*self.piece_indices.get(&id)?)
    }
}

pub(super) fn index_winning_fossil_definitions(
    active: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    mut index: ResMut<FossilDefinitionIndex>,
) {
    let assets_changed = events.read().any(|event| {
        matches!(
            event,
            AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
        )
    });
    events.clear();
    if !index.is_added() && !active.is_changed() && !assets_changed {
        return;
    }
    let Some(definitions) = active.get(&assets) else {
        *index = FossilDefinitionIndex::default();
        return;
    };
    index.pieces = definitions.fossil_pieces().copied().collect();
    index.slots = definitions.fossil_slots().copied().collect();
    let mut seen = HashSet::default();
    index.sets = index
        .pieces
        .iter()
        .map(|piece| piece.set)
        .filter(|set| seen.insert(*set))
        .collect::<Vec<_>>();
    index.piece_indices = index
        .pieces
        .iter()
        .enumerate()
        .map(|(position, piece)| (piece.id, position))
        .collect();
}
