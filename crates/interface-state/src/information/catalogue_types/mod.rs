use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory,
    AssetId,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CataloguePanel {
    pub category: CatalogueCategory,
    pub page: u16,
}

/// Selection shown in the buy-details panel.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectedCatalogueEntry(pub Option<AssetId>);

/// Last accepted child of one authored type list, independent of buy-details selection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RememberedCatalogueEntry(pub AssetId);

/// Animal reference retained while the player selects its care supplies.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectedCareAnimal(pub Option<AssetId>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogueDetails;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogueRowEntry {
    pub definition: AssetId,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TypeListFilter {
    pub unlocked_only: bool,
    pub kind: Option<AssetId>,
    /// Selected authored shared-data field and value, independent of type/research filters.
    pub field_value: Option<(AssetId, AssetId)>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdoptionRowEntry {
    pub species: AssetId,
    pub offer_slot_index: u16,
    pub offer_option_index: u8,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogueFilter {
    pub unlocked_only: bool,
    pub affordable_only: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PurchaseChoice {
    pub definition: AssetId,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CatalogueTypeListRowProjection;
