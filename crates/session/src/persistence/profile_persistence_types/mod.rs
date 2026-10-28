use bevy::{prelude::Component, tasks::Task};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_data::AssetId;

use openzt2_interface_state::input::input_types::GameActionInputBindings;
use openzt2_interface_state::settings::audio_settings_types::AudioSettings;
use openzt2_interface_state::settings::display_settings_types::DisplaySettings;
use openzt2_zoo_state::progression::profile_challenge_types::ProfileChallengeCompletionCounts;

use openzt2_interface_state::persistence::profile_types::ProfileFailure;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::profile_types::ProfileRecord;

#[derive(Clone)]
pub(super) struct StoredProfileRecordAndSettings {
    pub(super) record: ProfileRecord,
    pub(super) options: ProfileOptions,
    pub(super) challenge_progress: ProfileChallengeCompletionCounts,
    pub(super) scenario_variables:
        openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
    pub(super) audio: AudioSettings,
    pub(super) bindings: GameActionInputBindings,
    pub(super) display: DisplaySettings,
    pub(super) graphics: GraphicsSettings,
    pub(super) online_messages:
        openzt2_interface_state::settings::online_message_policy_types::OnlineMessagePolicy,
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
        scenario_variables:
            openzt2_zoo_state::progression::profile_scenario_variables::ProfileScenarioVariables,
        audio: AudioSettings,
        bindings: GameActionInputBindings,
        display: DisplaySettings,
        graphics: GraphicsSettings,
        online_messages:
            openzt2_interface_state::settings::online_message_policy_types::OnlineMessagePolicy,
    },
    SettingsSaved,
    Deleted {
        profile: AssetId,
        entries: Vec<ProfileRecord>,
    },
}
