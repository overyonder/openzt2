use bevy::{prelude::Component, tasks::Task};
use openzt2_game_data::AssetId;

use crate::plugins::{
    input::input_types::GameActionInputBindings,
    progression::profile_challenge_types::ProfileChallengeCompletionCounts,
    settings::{
        audio_settings_types::AudioSettings, display_settings_types::DisplaySettings,
        graphics_settings_types::GraphicsSettings,
    },
};

use super::profile_types::{ProfileFailure, ProfileOptions, ProfileRecord};

#[derive(Clone)]
pub(super) struct StoredProfileRecordAndSettings {
    pub(super) record: ProfileRecord,
    pub(super) options: ProfileOptions,
    pub(super) challenge_progress: ProfileChallengeCompletionCounts,
    pub(super) audio: AudioSettings,
    pub(super) bindings: GameActionInputBindings,
    pub(super) display: DisplaySettings,
    pub(super) graphics: GraphicsSettings,
}

#[derive(Component)]
pub(super) struct ProfileFilesystemTask(
    pub(super) Task<Result<ProfileFilesystemTaskCompletion, ProfileFailure>>,
);

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct ProfileFilesystemTaskContext(pub(super) Option<AssetId>);

pub(super) enum ProfileFilesystemTaskCompletion {
    IndexLoaded {
        entries: Vec<ProfileRecord>,
        selected: Option<AssetId>,
    },
    Created {
        record: ProfileRecord,
        entries: Vec<ProfileRecord>,
    },
    Selected {
        profile: AssetId,
        options: ProfileOptions,
        challenge_progress: ProfileChallengeCompletionCounts,
        audio: AudioSettings,
        bindings: GameActionInputBindings,
        display: DisplaySettings,
        graphics: GraphicsSettings,
    },
    SettingsSaved,
    Deleted {
        profile: AssetId,
        entries: Vec<ProfileRecord>,
    },
}
