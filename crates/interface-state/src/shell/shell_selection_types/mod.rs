use bevy::{ecs::query::QueryData, prelude::*};
use openzt2_game_data::AssetId;
use openzt2_world_state::game_session_types::WorldSessionMode;

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

/// The choice shown in a reusable world-selection row.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = WorldChoiceRows)]
pub struct WorldChoiceView(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = WorldChoiceView)]
pub struct WorldChoiceRows(Vec<Entity>);

/// A globe anchor expires with the choice it represents.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = WorldChoiceMarkers)]
pub struct WorldChoiceMarkerView(pub Entity);

#[derive(Component)]
#[relationship_target(relationship = WorldChoiceMarkerView, linked_spawn)]
pub struct WorldChoiceMarkers(Vec<Entity>);

#[derive(QueryData)]
pub struct WorldChoiceBinding {
    pub views: AnyOf<(&'static WorldChoiceView, &'static WorldChoiceMarkerView)>,
}

impl WorldChoiceBindingItem<'_, '_> {
    pub fn choice(&self) -> Option<Entity> {
        self.views
            .0
            .map(|view| view.0)
            .or_else(|| self.views.1.map(|view| view.0))
    }
}

/// Authored world-scenario catalogue globe coordinate retained by one choice.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobeMarker(pub [i16; 2]);

#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ShellSelection {
    pub mode: Option<WorldSessionMode>,
    pub scenario: Option<AssetId>,
    /// Optional challenge-mode override selected by the authored cash slider.
    pub starting_cash_cents: Option<i64>,
}
