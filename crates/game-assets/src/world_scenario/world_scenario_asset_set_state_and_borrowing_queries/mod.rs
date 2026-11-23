//! Archive-precedence scenario asset-set state and borrowing canonical-record queries.

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::{
    world_scenario_asset_set_refresh_and_indexing::{ScenarioCatalogueIndex, ScenarioSourceOrder},
    world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
};

#[derive(Debug)]
pub(super) struct ScenarioDocumentHandle {
    pub(super) record: AssetId,
    pub(super) handle: Handle<WorldScenarioDocumentAsset>,
}

#[derive(Resource, Default)]
pub struct WorldScenarios {
    pub(super) catalogue_initialized: bool,
    pub(super) sources: BTreeMap<ScenarioSourceOrder, Handle<WorldScenarioDocumentAsset>>,
    pub(super) source_orders:
        BTreeMap<bevy::asset::AssetId<WorldScenarioDocumentAsset>, ScenarioSourceOrder>,
    pub(super) pending: BTreeSet<bevy::asset::AssetId<WorldScenarioDocumentAsset>>,
    pub(super) failed: BTreeSet<bevy::asset::AssetId<WorldScenarioDocumentAsset>>,
    pub(super) goal_sources:
        BTreeMap<ScenarioSourceOrder, BTreeSet<bevy::asset::AssetId<WorldScenarioDocumentAsset>>>,
    pub(super) goal_owners:
        BTreeMap<bevy::asset::AssetId<WorldScenarioDocumentAsset>, BTreeSet<ScenarioSourceOrder>>,
    pub(super) campaign_orders: BTreeMap<ScenarioSourceOrder, Vec<AssetId>>,
    pub(super) campaign_order: Vec<AssetId>,
    #[cfg(any(test, feature = "testing"))]
    pub(super) queued_test_sources: Vec<Handle<WorldScenarioDocumentAsset>>,
    pub(super) locations: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) maps: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) starts: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) campaigns: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) photo_challenges: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) photo_sets: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) terrains: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) textures: ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
    pub(super) scenarios: ScenarioCatalogueIndex<ScenarioDocumentHandle>,
    pub(super) start_paths: ScenarioCatalogueIndex<Box<str>>,
}

impl WorldScenarios {
    #[cfg(any(test, feature = "testing"))]
    pub fn queue_test_document(&mut self, handle: Handle<WorldScenarioDocumentAsset>) {
        self.queued_test_sources.push(handle);
    }

    #[cfg(any(test, feature = "testing"))]
    pub fn index_test_document(
        &mut self,
        handle: Handle<WorldScenarioDocumentAsset>,
        document: &openzt2_game_data::world_scenario::WorldScenarioDocument,
    ) {
        let order = (usize::MAX, self.sources.len());
        for campaign in &document.campaigns {
            self.campaigns.insert(order, campaign.id, handle.clone());
            if !self.campaign_order.contains(&campaign.id) {
                self.campaign_order.push(campaign.id);
            }
        }
        for map in &document.maps {
            self.maps.insert(order, map.id, handle.clone());
            self.terrains.insert(order, map.terrain, handle.clone());
        }
        for scenario in &document.scenarios {
            self.scenarios.insert(
                order,
                scenario.id,
                ScenarioDocumentHandle {
                    record: scenario.id,
                    handle: handle.clone(),
                },
            );
        }
        for challenge in &document.photo_challenges {
            self.photo_challenges
                .insert(order, challenge.id, handle.clone());
        }
        for set in &document.photo_challenge_sets {
            self.photo_sets.insert(order, set.id, handle.clone());
        }
        self.catalogue_initialized = true;
        self.source_orders.insert(handle.id(), order);
        self.sources.insert(order, handle);
    }

    pub fn has_pending_document_loads(
        &self,
        _asset_server: &AssetServer,
        _assets: &Assets<WorldScenarioDocumentAsset>,
    ) -> bool {
        self.has_pending_document_loads_from_publication()
    }

    pub fn catalogue_documents_are_loaded(
        &self,
        _assets: &Assets<WorldScenarioDocumentAsset>,
    ) -> bool {
        !self.has_pending_document_loads_from_publication()
            && self.failed.is_empty()
            && !self.sources.is_empty()
    }

