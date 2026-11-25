//! Authored award UI actions applied directly to earned progression awards.

use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_world_state::world_spawn::{
    world_load_completion_marker::WorldLoadCompleted,
    world_membership_types::{WorldMember, WorldRoot},
};

use super::award_and_progression_fact_types::{
    EarnedProgressionAward, ProgressionAwardPresentationAcknowledged,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiScenarioActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_mark_awards_seen_ui_actions_to_earned_progression_awards(
    mut commands: Commands,
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    scenario_action_nodes: Query<(&UiScenarioActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    unacknowledged_progression_awards: Query<
        (Entity, &WorldMember),
        (
            With<EarnedProgressionAward>,
            Without<ProgressionAwardPresentationAcknowledged>,
        ),
    >,
) {
    let Ok(world) = active_world.single() else {
        activated_ui_nodes.clear();
        return;
    };
    for activated_ui_node in activated_ui_nodes.read() {
        let Ok((authored_scenario_actions, ui_document_owner)) =
            scenario_action_nodes.get(activated_ui_node.node)
        else {
            continue;
        };
        let Ok(ui_document_root) = ui_document_roots.get(ui_document_owner.0) else {
            continue;
        };
        let Some(ui_document_asset) = ui_document_assets.get(&ui_document_root.document) else {
            continue;
        };
        for authored_scenario_action in
            authored_scenario_actions.authored_action_records(ui_document_asset)
        {
            if activated_ui_node.trigger == authored_scenario_action.trigger
                && matches!(
                    &authored_scenario_action.action,
                    UiScenarioAction::MarkAwardsSeen
                )
            {
                for (award, _) in unacknowledged_progression_awards
                    .iter()
                    .filter(|(_, member)| member.root == world)
                {
                    commands
                        .entity(award)
                        .insert(ProgressionAwardPresentationAcknowledged);
                }
            }
        }
    }
}
