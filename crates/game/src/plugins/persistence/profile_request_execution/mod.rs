use bevy::{prelude::*, tasks::IoTaskPool};
use openzt2_game_data::AssetId;

use crate::plugins::{
    input::input_types::GameActionInputBindings,
    progression::profile_challenge_types::ProfileChallengeCompletionCounts,
    settings::{
        audio_settings_types::AudioSettings, display_settings_types::DisplaySettings,
        graphics_settings_types::GraphicsSettings,
    },
};

use super::{
    durable_filesystem_operations::{
        atomically_write_and_sync_file, read_file_with_maximum_byte_count,
    },
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    profile_identifier_generation::generate_unique_profile_identifier_and_timestamp_from_display_name_and_existing_profiles,
    profile_index_encoding::{
        decode_stored_profile_index, encode_stored_profile_index,
        read_stored_profile_index_or_empty, MAXIMUM_PROFILE_COUNT,
        MAXIMUM_PROFILE_DISPLAY_NAME_BYTE_COUNT, MAXIMUM_PROFILE_INDEX_BYTE_COUNT,
    },
    profile_persistence_types::{
        ProfileFilesystemTask, ProfileFilesystemTaskCompletion, ProfileFilesystemTaskContext,
        StoredProfileRecordAndSettings,
    },
    profile_types::{
        CreateProfile, DeleteProfile, LoadProfileIndex, ProfileFailure, ProfileIndex,
        ProfileOperationFailed, ProfileOptions, ProfileRecord, SelectProfile,
    },
};

pub(super) fn begin_loading_profile_index(
    mut commands: Commands,
    mut profile_index_load_requests: MessageReader<LoadProfileIndex>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    active_profile_filesystem_tasks: Query<(), With<ProfileFilesystemTask>>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
) {
    let mut operation_started = false;
    for _ in profile_index_load_requests.read() {
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::Io,
            });
            continue;
        };
        if operation_started || !active_profile_filesystem_tasks.is_empty() {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::Busy,
            });
            continue;
        }
        let profile_index_file_path = persistence_filesystem_paths.profile_index_file_path();
        operation_started = true;
        commands.spawn((
            ProfileFilesystemTaskContext(None),
            ProfileFilesystemTask(IoTaskPool::get().spawn(async move {
                match read_file_with_maximum_byte_count(
                    &profile_index_file_path,
                    MAXIMUM_PROFILE_INDEX_BYTE_COUNT,
                ) {
                    Ok(profile_index_bytes) => {
                        let (stored_profile_records_and_settings, selected_profile_identifier) =
                            decode_stored_profile_index(&profile_index_bytes)?;
                        let profile_records = stored_profile_records_and_settings
                            .into_iter()
                            .map(|stored_profile| stored_profile.record)
                            .collect();
                        Ok(ProfileFilesystemTaskCompletion::IndexLoaded {
                            entries: profile_records,
                            selected: selected_profile_identifier,
                        })
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        Ok(ProfileFilesystemTaskCompletion::IndexLoaded {
                            entries: Vec::new(),
                            selected: None,
                        })
                    }
                    Err(_) => Err(ProfileFailure::Io),
                }
            })),
        ));
    }
}

