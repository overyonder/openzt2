use openzt2_environment::EnvironmentPlugin;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{document::WorldDefinitionDocument, environment::EnvironmentDefinition},
    world_scenario::{
        PhotoChallengeRule, StartingTopologyNodeRecord, StartingZooRecord, WorldScenarioDocument,
    },
    AssetId,
};

use openzt2_interface_state::photos::photo_album_types::PhotoAlbum;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::environment::environment_state_types::WorldEnvironment;

use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;

use crate::persistence::{
    persistence_failure_types::WorldSnapshotPersistenceFailed,
    slot_io::photo_baseline_snapshot_tests::{
        initialize_fixture_album_localization, loading_app, photo_fixture_catalogue,
        prepare_photo_snapshot_application, FIRST, PROFILE, SCENARIO, SECOND,
    },
};
use openzt2_game_assets::{
    application_lifecycle::GamePhase,
    asset_source::create_bevy_asset_source_for_enabled_z2f_archives,
    {
        world_definitions::{
            world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
            world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
        },
        world_scenario::{
            world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios,
            world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset,
        },
    },
};

use openzt2_world::world_spawn::world_hydration_completion::complete_or_fail_finished_world_hydration;
use openzt2_world::world_spawn::world_persistent_id_reservation::reserve_all_imported_world_persistent_ids;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_hydration_types::WorldHydration;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompletionSet;
use openzt2_world_state::world_spawn::world_loading_performance_attribution::WorldLoadingPerformanceAttribution;

struct EmptyArchiveFixture(PathBuf);

