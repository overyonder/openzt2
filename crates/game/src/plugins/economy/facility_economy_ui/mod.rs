use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::economy::UiEconomyAction;

use crate::{
    assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    plugins::{
        information::entity_selection_types::SelectedEntity,
        ui::{
            authored_ui_change_activation_dispatch::UiPreviousSelection,
            authored_ui_node_projection_components::UiDocumentOwner,
            authored_ui_node_projection_components::UiDocumentRoot,
            authored_ui_selection_state::UiSelected,
        },
    },
};

use super::facility_economy_types::{
    CurrentSellQuote, FacilityPriceIndex, MaintenanceScheduleIndex,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiEconomyActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_selected_facility_economy_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiEconomyActions, &UiDocumentOwner)>,
    document_roots: Query<&UiDocumentRoot>,
    selected_entity: Res<SelectedEntity>,
    objects: Query<&DefinitionId>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
) {
    for activation in activations.read() {
        let Ok((action_source, document_owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok(document_root) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        let Some(selected_entity) = selected_entity.0 else {
            continue;
        };
        for action_record in action_source.authored_action_records(document) {
            if activation.trigger != action_record.trigger {
                continue;
            }
            match &action_record.action {
                UiEconomyAction::SetSelectedFacilityPriceIndex { price_index } => {
                    commands
                        .entity(selected_entity)
                        .insert(FacilityPriceIndex(*price_index));
                }
                UiEconomyAction::SetSelectedMaintenanceSchedule {
                    maintenance_schedule_index,
                } => {
                    commands
                        .entity(selected_entity)
                        .insert(MaintenanceScheduleIndex(*maintenance_schedule_index));
                }
                UiEconomyAction::RefreshSelectedEntitySellQuote => {
                    if let Some(refund) = objects
                        .get(selected_entity)
                        .ok()
                        .and_then(|definition| {
                            active_definitions
                                .get(&definitions)?
                                .find_object(definition.0)
                        })
                        .and_then(super::object_sale_refund::calculate_authored_object_sale_refund)
                    {
                        commands
                            .entity(selected_entity)
                            .insert(CurrentSellQuote(refund));
                    } else {
                        commands
                            .entity(selected_entity)
                            .remove::<CurrentSellQuote>();
                    }
                }
                UiEconomyAction::SetZooAdmissionPriceBand { .. }
                | UiEconomyAction::SetZooAdmissionsOpen { .. }
                | UiEconomyAction::GrantZooCash { .. } => {}
            }
        }
    }
}

/// Reflects the selected facility's authoritative maintenance schedule onto
/// every authored option action. No copied schedule-row model is retained.
pub(super) fn project_selected_facility_maintenance_schedule_options(
    documents: Res<Assets<UiDocumentAsset>>,
    selected_entity: Res<SelectedEntity>,
    maintenance_schedules: Query<Ref<MaintenanceScheduleIndex>>,
    document_roots: Query<&UiDocumentRoot>,
    mut action_nodes: Query<(
        &UiEconomyActions,
        &UiDocumentOwner,
        &mut UiSelected,
        &mut UiPreviousSelection,
    )>,
) {
    let Some(selected_entity_value) = selected_entity.0 else {
        return;
    };
    let Ok(maintenance_schedule) = maintenance_schedules.get(selected_entity_value) else {
        return;
    };
    if !selected_entity.is_changed()
        && !maintenance_schedule.is_changed()
        && !documents.is_changed()
    {
        return;
    }
    for (action_source, document_owner, mut selected, mut previous_selection) in &mut action_nodes {
        let Ok(document_root) = document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&document_root.document) else {
            continue;
        };
        let option_matches_selected_schedule =
            action_source
                .authored_action_records(document)
                .any(|action_record| {
                    matches!(
                        &action_record.action,
                        UiEconomyAction::SetSelectedMaintenanceSchedule {
                            maintenance_schedule_index,
                        } if *maintenance_schedule_index == maintenance_schedule.0
                    )
                });
        if selected.0 != option_matches_selected_schedule {
            selected.0 = option_matches_selected_schedule;
            previous_selection
                .synchronize_with_non_authored_selection_change(option_matches_selected_schedule);
        }
    }
}
