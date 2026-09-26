//! Archive-precedence scenario asset-set state and borrowing canonical-record queries.

use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use super::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

#[derive(Debug)]
pub(super) struct ScenarioDocumentHandle {
    pub(super) record: AssetId,
    pub(super) handle: Handle<WorldScenarioDocumentAsset>,
}

#[derive(Resource, Default)]
pub struct WorldScenarios {
    pub(super) revision: Option<u64>,
    pub(super) dirty: bool,
    pub(super) handles: Vec<Handle<WorldScenarioDocumentAsset>>,
    pub(super) locations: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) maps: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) starts: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) campaigns: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) campaign_order: Vec<AssetId>,
    pub(super) scenarios: BTreeMap<AssetId, ScenarioDocumentHandle>,
    pub(super) photo_challenges: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) photo_sets: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) terrains: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) textures: BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
    pub(super) start_paths: BTreeMap<AssetId, Box<str>>,
}

impl WorldScenarios {
    #[cfg(test)]
    pub(crate) fn index_test_document(
        &mut self,
        handle: Handle<WorldScenarioDocumentAsset>,
        document: &openzt2_game_data::world_scenario::WorldScenarioDocument,
    ) {
        for map in &document.maps {
            self.maps.insert(map.id, handle.clone());
            self.terrains.insert(map.terrain, handle.clone());
        }
        self.handles.push(handle);
    }

    pub(crate) fn has_pending_document_loads(
        &self,
        asset_server: &AssetServer,
        assets: &Assets<WorldScenarioDocumentAsset>,
    ) -> bool {
        self.dirty
            || self.revision.is_none()
            || self.handles.iter().any(|handle| {
                !assets.contains(handle.id())
                    && !matches!(
                        asset_server.load_state(handle.id()),
                        bevy::asset::LoadState::Failed(_)
                    )
            })
    }

    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    pub(crate) fn catalogue_documents_are_loaded(
        &self,
        assets: &Assets<WorldScenarioDocumentAsset>,
    ) -> bool {
        !self.dirty
            && !self.handles.is_empty()
            && self
                .handles
                .iter()
                .all(|handle| assets.contains(handle.id()))
    }

    pub fn get<'a>(
        &'a self,
        assets: &'a Assets<WorldScenarioDocumentAsset>,
    ) -> Option<WorldScenariosView<'a>> {
        (!self.maps.is_empty() || !self.campaigns.is_empty()).then_some(WorldScenariosView {
            owner: self,
            assets,
        })
    }
}

#[derive(Clone, Copy)]
pub struct WorldScenariosView<'a> {
    owner: &'a WorldScenarios,
    assets: &'a Assets<WorldScenarioDocumentAsset>,
}

impl<'a> WorldScenariosView<'a> {
    fn asset(
        self,
        index: &'a BTreeMap<AssetId, Handle<WorldScenarioDocumentAsset>>,
        id: AssetId,
    ) -> Option<&'a WorldScenarioDocumentAsset> {
        self.assets.get(index.get(&id)?)
    }

    pub fn map(self, id: AssetId) -> Option<&'a openzt2_game_data::world_scenario::WorldMapRecord> {
        self.asset(&self.owner.maps, id)?
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
        self.asset(&self.owner.starts, id)?
            .document
            .find_starting_zoo(id)
    }

    pub fn scenario(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::ScenarioDefinitionRecord> {
        let source = self.owner.scenarios.get(&id)?;
        self.assets
            .get(&source.handle)?
            .document
            .find_scenario_record(source.record)
    }

    pub fn campaign_scenario(
        self,
        id: AssetId,
    ) -> Option<&'a openzt2_game_data::world_scenario::CampaignScenarioRecord> {
        self.campaigns()
            .flat_map(|campaign| &campaign.scenarios)
            .find(|scenario| scenario.id == id)
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
        asset
            .campaign_scenarios
            .iter()
            .all(|scenario| self.assets.get(&scenario.handle).is_some())
            .then(|| asset.document.find_campaign_record(id))?
    }

    pub fn document_for_scenario(self, id: AssetId) -> Option<&'a WorldScenarioDocumentAsset> {
        self.assets.get(&self.owner.scenarios.get(&id)?.handle)
    }

    pub fn handle_for_scenario(
        self,
        id: AssetId,
    ) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        Some(&self.owner.scenarios.get(&id)?.handle)
    }

    pub fn handle_for_map(self, id: AssetId) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.owner.maps.get(&id)
    }

    pub fn handle_for_start(self, id: AssetId) -> Option<&'a Handle<WorldScenarioDocumentAsset>> {
        self.owner.starts.get(&id)
    }

    pub fn has_terrain(self, id: AssetId) -> bool {
        self.owner.terrains.contains_key(&id)
    }

    pub(crate) fn start_path(self, id: AssetId) -> Option<&'a str> {
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
            .keys()
            .filter_map(move |id| self.scenario(*id))
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
