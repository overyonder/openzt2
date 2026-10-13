use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::shell::shell_navigation_request_types::{ChooseWorld, StartSelectedWorld};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiScenarioActions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;

/// Objective category selected by the owning scenario UI document.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScenarioObjectiveCategoryFilter(pub AssetId);

/// Objective status selected by the owning scenario UI document.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum ScenarioObjectiveStatusFilter {
    #[default]
    All,
    Success,
    Failure,
    Neutral,
}

/// The live objectives an authored goal list shows, one per row in order.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(super) struct ScenarioGoalListProjection {
    pub(super) objectives: Vec<Entity>,
}

/// Stable reference from an authored goal row to its live objective.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScenarioGoalRowProjection {
    pub(super) objective: Entity,
    pub(super) definition: AssetId,
}

#[derive(SystemParam)]
pub(super) struct ScenarioShellMessageWriters<'w> {
    pub(super) choose_world: MessageWriter<'w, ChooseWorld>,
    pub(super) start_world: MessageWriter<'w, StartSelectedWorld>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredScenarioUiActionDocumentQueries<'w, 's> {
    pub(super) action_nodes: Query<'w, 's, (&'static UiScenarioActions, &'static UiDocumentOwner)>,
    pub(super) document_roots: Query<'w, 's, &'static UiDocumentRoot>,
}
