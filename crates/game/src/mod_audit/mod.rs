//! Opt-in Freeform smoke test. Set `OPENZT2_MOD_AUDIT=1` on a build with the
//! `mod-audit` feature and use a disposable `XDG_DATA_HOME` for the run.
//! `OPENZT2_MOD_AUDIT_ANIMAL` optionally checks adoption and presentation of a
//! land animal on the chosen map. `OPENZT2_MOD_AUDIT_ANIMAL_BATCH` checks one
//! adult land variant per archive with an eligible winning model.

mod animal;
mod animal_batch;

use std::time::{Duration, Instant};

use bevy::{
    app::AppExit,
    asset::{io::AssetSourceId, AssetPath, UntypedAssetLoadFailedEvent},
    gltf::Gltf,
    prelude::*,
};
use openzt2_game_assets::{
    animation::animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
    application_lifecycle::GamePhase,
    asset_source::AssetArchives,
    audio::audio_asset_types::AudioAsset,
    behavior::behavior_asset_types::BehaviorDocumentAsset,
    localization::localization_asset_types::LocalizationAsset,
    lua_script::lua_script_asset_loading::LuaScriptAsset,
    scene_prefab::ScenePrefabAsset,
    species::species_asset_types::SpeciesAsset,
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    world_scenario::{
        world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
        world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
    },
};
use openzt2_game_data::world_scenario::WorldMapSupportedGameModeFlags;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_session::shell::shell_navigation_request_types::SplashFinished;
use openzt2_world_state::{
    game_session_types::WorldSessionMode,
    world_spawn::{
        world_load_failure::WorldLoadFailed, world_load_request_acceptance::BeginWorldLoad,
    },
};

#[derive(Resource)]
struct Audit {
    start: Instant,
    requested_map: bool,
    entered_world: Option<Instant>,
    acknowledged_splash: bool,
    last_phase: Option<GamePhase>,
}

#[derive(Resource)]
struct ArchiveNames(Vec<std::path::PathBuf>);

pub(super) fn install(app: &mut App) {
    if std::env::var_os("OPENZT2_MOD_AUDIT").is_none() {
        return;
    }
    let archives = app
        .world()
        .resource::<AssetArchives>()
        .enabled_archive_statuses();
    for archive in &archives {
        if !archive.is_required_base_archive {
            println!("mod-audit\tmod\t{}", archive.archive_name.display());
        }
    }
    app.insert_resource(ArchiveNames(
        archives
            .into_iter()
            .map(|archive| archive.archive_name)
            .collect(),
    ))
    .insert_resource(Audit {
        start: Instant::now(),
        requested_map: false,
        entered_world: None,
        acknowledged_splash: false,
        last_phase: None,
    })
    .add_systems(Last, (report_asset_failures, exercise_freeform))
    .add_systems(
        Last,
        (
            report_loaded::<SpeciesAsset>,
            report_loaded::<WorldDefinitionAsset>,
            report_loaded::<WorldScenarioDocumentAsset>,
            report_loaded::<UiDocumentAsset>,
            report_loaded::<BehaviorDocumentAsset>,
            report_loaded::<ScenePrefabAsset>,
            report_loaded::<Gltf>,
            report_loaded::<Image>,
            report_loaded::<AnimationSetAsset>,
            report_loaded::<AnimationClipSourceAsset>,
            report_loaded::<LuaScriptAsset>,
            report_loaded::<LocalizationAsset>,
            report_loaded::<AudioAsset>,
            report_loaded::<TerrainAsset>,
        ),
    );
    animal::install(app);
    animal_batch::install(app);
}

fn archive_name<'a>(
    path: &AssetPath<'_>,
    archives: &AssetArchives,
    names: &'a ArchiveNames,
) -> &'a str {
    if *path.source() != AssetSourceId::Default {
        return "";
    }
    archives
        .resolved_winning_entry(path.path())
        .and_then(|entry| names.0.get(entry.archive_priority_index)?.to_str())
        .unwrap_or_default()
}

