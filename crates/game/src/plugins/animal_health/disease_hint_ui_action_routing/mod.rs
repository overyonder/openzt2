//! Authored disease-hint UI actions applied directly to selected animal health.

use bevy::prelude::*;
use openzt2_game_data::ui_document::action::scenarios::UiScenarioAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::entity_selection_types::SelectedEntity,
        ui::{
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
        },
    },
};

use super::types::Disease;
use crate::plugins::ui::authored_ui_action_projection_components::UiScenarioActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_authored_increment_disease_hint_ui_actions_to_selected_animal_disease(
    mut activated_ui_nodes: MessageReader<UiNodeActivated>,
    ui_document_assets: Res<Assets<UiDocumentAsset>>,
    scenario_action_nodes: Query<(&UiScenarioActions, &UiDocumentOwner)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    selected_entity: Res<SelectedEntity>,
    mut animal_diseases: Query<&mut Disease>,
) {
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
            if activated_ui_node.trigger != authored_scenario_action.trigger
                || !matches!(
                    &authored_scenario_action.action,
                    UiScenarioAction::IncrementDiseaseHint
                )
            {
                continue;
            }
            let Some(selected_entity) = selected_entity.0 else {
                continue;
            };
            if let Ok(mut selected_animal_disease) = animal_diseases.get_mut(selected_entity) {
                selected_animal_disease.hint_level =
                    selected_animal_disease.hint_level.saturating_add(1).min(3);
            }
        }
    }
}