    pub fn catalogue_document_load_has_failed(
        &self,
        _asset_server: &AssetServer,
        _assets: &Assets<WorldScenarioDocumentAsset>,
    ) -> bool {
        !self.failed.is_empty()
    }

    fn has_pending_document_loads_from_publication(&self) -> bool {
        !self.catalogue_initialized || !self.pending.is_empty() || self.has_queued_test_sources()
    }

    fn has_queued_test_sources(&self) -> bool {
        #[cfg(any(test, feature = "testing"))]
        return !self.queued_test_sources.is_empty();
        #[cfg(not(any(test, feature = "testing")))]
        false
    }

    pub fn get<'a>(
        &'a self,
        assets: &'a Assets<WorldScenarioDocumentAsset>,
    ) -> Option<WorldScenariosView<'a>> {
        (!self.maps.is_empty()
            || !self.campaigns.is_empty()
            || !self.scenarios.is_empty()
            || !self.photo_challenges.is_empty()
            || !self.photo_sets.is_empty())
        .then_some(WorldScenariosView {
            owner: self,
            assets,
            documents: [None; 2],
        })
    }
}

#[derive(Clone, Copy)]
pub struct WorldScenariosView<'a> {
    owner: &'a WorldScenarios,
    assets: &'a Assets<WorldScenarioDocumentAsset>,
    documents: [Option<(
        &'a Handle<WorldScenarioDocumentAsset>,
        &'a WorldScenarioDocumentAsset,
    )>; 2],
}

impl<'a> WorldScenariosView<'a> {
    pub fn with_document(mut self, handle: &'a Handle<WorldScenarioDocumentAsset>) -> Option<Self> {
        let document = self.assets.get(handle)?;
        if self
            .documents
            .iter()
            .flatten()
            .any(|(existing, _)| existing.id() == handle.id())
        {
            return Some(self);
        }
        *self.documents.iter_mut().find(|entry| entry.is_none())? = Some((handle, document));
        Some(self)
    }

    fn scoped_document(
        self,
        contains: impl Fn(&WorldScenarioDocumentAsset) -> bool,
    ) -> Option<(
        &'a Handle<WorldScenarioDocumentAsset>,
        &'a WorldScenarioDocumentAsset,
    )> {
        self.documents
            .into_iter()
            .flatten()
            .find(|(_, document)| contains(document))
    }