fn report_asset_failures(
    mut events: MessageReader<UntypedAssetLoadFailedEvent>,
    archives: Res<AssetArchives>,
    names: Res<ArchiveNames>,
) {
    for event in events.read() {
        let owner = archive_name(&event.path, &archives, &names);
        println!(
            "mod-audit\tfailed\t{owner}\t{}\t{:?}",
            event.path,
            event.error.to_string()
        );
    }
}

fn report_loaded<T: Asset>(
    mut events: MessageReader<AssetEvent<T>>,
    server: Res<AssetServer>,
    archives: Res<AssetArchives>,
    names: Res<ArchiveNames>,
) {
    for event in events.read() {
        let AssetEvent::LoadedWithDependencies { id } = event else {
            continue;
        };
        if let Some(path) = server.get_path(*id) {
            let owner = archive_name(&path, &archives, &names);
            println!(
                "mod-audit\tloaded\t{owner}\t{path}\t{}",
                std::any::type_name::<T>()
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn exercise_freeform(
    mut audit: ResMut<Audit>,
    phase: Res<State<GamePhase>>,
    mut next_phase: ResMut<NextState<GamePhase>>,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    profile: Res<ProfileIndex>,
    animal_probe: Option<Res<animal::AnimalProbe>>,
    animal_batch: Option<Res<animal_batch::AnimalBatch>>,
    mut splash: MessageWriter<SplashFinished>,
    mut load: MessageWriter<BeginWorldLoad>,
    mut failures: MessageReader<WorldLoadFailed>,
    mut exit: MessageWriter<AppExit>,
) {
    if audit.last_phase != Some(*phase.get()) {
        println!("mod-audit\tphase\t{:?}", phase.get());
        audit.last_phase = Some(*phase.get());
    }
    if let Some(failure) = failures.read().next() {
        println!("mod-audit\tworld-failed\t{:?}", failure.reason);
        exit.write(AppExit::error());
        return;
    }
    if audit.start.elapsed() >= Duration::from_secs(240) {
        println!("mod-audit\ttimeout\t{:?}", phase.get());
        exit.write(AppExit::error());
        return;
    }
    match phase.get() {
        GamePhase::Boot
            if !audit.acknowledged_splash && audit.start.elapsed() >= Duration::from_secs(12) =>
        {
            audit.acknowledged_splash = true;
            println!("mod-audit\tsplash-acknowledged");
            splash.write(SplashFinished);
        }
        GamePhase::MainMenu if !audit.requested_map => {
            let Some(profile) = profile.selected_profile_identifier else {
                return;
            };
            let Some(scenarios) = scenarios.get(&scenario_assets) else {
                return;
            };
            let Some(map) = scenarios
                .maps()
                .filter(|map| {
                    map.supported_game_modes
                        .contains_all(WorldMapSupportedGameModeFlags::FREEFORM)
                        && scenarios.has_terrain(map.terrain)
                })
                .min_by_key(|map| map.catalogue_order)
            else {
                return;
            };
            audit.requested_map = true;
            println!("mod-audit\tworld-request\t{:?}", map.id);
            load.write(BeginWorldLoad {
                saved_world_document: None,
                mode_document: None,
                operation: None,
                scenario: map.id,
                mode: WorldSessionMode::Freeform,
                profile,
                starting_cash_cents: None,
            });
            next_phase.set(GamePhase::Loading);
        }
        GamePhase::InGame => {
            let entered = audit.entered_world.get_or_insert_with(|| {
                println!("mod-audit\tin-game");
                Instant::now()
            });
            if animal_batch.as_ref().is_some_and(|batch| !batch.finished()) {
                return;
            }
            if let Some(probe) = animal_probe.as_ref().filter(|probe| !probe.presented()) {
                if entered.elapsed() >= Duration::from_secs(45) {
                    println!("mod-audit\tanimal-timeout\t{}", probe.stage());
                    exit.write(AppExit::error());
                }
            } else if entered.elapsed() >= Duration::from_secs(12) {
                println!("mod-audit\tcomplete");
                exit.write(AppExit::Success);
            }
        }
        _ => {}
    }
}
