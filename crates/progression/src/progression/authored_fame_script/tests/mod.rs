use std::{
    io,
    path::Path,
    time::{Duration, Instant},
};

use bevy::{
    app::TaskPoolPlugin,
    asset::{
        io::{memory::MemoryAssetReader, AssetSourceBuilder, AssetSourceId},
        AssetApp, AssetPlugin,
    },
    ecs::message::Messages,
};
use openzt2_game_assets::{
    asset_source::{create_bevy_asset_source_for_enabled_z2f_archives, ArchiveOverlayChanged},
    lua_script::LuaScriptAssetLoadingPlugin,
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted, world_membership_types::WorldMember,
    zoo_entrance_anchor_synchronization::zoo_entrance_from_authored_gate_anchor,
};
use openzt2_zoo_state::{
    animal_lifecycle::types::{Animal, SpeciesHandle},
    progression::{
        award_and_progression_fact_types::{ProgressionFactChanged, ScenarioAwardPointTotal},
        fame_types::ZooMaximumFame,
        profile_challenge_types::TotalAnimalsReleasedCount,
        rating_types::ZooRating,
        unlock_types::UnlockedCatalogueDefinitionSet,
    },
};

use super::super::{
    fame_progression::{publish_world_maximum_fame, update_authored_fame},
    zoo_rating_input_types::{AnimalWelfareRating, GuestRatingFacts, LivingAnimalPopulation},
    zoo_rating_recomputation::reduce_living_animal_population_and_welfare,
};
use super::*;

const FAME_FIXTURE: &[u8] = br#"
function adjustZooFame(status)
    local animals = #findType("animal")
    local nextStars = animals * 2
    status:ZT_SET_FAME_HALFSTARS({nextStars, animals * 20})
    if nextStars > status.currentHalfStars then
        local gates = queryObject("BFGManager"):BFG_GET_ENTITIES("entrance")
        gates[1].value:BFG_ENTITY_MORPH_TO_NEW_ENTITY({"fixture-gate-upgraded"})
    end
end
"#;

#[test]
fn authored_fame_changes_as_the_zoo_grows() -> io::Result<()> {
    // The fixture has no includes, so only the asset loader needs a script file.
    let archives_path =
        std::env::temp_dir().join(format!("openzt2-fame-test-{}.z2f", std::process::id()));
    let empty_archive = b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0";
    std::fs::write(&archives_path, empty_archive)?;
    let source = create_bevy_asset_source_for_enabled_z2f_archives(vec![archives_path.clone()]);
    std::fs::remove_file(archives_path)?;
    let (_, archives) = source?;

    let reader = MemoryAssetReader::default();
    reader
        .root
        .insert_asset(Path::new(FAME_SCRIPT_PATH), FAME_FIXTURE.to_vec());
    let mut app = App::new();
    app.register_asset_source(
        AssetSourceId::Default,
        AssetSourceBuilder::new(move || Box::new(reader.clone())),
    )
    .add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin {
            watch_for_changes_override: Some(false),
            use_asset_processor_override: Some(false),
            ..default()
        },
    ))
    .insert_resource(archives)
    .add_message::<ArchiveOverlayChanged>()
    .add_plugins(LuaScriptAssetLoadingPlugin)
    .init_asset::<WorldDefinitionAsset>()
    .init_resource::<WorldDefinitions>()
    .register_required_components::<WorldRoot, ScenarioAwardPointTotal>()
    .register_required_components::<WorldRoot, TotalAnimalsReleasedCount>()
    .register_required_components::<WorldRoot, UnlockedCatalogueDefinitionSet>()
    .add_message::<FameGateAppearanceRequested>()
    .add_message::<ProgressionFactChanged>();
    let document =
        openzt2_game_data::world_definitions::document::WorldDefinitionDocument::default();
    let world = app.world_mut();
    let definitions = world
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    world
        .resource_mut::<WorldDefinitions>()
        .index_test_document(definitions, &document);
    let handle = world
        .resource::<AssetServer>()
        .load::<LuaScriptAsset>(FAME_SCRIPT_PATH);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !app
        .world()
        .resource::<Assets<LuaScriptAsset>>()
        .contains(handle.id())
    {
        assert!(
            Instant::now() < deadline,
            "fixture script asset did not load"
        );
        app.update();
        std::thread::yield_now();
    }
    app.add_systems(
        Update,
        (
            reduce_living_animal_population_and_welfare,
            update_authored_fame,
            publish_world_maximum_fame,
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
            Fame::default(),
            ZooMaximumFame::default(),
            LivingAnimalPopulation::default(),
            AnimalWelfareRating::default(),
            GuestRatingFacts::default(),
            super::super::authored_award_programs::AuthoredAwardPoints::default(),
            ZooRating::default(),
            AuthoredFameScript {
                handle,
                path: FAME_SCRIPT_PATH.to_owned(),
                lua: None,
                modules: BTreeSet::new(),
            },
        ))
        .id();
    let other_root = app
        .world_mut()
        .spawn(WorldRoot {
            scenario: AssetId::default(),
        })
        .id();
    let species = SpeciesHandle {
        species: AssetId::from_key("fixture-animal"),
    };
    app.world_mut()
        .spawn((Animal, species, WorldMember { root: other_root }));
    let gate = app
        .world_mut()
        .spawn((
            zoo_entrance_from_authored_gate_anchor(Vec3::ZERO),
            WorldMember { root },
            DefinitionId(AssetId::from_key("fixture-gate")),
        ))
        .id();

    app.update();
    assert_eq!(
        app.world().get::<Fame>(root).unwrap().current_percent,
        Some(0.0)
    );
    assert_eq!(
        app.world().get::<ZooMaximumFame>(root),
        Some(&ZooMaximumFame(Some(0.0)))
    );
    assert_eq!(fame_changes(&mut app), [ProgressionFactChanged::Fame]);

    app.world_mut()
        .spawn((Animal, species, WorldMember { root }));
    app.update();
    assert_eq!(app.world().get::<Fame>(root).unwrap().half_stars, 2);
    assert_eq!(
        app.world().get::<Fame>(root).unwrap().current_percent,
        Some(20.0)
    );
    assert_eq!(
        app.world().get::<ZooMaximumFame>(root),
        Some(&ZooMaximumFame(Some(20.0)))
    );
    assert_eq!(fame_changes(&mut app), [ProgressionFactChanged::Fame]);
    let gate_requests = app
        .world_mut()
        .resource_mut::<Messages<FameGateAppearanceRequested>>()
        .drain()
        .collect::<Vec<_>>();
    assert_eq!(gate_requests.len(), 1);
    assert_eq!(gate_requests[0].gate, gate);
    assert_eq!(
        gate_requests[0].definition,
        AssetId::from_key("fixture-gate-upgraded")
    );

    app.update();
    assert!(fame_changes(&mut app).is_empty());
    Ok(())
}

fn fame_changes(app: &mut App) -> Vec<ProgressionFactChanged> {
    app.world_mut()
        .resource_mut::<Messages<ProgressionFactChanged>>()
        .drain()
        .collect()
}