    fn asset(
        self,
        index: &'a ScenarioCatalogueIndex<Handle<WorldScenarioDocumentAsset>>,
        id: AssetId,
    ) -> Option<&'a WorldScenarioDocumentAsset> {
        self.assets.get(index.get(&id)?)
    }

    pub fn map(self, id: AssetId) -> Option<&'a openzt2_game_data::world_scenario::WorldMapRecord> {
        self.scoped_document(|document| document.document.find_world_map(id).is_some())
            .map(|(_, document)| document)
            .or_else(|| self.asset(&self.owner.maps, id))?
            .document
            .find_world_map(id)
    }

    pub fn location(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::WorldLocationRecord> {
        self.asset(&self.owner.locations, id)?
            .document
            .locations
            .iter()
            .find(|record| record.id == id)
    }

    pub fn start(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::StartingZooRecord> {
        self.scoped_document(|document| document.document.find_starting_zoo(id).is_some())
            .map(|(_, document)| document)
            .or_else(|| self.asset(&self.owner.starts, id))?
            .document
            .find_starting_zoo(id)
    }

    pub fn scenario(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::ScenarioDefinitionRecord> {
        let (_, document, record) = self.scenario_document(id)?;
        document.document.find_scenario_record(record)
    }

    fn scenario_document(
        self,
        id: AssetId,
    ) -> Option<(
        &'a Handle<WorldScenarioDocumentAsset>,
        &'a WorldScenarioDocumentAsset,
        AssetId,
    )> {
        if let Some((handle, document)) =
            self.scoped_document(|document| document.document.find_scenario_record(id).is_some())
        {
            return Some((handle, document, id));
        }
        let source = self.owner.scenarios.get(&id)?;
        let (handle, document) = self
            .scoped_document(|document| {
                document
                    .document
                    .find_scenario_record(source.record)
                    .is_some()
            })
            .or_else(|| Some((&source.handle, self.assets.get(&source.handle)?)))?;
        Some((handle, document, source.record))
    }

    pub fn campaign_scenario(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::CampaignScenarioRecord> {
        self.documents
            .into_iter()
            .flatten()
            .flat_map(|(_, document)| &document.document.campaigns)
            .flat_map(|campaign| &campaign.scenarios)
            .find(|scenario| scenario.id == id)
            .or_else(|| {
                self.campaigns()
                    .flat_map(|campaign| &campaign.scenarios)
                    .find(|scenario| scenario.id == id)
            })
    }

    pub fn campaign_for_scenario(
        self,
        id: AssetId,
    ) -> Option<(
        &'a openzt2_game_data::world_scenario::ScenarioCampaignRecord,
        usize,
    )> {
        self.campaigns().find_map(|campaign| {
            campaign
                .scenarios
                .iter()
                .position(|scenario| scenario.id == id)
                .map(|index| (campaign, index))
        })
    }

    pub fn campaign(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::ScenarioCampaignRecord> {
        let asset = self.asset(&self.owner.campaigns, id)?;
        // Campaign labels and map references belong to this document. A slow
        // or failed goal document must not hide the campaign or its siblings.
        asset.document.find_campaign_record(id)
    }

    pub fn document_for_scenario(self, id: AssetId) -> Option<&'a WorldScenarioDocumentAsset> {
        self.scenario_document(id).map(|(_, document, _)| document)
    }

    pub fn handle_for_scenario(
        self,
        id: AssetId,
    ) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.scenario_document(id).map(|(handle, _, _)| handle)
    }

    pub fn handle_for_map(self, id: AssetId) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.scoped_document(|document| document.document.find_world_map(id).is_some())
            .map(|(handle, _)| handle)
            .or_else(|| self.owner.maps.get(&id))
    }

    pub fn handle_for_start(self, id: AssetId) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.scoped_document(|document| document.document.find_starting_zoo(id).is_some())
            .map(|(handle, _)| handle)
            .or_else(|| self.owner.starts.get(&id))
    }

    pub fn has_terrain(self, id: AssetId) -> bool {
        self.owner.terrains.contains_key(&id)
    }

    pub fn start_path(self, id: AssetId) -> Option<&'a str> {
        self.owner.start_paths.get(&id).map(AsRef::as_ref)
    }

    pub fn texture_image(self, id: AssetId) -> Option<Handle<Image>> {
        self.assets
            .get(self.owner.textures.get(&id)?)?
            .texture_image(id)
    }

    pub fn maps(
        self,
    ) -> impl Iterator<Item = &'a openzt2_game_data::world_scenario::WorldMapRecord> {
        self.owner.maps.keys().filter_map(move |id| self.map(*id))
    }

    pub fn campaigns(
        self,
    ) -> impl Iterator<Item = &'a openzt2_game_data::world_scenario::ScenarioCampaignRecord> {
        self.owner
            .campaign_order
            .iter()
            .filter_map(move |id| self.campaign(*id))
    }

    pub fn scenarios(
        self,
    ) -> impl Iterator<Item = &'a openzt2_game_data::world_scenario::ScenarioDefinitionRecord> {
        self.owner
            .scenarios
            .iter()
            .filter(|(id, source)| **id == source.record)
            .filter_map(move |(id, _)| self.scenario(*id))
    }

    pub fn photo_challenge(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::PhotoChallengeRule> {
        self.asset(&self.owner.photo_challenges, id)?
            .document
            .find_photo_challenge_rule(id)
    }

    pub fn handle_for_photo_challenge(
        self,
        id: AssetId,
    ) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.owner.photo_challenges.get(&id)
    }

    pub fn photo_challenges(
        self,
    ) -> impl Iterator<Item = &'a openzt2_game_data::world_scenario::PhotoChallengeRule> {
        self.owner
            .photo_challenges
            .keys()
            .filter_map(move |id| self.photo_challenge(*id))
    }

    pub fn photo_set(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::PhotoChallengeSetRecord> {
        self.asset(&self.owner.photo_sets, id)?
            .document
            .find_photo_challenge_set(id)
    }

    pub fn handle_for_photo_set(
        self,
        id: AssetId,
    ) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.owner.photo_sets.get(&id)
    }

    pub fn photo_sets(
        self,
    ) -> impl Iterator<Item = &'a openzt2_game_data::world_scenario::PhotoChallengeSetRecord> {
        self.owner
            .photo_sets
            .keys()
            .filter_map(move |id| self.photo_set(*id))
    }
}

#[cfg(test)]
mod tests;
