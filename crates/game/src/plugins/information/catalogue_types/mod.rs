use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::catalogue_and_progression::catalogue_definition_types::CatalogueCategory,
    AssetId,
};

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CataloguePanel {
    pub(crate) category: CatalogueCategory,
    pub(crate) page: u16,
}

/// Selection shown in the buy-details panel.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SelectedCatalogueEntry(pub(crate) Option<AssetId>);

/// Last accepted child of one authored type list, independent of buy-details selection.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RememberedCatalogueEntry(pub(super) AssetId);

/// Animal reference retained while the player selects its care supplies.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SelectedCareAnimal(pub(super) Option<AssetId>);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CatalogueDetails;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CatalogueRowEntry {
    pub(crate) definition: AssetId,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TypeListFilter {
    pub(super) unlocked_only: bool,
    pub(super) kind: Option<AssetId>,
    /// Selected authored shared-data field and value, independent of type/research filters.
    pub(super) field_value: Option<(AssetId, AssetId)>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AdoptionRowEntry {
    pub(super) species: AssetId,
    pub(super) offer_slot_index: u16,
    pub(super) offer_option_index: u8,
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CatalogueFilter {
    pub(super) unlocked_only: bool,
    pub(super) affordable_only: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PurchaseChoice {
    pub(crate) definition: AssetId,
}
