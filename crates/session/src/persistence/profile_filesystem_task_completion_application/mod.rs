use bevy::{
    prelude::*,
    tasks::{block_on, poll_once},
};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;

use openzt2_interface_state::input::input_types::GameActionInputBindings;
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;
use openzt2_interface_state::settings::display_settings_types::DisplaySettings;
use openzt2_zoo_state::progression::profile_challenge_types::ProfileChallengeCompletionCounts;

use openzt2_interface_state::persistence::profile_types::ProfileCreated;
use openzt2_interface_state::persistence::profile_types::ProfileDeleted;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_interface_state::persistence::profile_types::ProfileIndexReady;
use openzt2_interface_state::persistence::profile_types::ProfileOperationFailed;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::profile_types::ProfileSelected;

use super::profile_persistence_types::{
    ProfileFilesystemTask, ProfileFilesystemTaskCompletion, ProfileFilesystemTaskContext,
};

pub(super) fn apply_completed_profile_filesystem_tasks_to_canonical_profile_state(
    mut commands: Commands,
    mut profile_filesystem_tasks: Query<(
        Entity,
        &ProfileFilesystemTaskContext,
        &mut ProfileFilesystemTask,
    )>,
    mut profile_index: ResMut<ProfileIndex>,
    mut profile_options: ResMut<ProfileOptions>,
    mut scenario_variables: ResMut<
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    >,
    mut profile_challenge_completion_counts: ResMut<ProfileChallengeCompletionCounts>,
    mut audio_settings: ResMut<AudioSettings>,
    mut game_action_input_bindings: ResMut<GameActionInputBindings>,
    mut display_settings: ResMut<DisplaySettings>,
    mut graphics_settings: ResMut<GraphicsSettings>,
    mut profile_index_ready_messages: MessageWriter<ProfileIndexReady>,
    mut profile_created_messages: MessageWriter<ProfileCreated>,
    mut profile_selected_messages: MessageWriter<ProfileSelected>,
    mut profile_deleted_messages: MessageWriter<ProfileDeleted>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
) {
    for (task_entity, task_context, mut profile_filesystem_task) in &mut profile_filesystem_tasks {
        let Some(filesystem_task_result) = block_on(poll_once(&mut profile_filesystem_task.0))
        else {
            continue;
        };
        commands.entity(task_entity).despawn();

        match filesystem_task_result {
            Ok(ProfileFilesystemTaskCompletion::IndexLoaded {
                entries,
                selected: selected_profile_identifier,
            }) => {
                profile_index.profile_records = entries;
                profile_index.selected_profile_identifier = selected_profile_identifier;
                profile_index_ready_messages.write(ProfileIndexReady);
            }
            Ok(ProfileFilesystemTaskCompletion::Created { record, entries }) => {
                profile_index.profile_records = entries;
                profile_created_messages.write(ProfileCreated {
                    created_profile_identifier: record.profile_identifier,
                });
            }
            Ok(ProfileFilesystemTaskCompletion::Selected {
                profile,
                options,
                challenge_progress,
                scenario_variables: loaded_scenario_variables,
                audio,
                bindings,
                display,
                graphics,
            }) => {
                *profile_options = options;
                *scenario_variables = loaded_scenario_variables;
                *profile_challenge_completion_counts = challenge_progress;
                *audio_settings = audio;
                *game_action_input_bindings = bindings;
                *display_settings = display;
                *graphics_settings = graphics;
                profile_index.selected_profile_identifier = Some(profile);
                profile_selected_messages.write(ProfileSelected {
                    selected_profile_identifier: profile,
                });
            }
            Ok(ProfileFilesystemTaskCompletion::Deleted { profile, entries }) => {
                profile_index.profile_records = entries;
                profile_deleted_messages.write(ProfileDeleted {
                    deleted_profile_identifier: profile,
                });
            }
            Ok(ProfileFilesystemTaskCompletion::SettingsSaved) => {}
            Err(persistence_failure) => {
                profile_operation_failure_messages.write(ProfileOperationFailed {
                    affected_profile_identifier: task_context.0,
                    failure_reason: persistence_failure,
                });
            }
        }
    }
}