pub(super) fn begin_creating_requested_profiles(
    mut commands: Commands,
    mut profile_creation_requests: MessageReader<CreateProfile>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_index: Res<ProfileIndex>,
    active_profile_filesystem_tasks: Query<(), With<ProfileFilesystemTask>>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
) {
    let mut operation_started = false;
    for profile_creation_request in profile_creation_requests.read() {
        let normalized_profile_display_name = profile_creation_request
            .requested_profile_display_name
            .trim();
        if normalized_profile_display_name.is_empty()
            || normalized_profile_display_name.len() > MAXIMUM_PROFILE_DISPLAY_NAME_BYTE_COUNT
            || normalized_profile_display_name
                .chars()
                .any(char::is_control)
        {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::InvalidName,
            });
            continue;
        }
        if profile_index.profile_records.iter().any(|profile_record| {
            profile_record
                .profile_display_name
                .eq_ignore_ascii_case(normalized_profile_display_name)
        }) {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::DuplicateName,
            });
            continue;
        }
        if profile_index.profile_records.len() >= MAXIMUM_PROFILE_COUNT {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::CapacityExceeded,
            });
            continue;
        }
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::Io,
            });
            continue;
        };
        if operation_started || !active_profile_filesystem_tasks.is_empty() {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: None,
                failure_reason: ProfileFailure::Busy,
            });
            continue;
        }

        let requested_profile_display_name = normalized_profile_display_name.to_owned();
        let (profile_identifier, profile_creation_unix_time_milliseconds) =
            generate_unique_profile_identifier_and_timestamp_from_display_name_and_existing_profiles(
                &requested_profile_display_name,
                &profile_index.profile_records,
            );
        let new_profile_record = ProfileRecord {
            profile_identifier,
            profile_display_name: requested_profile_display_name,
            last_played_unix_timestamp_milliseconds: profile_creation_unix_time_milliseconds,
        };
        let mut updated_profile_records = profile_index.profile_records.clone();
        updated_profile_records.push(new_profile_record.clone());
        updated_profile_records.sort_by_key(|profile_record| profile_record.profile_identifier.0);
        let profile_index_file_path = persistence_filesystem_paths.profile_index_file_path();
        let selected_profile_identifier = profile_index.selected_profile_identifier;
        operation_started = true;
        commands.spawn((
            ProfileFilesystemTaskContext(Some(profile_identifier)),
            ProfileFilesystemTask(IoTaskPool::get().spawn(async move {
                let new_profile_options = ProfileOptions {
                    profile_identifier,
                    locale_identifier: AssetId::default(),
                    controller_input_enabled: true,
                };
                let mut stored_profile_records_and_settings =
                    read_stored_profile_index_or_empty(&profile_index_file_path)?.0;
                if stored_profile_records_and_settings
                    .iter()
                    .any(|stored_profile| {
                        stored_profile.record.profile_identifier == profile_identifier
                            || stored_profile
                                .record
                                .profile_display_name
                                .eq_ignore_ascii_case(&new_profile_record.profile_display_name)
                    })
                {
                    return Err(ProfileFailure::DuplicateName);
                }
                stored_profile_records_and_settings.push(StoredProfileRecordAndSettings {
                    record: new_profile_record.clone(),
                    options: new_profile_options,
                    challenge_progress: ProfileChallengeCompletionCounts::default(),
                    audio: AudioSettings::default(),
                    bindings: GameActionInputBindings::default(),
                    display: DisplaySettings::default(),
                    graphics: GraphicsSettings::default(),
                });
                stored_profile_records_and_settings
                    .sort_by_key(|stored_profile| stored_profile.record.profile_identifier.0);
                let encoded_profile_index_bytes = encode_stored_profile_index(
                    &stored_profile_records_and_settings,
                    selected_profile_identifier,
                )?;
                atomically_write_and_sync_file(
                    &profile_index_file_path,
                    &encoded_profile_index_bytes,
                )
                .map_err(|_| ProfileFailure::Io)?;
                Ok(ProfileFilesystemTaskCompletion::Created {
                    record: new_profile_record,
                    entries: updated_profile_records,
                })
            })),
        ));
    }
}

