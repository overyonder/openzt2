use super::*;
use openzt2_zoo_state::progression::profile_scenario_variables::{
    PersistProfileScenarioVariables, ProfileScenarioVariables,
};

#[test]
fn dirty_outgoing_variables_are_saved_in_the_selection_transaction() {
    let root = std::env::temp_dir().join(format!(
        "openzt2-profile-switch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let paths = PersistenceFilesystemPaths::new(root.clone());
    let a = AssetId([1; 16]);
    let b = AssetId([2; 16]);
    let key = AssetId::from_key("campaign");
    let records: Vec<_> = [a, b]
        .into_iter()
        .map(|id| ProfileRecord {
            profile_identifier: id,
            profile_display_name: format!("profile {}", id.0[0]),
            last_played_unix_timestamp_milliseconds: 0,
        })
        .collect();
    let initial =
        super::super::profile_index_encoding::encode_profile_index_for_test(&records, Some(a))
            .unwrap();
    atomically_write_and_sync_file(&paths.profile_index_file_path(), &initial).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(paths.clone())
        .insert_resource(ProfileIndex {
            profile_records: records,
            selected_profile_identifier: Some(a),
        })
        .insert_resource(ProfileOptions {
            profile_identifier: a,
            ..Default::default()
        })
        .init_resource::<ProfileScenarioVariables>()
        .init_resource::<ProfileChallengeCompletionCounts>()
        .init_resource::<AudioSettings>()
        .init_resource::<GameActionInputBindings>()
        .init_resource::<DisplaySettings>()
        .init_resource::<GraphicsSettings>()
        .add_message::<SelectProfile>()
        .add_message::<ProfileOperationFailed>()
        .add_message::<PersistProfileScenarioVariables>()
        .add_systems(
            Update,
            (
                |mut variables: ResMut<ProfileScenarioVariables>| {
                    variables
                        .set(AssetId::from_key("campaign"), Some("completed"))
                        .unwrap();
                },
                begin_selecting_requested_profiles,
                begin_persisting_changed_active_profile_options,
            )
                .chain(),
        );
    app.world_mut().write_message(SelectProfile {
        requested_profile_identifier: b,
    });
    app.update();
    let world = app.world_mut();
    let mut query = world.query::<&mut ProfileFilesystemTask>();
    let mut task = query.single_mut(world).unwrap();
    let result = bevy::tasks::block_on(&mut task.0).unwrap();
    assert!(
        matches!(result, ProfileFilesystemTaskCompletion::Selected { profile, .. } if profile == b)
    );
    let (stored, selected) =
        read_stored_profile_index_or_empty(&paths.profile_index_file_path()).unwrap();
    assert_eq!(selected, Some(b));
    assert_eq!(stored[0].scenario_variables.get(key), Some("completed"));
    assert_eq!(stored[1].scenario_variables.get(key), None);
    std::fs::remove_dir_all(root).unwrap();
}
