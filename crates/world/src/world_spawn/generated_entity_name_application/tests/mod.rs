use super::*;
use openzt2_game_data::{
    localization::{LocalizationCatalog, LocalizationEntry},
    world_definitions::{
        document::WorldDefinitionDocument,
        entity_naming::{EntityNamingDefinition, GeneratedEntityNameDefinition},
    },
    AssetId,
};

#[test]
fn restored_high_water_names_new_entities_once_and_preserves_explicit_names() {
    let definition = AssetId::from_key("acacia");
    let foliage = AssetId::from_key("foliage");
    let tree = AssetId::from_key("tree");
    let label = AssetId::from_key("entityname:foliage");
    let document = WorldDefinitionDocument {
        entity_naming: vec![EntityNamingDefinition {
            id: definition,
            counted_tags: vec![definition, tree, foliage],
            generated_name: Some(GeneratedEntityNameDefinition {
                localization_key: label,
                counter_tag: foliage,
            }),
        }],
        ..default()
    };
    let mut assets = Assets::default();
    let handle = assets.add(WorldDefinitionAsset::from_test_document(document.clone()));
    let mut active = WorldDefinitions::default();
    active.index_test_document(handle, &document);
    let mut localization_assets = Assets::default();
    let mut localization = LocalizationPrecedenceIndex::default();
    localization.index_test_catalogue(
        &mut localization_assets,
        LocalizationCatalog {
            localization_entries: vec![LocalizationEntry {
                localization_entry_identifier: label,
                authored_localization_key: "entityname:foliage".to_owned(),
                plain_localized_text: "Foliage".to_owned(),
                localized_text_format_tokens: vec![],
                localized_rich_content_nodes: vec![],
                localized_text_presentation: default(),
            }],
            ..default()
        },
    );
    let mut app = App::new();
    app.insert_resource(assets)
        .insert_resource(active)
        .insert_resource(localization_assets)
        .insert_resource(localization)
        .add_systems(
            Update,
            (
                initialize_world_entity_name_counters,
                register_and_name_spawned_world_entities,
            )
                .chain(),
        );
    let root = app
        .world_mut()
        .spawn((
            WorldRoot {
                scenario: AssetId::default(),
            },
            WorldLoadCompleted,
            EntityNameTagCounters::from_saved_entries([(foliage, 198)]).unwrap(),
        ))
        .id();
    let restored = app
        .world_mut()
        .spawn((
            WorldMember { root },
            DefinitionId(definition),
            PersistentId(1),
            EntityNameRegistered,
        ))
        .id();
    let unnamed = app
        .world_mut()
        .spawn((
            WorldMember { root },
            DefinitionId(definition),
            PersistentId(3),
        ))
        .id();
    let explicit = app
        .world_mut()
        .spawn((
            WorldMember { root },
            DefinitionId(definition),
            PersistentId(2),
            Name::new("Old oak"),
        ))
        .id();
    app.update();
    app.update();
    assert_eq!(
        app.world()
            .get::<EntityNameTagCounters>(root)
            .unwrap()
            .value(foliage),
        Some(200)
    );
    assert_eq!(
        app.world().get::<Name>(unnamed).unwrap().as_str(),
        "Foliage 200"
    );
    assert_eq!(
        app.world().get::<Name>(explicit).unwrap().as_str(),
        "Old oak"
    );
    assert!(app.world().get::<Name>(restored).is_none());
    assert!(app.world().get::<EntityNameRegistered>(unnamed).is_some());
}
