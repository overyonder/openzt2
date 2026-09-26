use crate::game_session_types::WorldSessionMode;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellScreen {
    Splash,
    MainMenu,
    ProfileSelect,
    Options,
    Downloads,
    SavedGames,
    Globe { mode: WorldSessionMode },
    MapSelect { mode: WorldSessionMode },
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileChoice(pub AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldChoice {
    pub scenario: AssetId,
    pub map: AssetId,
    pub mode: WorldSessionMode,
}

/// The map choice represented by this UI entity.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldChoiceView(pub Entity);

/// Authored world-scenario catalogue globe coordinate retained by one choice.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GlobeMarker(pub [i16; 2]);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ShellSelection {
    pub mode: Option<WorldSessionMode>,
    pub scenario: Option<AssetId>,
    /// Optional challenge-mode override selected by the authored cash slider.
    pub starting_cash_cents: Option<i64>,
}
