use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Resource, Debug, Default, PartialEq, Eq)]
pub struct UnlockedCatalogueDefinitionSet {
    pub words: Vec<u64>,
    pub definition_count: u32,
}

#[derive(Component)]
pub(crate) struct CatalogueUnlockStorageInitialized;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyCatalogueDefinitionUnlockRequest {
    pub operation: Entity,
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogueDefinitionUnlockApplied {
    pub operation: Entity,
    pub definition: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogueDefinitionUnlockRejected {
    pub operation: Entity,
    pub definition: AssetId,
}
