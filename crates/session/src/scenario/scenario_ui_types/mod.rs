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

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[relationship(relationship_target = ScenarioGoalRows)]
pub(super) struct ScenarioGoalRowOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = ScenarioGoalRowOf)]
pub(super) struct ScenarioGoalRows(Vec<Entity>);

#[derive(SystemParam)]
pub(super) struct ScenarioShellMessageWriters<'w> {
    pub(super) continue_campaign:
        MessageWriter<'w, crate::shell::shell_navigation_request_types::ContinueCampaign>,
    pub(super) choose_world: MessageWriter<'w, ChooseWorld>,
    pub(super) start_world: MessageWriter<'w, StartSelectedWorld>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredScenarioUiActionDocumentQueries<'w, 's> {
    pub(super) action_nodes: Query<'w, 's, (&'static UiScenarioActions, &'static UiDocumentOwner)>,
    pub(super) document_roots: Query<'w, 's, &'static UiDocumentRoot>,
}
