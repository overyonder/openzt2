//! Player-visible application lifecycle state.

use bevy::prelude::*;

/// The shell owns menu navigation and the world-loading plugin owns loading,
/// in-game completion and teardown. Domain plugins observe this lifecycle.
#[derive(States, Reflect, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum GamePhase {
    #[default]
    Boot,
    MainMenu,
    MapSelection,
    Loading,
    InGame,
}