pub(super) fn begin_selecting_requested_profiles(
    mut commands: Commands,
    mut profile_selection_requests: MessageReader<SelectProfile>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_index: Res<ProfileIndex>,
    active_profile_filesystem_tasks: Query<(), With<ProfileFilesystemTask>>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
) {
    let mut operation_started = false;
    for profile_selection_request in profile_selection_requests.read() {
        if !profile_index.profile_records.iter().any(|profile_record| {
            profile_record.profile_identifier
                == profile_selection_request.requested_profile_identifier
        }) {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_selection_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Missing,
            });
            continue;
        }
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_selection_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Io,
            });
            continue;
        };
        if operation_started || !active_profile_filesystem_tasks.is_empty() {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_selection_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Busy,
            });
            continue;
        }
        let requested_profile_identifier = profile_selection_request.requested_profile_identifier;
        let profile_index_file_path = persistence_filesystem_paths.profile_index_file_path();
        operation_started = true;
        commands.spawn((
            ProfileFilesystemTaskContext(Some(requested_profile_identifier)),
            ProfileFilesystemTask(IoTaskPool::get().spawn(async move {
                let (stored_profile_records_and_settings, _) =
                    read_stored_profile_index_or_empty(&profile_index_file_path)?;
                let selected_profile_options = stored_profile_records_and_settings
                    .iter()
                    .find(|stored_profile| {
                        stored_profile.record.profile_identifier == requested_profile_identifier
                    })
                    .map(|stored_profile| stored_profile.options)
                    .ok_or(ProfileFailure::Missing)?;
                let selected_profile_challenge_progress = stored_profile_records_and_settings
                    .iter()
                    .find(|stored_profile| {
                        stored_profile.record.profile_identifier == requested_profile_identifier
                    })
                    .map(|stored_profile| stored_profile.challenge_progress)
                    .ok_or(ProfileFailure::Missing)?;
                let selected_stored_profile = stored_profile_records_and_settings
                    .iter()
                    .find(|stored_profile| {
                        stored_profile.record.profile_identifier == requested_profile_identifier
                    })
                    .ok_or(ProfileFailure::Missing)?;
                let encoded_profile_index_bytes = encode_stored_profile_index(
                    &stored_profile_records_and_settings,
                    Some(requested_profile_identifier),
                )?;
                atomically_write_and_sync_file(
                    &profile_index_file_path,
                    &encoded_profile_index_bytes,
                )
                .map_err(|_| ProfileFailure::Io)?;
                Ok(ProfileFilesystemTaskCompletion::Selected {
                    profile: requested_profile_identifier,
                    options: selected_profile_options,
                    challenge_progress: selected_profile_challenge_progress,
                    audio: selected_stored_profile.audio,
                    bindings: selected_stored_profile.bindings.clone(),
                    display: selected_stored_profile.display,
                    graphics: selected_stored_profile.graphics,
                })
            })),
        ));
    }
}

pub(super) fn begin_deleting_requested_profiles(
    mut commands: Commands,
    mut profile_deletion_requests: MessageReader<DeleteProfile>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_index: Res<ProfileIndex>,
    active_profile_filesystem_tasks: Query<(), With<ProfileFilesystemTask>>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
) {
    let mut operation_started = false;
    for profile_deletion_request in profile_deletion_requests.read() {
        if profile_index.selected_profile_identifier
            == Some(profile_deletion_request.requested_profile_identifier)
        {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_deletion_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Active,
            });
            continue;
        }
        if !profile_index.profile_records.iter().any(|profile_record| {
            profile_record.profile_identifier
                == profile_deletion_request.requested_profile_identifier
        }) {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_deletion_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Missing,
            });
            continue;
        }
        let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_deletion_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Io,
            });
            continue;
        };
        if operation_started || !active_profile_filesystem_tasks.is_empty() {
            profile_operation_failure_messages.write(ProfileOperationFailed {
                affected_profile_identifier: Some(
                    profile_deletion_request.requested_profile_identifier,
                ),
                failure_reason: ProfileFailure::Busy,
            });
            continue;
        }
        let profile_identifier_to_delete = profile_deletion_request.requested_profile_identifier;
        let profile_index_file_path = persistence_filesystem_paths.profile_index_file_path();
        let profile_save_directory_path = persistence_filesystem_paths
            .profile_save_directory_path(&profile_identifier_to_delete.0);
        let selected_profile_identifier = profile_index.selected_profile_identifier;
        operation_started = true;
        commands.spawn((
            ProfileFilesystemTaskContext(Some(profile_identifier_to_delete)),
            ProfileFilesystemTask(IoTaskPool::get().spawn(async move {
                if profile_save_directory_path.exists()
                    && std::fs::read_dir(&profile_save_directory_path)
                        .map_err(|_| ProfileFailure::Io)?
                        .next()
                        .transpose()
                        .map_err(|_| ProfileFailure::Io)?
                        .is_some()
                {
                    return Err(ProfileFailure::HasSaves);
                }
                let (mut stored_profile_records_and_settings, _) =
                    read_stored_profile_index_or_empty(&profile_index_file_path)?;
                let profile_count_before_deletion = stored_profile_records_and_settings.len();
                stored_profile_records_and_settings.retain(|stored_profile| {
                    stored_profile.record.profile_identifier != profile_identifier_to_delete
                });
                if stored_profile_records_and_settings.len() == profile_count_before_deletion {
                    return Err(ProfileFailure::Missing);
                }
                let remaining_profile_records = stored_profile_records_and_settings
                    .iter()
                    .map(|stored_profile| stored_profile.record.clone())
                    .collect();
                let encoded_profile_index_bytes = encode_stored_profile_index(
                    &stored_profile_records_and_settings,
                    selected_profile_identifier,
                )?;
                atomically_write_and_sync_file(
                    &profile_index_file_path,
                    &encoded_profile_index_bytes,
                )
                .map_err(|_| ProfileFailure::Io)?;
                Ok(ProfileFilesystemTaskCompletion::Deleted {
                    profile: profile_identifier_to_delete,
                    entries: remaining_profile_records,
                })
            })),
        ));
    }
}