impl Drop for EmptyArchiveFixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn imported_ids_default_album_and_environment_keep_their_order_with_delayed_localization() {
    for delay_localization in [false, true] {
        let archive = EmptyArchiveFixture(std::env::temp_dir().join(format!(
            "openzt2-photo-baseline-{}-{}.z2f", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        )));
        zip::ZipWriter::new(fs::File::create_new(&archive.0).unwrap())
            .finish()
            .unwrap();
        let (_, archives) =
            create_bevy_asset_source_for_enabled_z2f_archives(vec![archive.0.clone()]).unwrap();
        let (mut app, root) = loading_app();
        app.add_plugins(EnvironmentPlugin)
            .add_message::<openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioCue>()
            .add_message::<openzt2_game_assets::effect::SpawnParticleEffectEmitter>()
            .add_message::<openzt2_game_assets::effect::StopParticleEffectsAttachedToEntity>()
            .insert_resource(archives)
            .init_resource::<WorldLoadingPerformanceAttribution>()
            .add_systems(
                Update,
                (
                    reserve_all_imported_world_persistent_ids,
                    complete_or_fail_finished_world_hydration.in_set(WorldLoadCompletionSet),
                )
                    .chain()
                    .in_set(GameSet::Intent),
            );
        app.world_mut()
            .entity_mut(root)
            .remove::<WorldLoadCompleted>();
        app.insert_resource(PersistentIdAllocator::new(root));

        let mut catalogue = photo_fixture_catalogue();
        catalogue.starting_zoos.push(StartingZooRecord {
            environment: SCENARIO,
            id: SCENARIO,
            profile: PROFILE,
            map: SCENARIO,
            camera_position_m: [0.; 3],
            camera_rotation_radians: [0.; 3],
            name: "baseline".into(),
            admissions_open: true,
            cash_cents: 5000,
            admission_cents: Some(250),
            fame_half_stars: 0,
            maximum_fame_percent_reached: None,
            start_tick: 1,
            absolute_day: 0,
            calendar: [2000, 1, 1],
            imported_seed: Some(17),
            entities: vec![],
            spawn_values: vec![],
            paths: vec![],
            topology_nodes: vec![StartingTopologyNodeRecord {
                persistent_id: 6,
                position_m: [0.; 3],
            }],
            fences: vec![],
        });
        let start = app
            .world_mut()
            .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
            .add(WorldScenarioDocumentAsset::from_test_document(catalogue));
        app.world_mut()
            .resource_mut::<WorldScenarios>()
            .queue_test_document(start.clone());
        let second = WorldScenarioDocument {
            photo_challenges: vec![PhotoChallengeRule {
                id: SECOND,
                instruction_key: SECOND,
                completion_key: SECOND,
                difficulty: 1,
            }],
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldScenarioDocumentAsset>>()
            .add(WorldScenarioDocumentAsset::from_test_document(second));
        app.world_mut()
            .resource_mut::<WorldScenarios>()
            .queue_test_document(handle);
        // Upstream geometry/value hydration has completed; ID reservation and
        // the completion notification still run through their production systems.
        let mut hydration =
            WorldHydration::new_pending_world_hydration(SCENARIO, start, SCENARIO, 0);
        hydration.record_terrain_hydration_completion();
        hydration.record_starting_topology_hydration_completion();
        hydration.record_starting_path_hydration_completion();
        hydration.record_authored_world_spawn_value_application_completion();
        app.world_mut().entity_mut(root).insert(hydration);
        let simulation_timing = app
            .world()
            .resource::<WorldDefinitions>()
            .get(app.world().resource::<Assets<WorldDefinitionAsset>>())
            .unwrap()
            .timing()
            .clone();
        let definitions = WorldDefinitionDocument {
            simulation_timing: Some(simulation_timing),
            environments: vec![EnvironmentDefinition {
                id: SCENARIO,
                family: SCENARIO,
                light_keyframes: vec![],
                fog_keyframes: vec![],
                sky_keyframes: vec![],
                weather: vec![],
                transitions: vec![],
                ambient: vec![],
                fog_samples: vec![],
                map_samples: vec![],
                visual_samples: vec![],
                light_samples: vec![],
                initial_weather: AssetId::default(),
                wind_mps: [0., 0.],
            }],
            ..default()
        };
        let handle = app
            .world_mut()
            .resource_mut::<Assets<WorldDefinitionAsset>>()
            .add(WorldDefinitionAsset::from_test_document(
                definitions.clone(),
            ));
        app.world_mut()
            .resource_mut::<WorldDefinitions>()
            .index_test_document(handle, &definitions);
        prepare_photo_snapshot_application(app.world_mut());
        if !delay_localization {
            initialize_fixture_album_localization(app.world_mut());
        }
        app.update();
        assert!(app.world().get::<WorldLoadCompleted>(root).is_some());
        assert!(app.world().get::<WorldHydration>(root).is_none());
        let mut albums = app
            .world_mut()
            .query_filtered::<(Entity, &PersistentId), With<PhotoAlbum>>();
        let (album, id) = albums.single(app.world()).unwrap();
        assert_eq!(*id, PersistentId(7));
        assert_eq!(
            app.world().get::<PhotoAlbum>(album).unwrap().profile,
            PROFILE
        );
        let mut environments = app
            .world_mut()
            .query_filtered::<&PersistentId, With<WorldEnvironment>>();
        assert_eq!(*environments.single(app.world()).unwrap(), PersistentId(8));
        assert_eq!(app.world().resource::<PersistentIdAllocator>().next(), 9);
        if delay_localization {
            assert!(app.world().get::<Name>(album).is_none());
            app.update();
            assert!(app
                .world()
                .resource::<Messages<WorldSnapshotLoadedFromSlot>>()
                .is_empty());
            assert!(matches!(
                app.world().resource::<NextState<GamePhase>>(),
                NextState::Unchanged
            ));
            initialize_fixture_album_localization(app.world_mut());
        }
        app.update();
        assert_eq!(
            app.world().get::<Name>(album).unwrap().as_str(),
            "Fixture Album"
        );
        assert_eq!(
            *app.world().resource::<State<GamePhase>>().get(),
            GamePhase::Loading
        );
        assert!(matches!(
            app.world().resource::<NextState<GamePhase>>(),
            NextState::Pending(GamePhase::InGame)
        ));
        assert_eq!(
            app.world()
                .resource::<Messages<WorldSnapshotLoadedFromSlot>>()
                .len(),
            1
        );
        assert!(app
            .world()
            .resource::<Messages<WorldSnapshotPersistenceFailed>>()
            .is_empty());
        let mut challenges = app
            .world_mut()
            .query::<(&PhotoChallenge, &PhotoChallengeProgress)>();
        for (id, expected_photo) in [(FIRST, 41), (SECOND, 42)] {
            let (_, progress) = challenges
                .iter(app.world())
                .find(|(challenge, _)| challenge.id == id)
                .unwrap();
            let photo = progress.completed_by.unwrap();
            assert_eq!(
                app.world().get::<PersistentId>(photo),
                Some(&PersistentId(expected_photo))
            );
            assert_eq!(
                app.world()
                    .get::<openzt2_interface_state::photos::photo_album_types::AlbumMember>(photo)
                    .unwrap()
                    .0,
                album
            );
        }
    }
}
