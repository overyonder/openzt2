use bevy::prelude::Component;
use openzt2_game_data::AssetId;

/// Allocation-free identity for a generated person name.
///
/// The text remains in the world-definition asset. Live people retain only
/// stable row coordinates into the validated authored name table.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneratedPersonNameRowSelection {
    pub person_name_pool: AssetId,
    pub first_name_row: u32,
    pub last_name_row: u32,
}
