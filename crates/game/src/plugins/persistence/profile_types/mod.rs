use bevy::prelude::{Message, Resource};
use openzt2_game_data::AssetId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileRecord {
    pub profile_identifier: AssetId,
    pub profile_display_name: String,
    pub last_played_unix_timestamp_milliseconds: u64,
}

#[derive(Resource, Debug, Default)]
pub struct ProfileIndex {
    pub profile_records: Vec<ProfileRecord>,
    /// Profile used when launching a world.
    pub selected_profile_identifier: Option<AssetId>,
}

/// Settings loaded for `ProfileIndex::selected_profile_identifier`.
///
/// `profile_identifier` identifies the profile for save-path and snapshot checks.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProfileOptions {
    pub profile_identifier: AssetId,
    pub locale_identifier: AssetId,
    pub controller_input_enabled: bool,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadProfileIndex;

#[derive(Message, Debug, Clone, PartialEq, Eq)]
pub struct CreateProfile {
    pub requested_profile_display_name: String,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectProfile {
    pub requested_profile_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteProfile {
    pub requested_profile_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileIndexReady;

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileCreated {
    pub created_profile_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSelected {
    pub selected_profile_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileDeleted {
    pub deleted_profile_identifier: AssetId,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileOperationFailed {
    pub affected_profile_identifier: Option<AssetId>,
    pub failure_reason: ProfileFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileFailure {
    Busy,
    Io,
    CorruptIndex,
    InvalidName,
    DuplicateName,
    Missing,
    Active,
    HasSaves,
    CapacityExceeded,
}
