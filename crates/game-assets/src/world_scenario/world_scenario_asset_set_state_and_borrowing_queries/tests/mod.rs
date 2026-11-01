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
