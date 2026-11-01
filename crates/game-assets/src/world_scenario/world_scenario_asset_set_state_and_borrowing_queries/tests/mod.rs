use super::*;
use openzt2_game_data::world_scenario::{WorldMapRecord, WorldScenarioDocument};

#[test]
fn selected_document_resolves_colliding_records_without_changing_map_catalogue() {
    let identifier = AssetId([1; 16]);
    let original_camera = AssetId([2; 16]);
    let saved_camera = AssetId([3; 16]);
    let mut original = WorldScenarioDocument::default();
    original.maps.push(WorldMapRecord {
        id: identifier,
        catalogue_order: 0,
        name_key: identifier,
        location: identifier,
        biome: identifier,
        biome_key: identifier,
        size_key: identifier,
        description_key: None,
        thumbnail: identifier,
        environment: identifier,
        camera: original_camera,
        terrain: identifier,
        starting_zoo: identifier,
        expansion_pack_filter_identifier: 0,
        supported_game_modes: Default::default(),
    });
    let mut saved = original.clone();
    saved.maps[0].camera = saved_camera;
    let mut assets = Assets::<WorldScenarioDocumentAsset>::default();
    let original_handle = assets.add(WorldScenarioDocumentAsset::from_test_document(
        original.clone(),
    ));
    let saved_handle = assets.add(WorldScenarioDocumentAsset::from_test_document(saved));
    let mut catalogue = WorldScenarios::default();
    catalogue.index_test_document(original_handle.clone(), &original);
    let normal = catalogue.get(&assets).unwrap();
    let selected = normal.with_document(&saved_handle).unwrap();
    assert_eq!(selected.map(identifier).unwrap().camera, saved_camera);
    assert_eq!(selected.handle_for_map(identifier), Some(&saved_handle));
    assert_eq!(normal.map(identifier).unwrap().camera, original_camera);
    assert_eq!(normal.handle_for_map(identifier), Some(&original_handle));
    assert_eq!(
        catalogue
            .get(&assets)
            .unwrap()
            .maps()
            .map(|map| map.camera)
            .collect::<Vec<_>>(),
        vec![original_camera]
    );
}

#[test]
fn retained_scenarios_and_campaign_aliases_resolve_without_catalogue_replacement() {
    use openzt2_game_data::world_scenario::{ScenarioDefinitionRecord, ScenarioObjectiveRecord};

    let stock_id = AssetId::from_key("stock-scenario");
    let saved_id = AssetId::from_key("saved-scenario");
    let campaign_alias = AssetId::from_key("campaign-entry");
    let original_objective = AssetId::from_key("original-rule");
    let retained_objective = AssetId::from_key("retained-rule");
    let scenario = |id, objective| ScenarioDefinitionRecord {
        id,
        name_key: id,
        description_key: id,
        objectives: vec![ScenarioObjectiveRecord {
            id: objective,
            name: None,
            category: id,
            text_key: id,
            success_text_key: id,
            failure_text_key: id,
            prerequisite_objectives: vec![],
            hidden: false,
        }],
        objective_groups: vec![],
        time_limit_ticks: 0,
        evaluation_interval_seconds: 4.0,
        flags: Default::default(),
        ui_modifications: vec![],
    };
    let mut original = WorldScenarioDocument::default();
    original
        .scenarios
        .push(scenario(stock_id, original_objective));
    let mut retained = WorldScenarioDocument::default();
    retained
        .scenarios
        .push(scenario(stock_id, retained_objective));
    retained
        .scenarios
        .push(scenario(saved_id, retained_objective));
    let mut assets = Assets::<WorldScenarioDocumentAsset>::default();
    let original_handle = assets.add(WorldScenarioDocumentAsset::from_test_document(
        original.clone(),
    ));
    let retained_handle = assets.add(WorldScenarioDocumentAsset::from_test_document(retained));
    let mut catalogue = WorldScenarios::default();
    catalogue.index_test_document(original_handle.clone(), &original);
    catalogue.scenarios.insert(
        campaign_alias,
        ScenarioDocumentHandle {
            record: stock_id,
            handle: original_handle.clone(),
        },
    );
    let normal = catalogue.get(&assets).unwrap();
    let selected = normal.with_document(&retained_handle).unwrap();
    for id in [stock_id, saved_id, campaign_alias] {
        assert_eq!(
            selected.scenario(id).unwrap().objectives[0].id,
            retained_objective
        );
        assert_eq!(selected.handle_for_scenario(id), Some(&retained_handle));
        assert!(std::ptr::eq(
            selected.document_for_scenario(id).unwrap(),
            assets.get(&retained_handle).unwrap()
        ));
    }
    assert_eq!(
        normal.scenario(stock_id).unwrap().objectives[0].id,
        original_objective
    );
    assert_eq!(normal.scenario(campaign_alias).unwrap().id, stock_id);
    assert_eq!(
        normal.handle_for_scenario(campaign_alias),
        Some(&original_handle)
    );
    assert!(normal.scenario(saved_id).is_none());
    assert_eq!(
        normal
            .scenarios()
            .map(|scenario| scenario.id)
            .collect::<Vec<_>>(),
        vec![stock_id]
    );
}
