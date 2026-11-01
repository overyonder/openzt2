use super::super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure;
use bevy::prelude::*;
use openzt2_game_assets::world_scenario::{
    world_scenario_document_asset_and_dependency_handles::{
        DemandLoadedAssetPath, WorldScenarioDocumentAsset,
    },
    world_scenario_document_source::WorldScenarioDocumentSource,
    world_scenario_document_source_encoding::validate_world_scenario_document_source,
};
use openzt2_game_data::{world_scenario::ScenarioCampaignRecord, AssetId};
use openzt2_world_state::world_spawn::{
    selected_world_document_handles::SelectedWorldDocumentHandles,
    selected_world_identity::SelectedWorldIdentity,
};

pub(super) fn capture_saved_world_descriptor(
    handles: &SelectedWorldDocumentHandles,
    selection: &SelectedWorldIdentity,
    campaign: Option<&ScenarioCampaignRecord>,
    assets: &Assets<WorldScenarioDocumentAsset>,
    server: &AssetServer,
) -> Result<WorldScenarioDocumentSource, Failure> {
    let map = assets
        .get(&handles.map)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    let mut source = map
        .capture_source_document(server)
        .map_err(|_| Failure::UnknownAssetIdentifier)?;
    if handles.map != handles.start {
        let start = assets
            .get(&handles.start)
            .ok_or(Failure::UnknownAssetIdentifier)?;
        let additional = start
            .capture_source_document(server)
            .map_err(|_| Failure::UnknownAssetIdentifier)?;
        merge_sources(&mut source, additional)?;
    }
    preserve_retained_campaign_or_add_baseline_campaign(
        &mut source.document.campaigns,
        selection.requested,
        campaign,
    )?;
    let map = source
        .document
        .find_world_map(selection.map)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    source
        .document
        .find_starting_zoo(selection.start)
        .ok_or(Failure::UnknownAssetIdentifier)?;
    if !source.terrains.iter().any(|path| path.id == map.terrain) {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    validate_world_scenario_document_source(&source)
        .map_err(|_| Failure::CorruptSnapshotSection)?;
    Ok(source)
}

fn merge_sources(
    destination: &mut WorldScenarioDocumentSource,
    additional: WorldScenarioDocumentSource,
) -> Result<(), Failure> {
    let document = &mut destination.document;
    let source = additional.document;
    merge_records(&mut document.maps, source.maps, |record| record.id)?;
    merge_records(
        &mut document.starting_zoos,
        source.starting_zoos,
        |record| record.id,
    )?;
    merge_records(&mut document.locations, source.locations, |record| {
        record.id
    })?;
    merge_records(&mut document.campaigns, source.campaigns, |record| {
        record.id
    })?;
    merge_records(&mut document.scenarios, source.scenarios, |record| {
        record.id
    })?;
    merge_records(
        &mut document.photo_challenges,
        source.photo_challenges,
        |record| record.id,
    )?;
    merge_records(
        &mut document.photo_challenge_sets,
        source.photo_challenge_sets,
        |record| record.id,
    )?;
    append_distinct(&mut document.campaign_order, source.campaign_order);
    append_distinct(
        &mut document.scenario_script_bindings,
        source.scenario_script_bindings,
    );
    append_distinct(
        &mut document.photo_challenge_script_bindings,
        source.photo_challenge_script_bindings,
    );
    merge_paths(&mut destination.starting_zoos, additional.starting_zoos)?;
    merge_paths(&mut destination.terrains, additional.terrains)?;
    merge_paths(&mut destination.scenes, additional.scenes)?;
    merge_paths(&mut destination.textures, additional.textures)?;
    Ok(())
}

fn merge_paths(
    destination: &mut Box<[DemandLoadedAssetPath]>,
    additional: Box<[DemandLoadedAssetPath]>,
) -> Result<(), Failure> {
    let mut paths = std::mem::take(destination).into_vec();
    paths.extend(additional);
    paths.sort_unstable_by_key(|path| path.id);
    if paths
        .windows(2)
        .any(|pair| pair[0].id == pair[1].id && pair[0].path != pair[1].path)
    {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    paths.dedup_by_key(|path| path.id);
    *destination = paths.into_boxed_slice();
    Ok(())
}

fn merge_records<T: PartialEq>(
    destination: &mut Vec<T>,
    additional: Vec<T>,
    identifier: impl Fn(&T) -> AssetId,
) -> Result<(), Failure> {
    destination.extend(additional);
    destination.sort_unstable_by_key(&identifier);
    if destination
        .windows(2)
        .any(|pair| identifier(&pair[0]) == identifier(&pair[1]) && pair[0] != pair[1])
    {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    destination.dedup_by_key(|record| identifier(record));
    Ok(())
}

fn append_distinct<T: PartialEq>(destination: &mut Vec<T>, additional: Vec<T>) {
    for record in additional {
        if !destination.contains(&record) {
            destination.push(record);
        }
    }
}

fn preserve_retained_campaign_or_add_baseline_campaign(
    retained: &mut Vec<ScenarioCampaignRecord>,
    requested: AssetId,
    baseline: Option<&ScenarioCampaignRecord>,
) -> Result<(), Failure> {
    if retained.iter().any(|campaign| {
        campaign
            .scenarios
            .iter()
            .any(|scenario| scenario.id == requested)
    }) {
        return Ok(());
    }
    if let Some(campaign) = baseline {
        merge_records(retained, vec![campaign.clone()], |record| record.id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_data::world_scenario::CampaignScenarioRecord;

    #[test]
    fn retained_campaign_survives_changed_installation_and_baseline_campaign_fills_missing_source()
    {
        let requested = AssetId([1; 16]);
        let retained = ScenarioCampaignRecord {
            id: AssetId([2; 16]),
            name_key: AssetId([3; 16]),
            description_key: AssetId([4; 16]),
            scenarios: vec![CampaignScenarioRecord {
                id: requested,
                initial_state: Default::default(),
                map: AssetId([5; 16]),
                starting_zoo: AssetId([6; 16]),
                starting_cash_cents: 5000,
                difficulty: AssetId([7; 16]),
                name_key: AssetId([8; 16]),
                description_key: AssetId([9; 16]),
            }],
        };
        let mut changed = retained.clone();
        changed.scenarios[0].starting_cash_cents = 10000;
        let mut campaigns = vec![retained.clone()];
        preserve_retained_campaign_or_add_baseline_campaign(
            &mut campaigns,
            requested,
            Some(&changed),
        )
        .unwrap();
        assert_eq!(campaigns, vec![retained.clone()]);
        let mut empty = Vec::new();
        preserve_retained_campaign_or_add_baseline_campaign(&mut empty, requested, Some(&retained))
            .unwrap();
        assert_eq!(empty, vec![retained]);
    }
}
