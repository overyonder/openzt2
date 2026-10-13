//! Interface changes authored by the mode's scenario manager document.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::game_session_types::WorldSessionMode;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

/// `scenario/goals/challengegoals.xml`, which the main menu's Challenge button
/// selects with `ZT_SCENARIO_SELECTION_CHANGED`. Its `uimods` show the goals
/// panel's `photo challenges tab`.
const CHALLENGE_MODE_SCENARIO: &str = "challengegoals";

/// Applies the Challenge mode scenario's `uimods` to interface nodes as they
/// are projected.
pub(super) fn apply_mode_scenario_ui_modifications_to_projected_nodes(
    worlds: Query<&SelectedWorldIdentity>,
    scenarios: Res<WorldScenarios>,
    scenario_assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut nodes: Query<(&Name, &mut Visibility), Added<UiDocumentOwner>>,
) {
    if nodes.is_empty()
        || !worlds
            .iter()
            .any(|world| world.mode == WorldSessionMode::Challenge)
    {
        return;
    }
    let Some(modifications) = scenarios
        .get(&scenario_assets)
        .and_then(|scenarios| scenarios.scenario(AssetId::from_key(CHALLENGE_MODE_SCENARIO)))
        .map(|scenario| &scenario.ui_modifications)
    else {
        return;
    };
    for (name, mut visibility) in &mut nodes {
        if let Some(modification) = modifications
            .iter()
            .find(|modification| modification.node.eq_ignore_ascii_case(name.as_str()))
        {
            *visibility = if modification.visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}
