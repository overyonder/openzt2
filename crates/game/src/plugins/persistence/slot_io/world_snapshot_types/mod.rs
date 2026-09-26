use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;

pub(in crate::plugins::persistence) const MAXIMUM_WORLD_DISPLAY_NAME_UTF8_BYTE_COUNT: usize = 128;

#[derive(Debug, Clone)]
pub(super) struct WorldSnapshotRecord {
    pub scenario_definition_identifier: AssetId,
    pub requested_world_definition_identifier: AssetId,
    pub map_definition_identifier: AssetId,
    pub starting_point_definition_identifier: AssetId,
    pub session_mode: WorldSessionMode,
    pub last_saved_unix_timestamp_milliseconds: u64,
    pub display_name: String,
}