/// Saves the latest settings once the previous profile operation has finished.
pub(super) fn begin_persisting_changed_active_profile_options(
    mut commands: Commands,
    profile_options: Res<ProfileOptions>,
    profile_challenge_completion_counts: Res<ProfileChallengeCompletionCounts>,
    audio_settings: Res<AudioSettings>,
    game_action_input_bindings: Res<GameActionInputBindings>,
    display_settings: Res<DisplaySettings>,
    graphics_settings: Res<GraphicsSettings>,
    persistence_filesystem_paths: Option<Res<PersistenceFilesystemPaths>>,
    profile_index: Res<ProfileIndex>,
    active_profile_filesystem_tasks: Query<(), With<ProfileFilesystemTask>>,
    mut profile_operation_failure_messages: MessageWriter<ProfileOperationFailed>,
    mut pending_settings_profile: Local<Option<AssetId>>,
) {
    let Some(active_profile_identifier) = profile_index.selected_profile_identifier else {
        *pending_settings_profile = None;
        return;
    };
    if profile_options.profile_identifier != active_profile_identifier {
        *pending_settings_profile = None;
        return;
    }
    if profile_options.is_changed()
        || profile_challenge_completion_counts.is_changed()
        || audio_settings.is_changed()
        || game_action_input_bindings.is_changed()
        || display_settings.is_changed()
        || graphics_settings.is_changed()
    {
        *pending_settings_profile = Some(active_profile_identifier);
    }
    if !active_profile_filesystem_tasks.is_empty()
        || *pending_settings_profile != Some(active_profile_identifier)
    {
        return;
    }
    *pending_settings_profile = None;
    let Some(persistence_filesystem_paths) = persistence_filesystem_paths.as_deref() else {
        profile_operation_failure_messages.write(ProfileOperationFailed {
            affected_profile_identifier: Some(active_profile_identifier),
            failure_reason: ProfileFailure::Io,
        });
        return;
    };
    let profile_options_to_persist = *profile_options;
    let profile_challenge_completion_counts_to_persist = *profile_challenge_completion_counts;
    let audio_settings_to_persist = *audio_settings;
    let game_action_input_bindings_to_persist = game_action_input_bindings.clone();
    let display_settings_to_persist = *display_settings;
    let graphics_settings_to_persist = *graphics_settings;
    let profile_index_file_path = persistence_filesystem_paths.profile_index_file_path();
    commands.spawn((
        ProfileFilesystemTaskContext(Some(active_profile_identifier)),
        ProfileFilesystemTask(IoTaskPool::get().spawn(async move {
            let (mut stored_profile_records_and_settings, selected_profile_identifier) =
                read_stored_profile_index_or_empty(&profile_index_file_path)?;
            let stored_active_profile = stored_profile_records_and_settings
                .iter_mut()
                .find(|stored_profile| {
                    stored_profile.record.profile_identifier == active_profile_identifier
                })
                .ok_or(ProfileFailure::Missing)?;
            stored_active_profile.options = profile_options_to_persist;
            stored_active_profile.challenge_progress =
                profile_challenge_completion_counts_to_persist;
            stored_active_profile.audio = audio_settings_to_persist;
            stored_active_profile.bindings = game_action_input_bindings_to_persist;
            stored_active_profile.display = display_settings_to_persist;
            stored_active_profile.graphics = graphics_settings_to_persist;
            let encoded_profile_index_bytes = encode_stored_profile_index(
                &stored_profile_records_and_settings,
                selected_profile_identifier,
            )?;
            atomically_write_and_sync_file(&profile_index_file_path, &encoded_profile_index_bytes)
                .map_err(|_| ProfileFailure::Io)?;
            Ok(ProfileFilesystemTaskCompletion::SettingsSaved)
        })),
    ));
}
