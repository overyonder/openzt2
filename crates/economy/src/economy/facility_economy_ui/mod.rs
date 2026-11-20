use openzt2_interface_state::information::entity_selection_types::SelectionChanged;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::economy::UiEconomyAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

use super::facility_economy_types::{
    CurrentSellQuote, FacilityPriceIndex, MaintenanceScheduleIndex,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiEconomyActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

pub(super) fn route_selected_facility_economy_ui_actions(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiEconomyActions, &UiDocumentOwner)>,
    document_roots: Query<&UiDocumentRoot>,
    selected_entity: WorldUiSelection,
    objects: Query<&DefinitionId>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    transport_sales: openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries,
    mut prices: Query<&mut FacilityPriceIndex>,
    mut schedules: Query<&mut MaintenanceScheduleIndex>,
    mut quotes: Query<&mut CurrentSellQuote>,
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
        let Some(subject) = selected_entity.subject() else {
            continue;
        };
        for record in action_source.authored_action_records(document) {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiEconomyAction::SetSelectedFacilityPriceIndex { price_index } => {
                    let value = FacilityPriceIndex(*price_index);
                    if let Ok(mut current) = prices.get_mut(subject) {
                        current.set_if_neq(value);
                    } else {
                        commands.entity(subject).insert(value);
                    }
                }
                UiEconomyAction::SetSelectedMaintenanceSchedule {
                    maintenance_schedule_index,
                } => {
                    let value = MaintenanceScheduleIndex(*maintenance_schedule_index);
                    if let Ok(mut current) = schedules.get_mut(subject) {
                        current.set_if_neq(value);
                    } else {
                        commands.entity(subject).insert(value);
                    }
                }
                UiEconomyAction::RefreshSelectedEntitySellQuote => {
                    if transport_sales.requires_transport_sale(subject) {
                        continue;
                    }
                    let refund = objects
                        .get(subject)
                        .ok()
                        .and_then(|definition| {
                            active_definitions
                                .get(&definitions)?
                                .find_object(definition.0)
                        })
                        .and_then(super::object_sale_refund::calculate_authored_object_sale_refund);
                    match (quotes.get_mut(subject), refund) {
                        (Ok(mut current), Some(refund)) => {
                            current.set_if_neq(CurrentSellQuote(refund));
                        }
                        (Err(_), Some(refund)) => {
                            commands.entity(subject).insert(CurrentSellQuote(refund));
                        }
                        (Ok(_), None) => {
                            commands.entity(subject).remove::<CurrentSellQuote>();
                        }
                        (Err(_), None) => {}
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
    selected_entity: WorldUiSelection,
    mut selection_changes: MessageReader<SelectionChanged>,
    maintenance_schedules: Query<Ref<MaintenanceScheduleIndex>>,
    document_roots: Query<&UiDocumentRoot>,
    mut action_nodes: Query<(&UiEconomyActions, &UiDocumentOwner, &mut UiSelected)>,
) {
    let selection_changed = selection_changes.read().count() != 0;
    let Some(selected_entity_value) = selected_entity.subject() else {
        return;
    };
    let Ok(maintenance_schedule) = maintenance_schedules.get(selected_entity_value) else {
        return;
    };
    if !selection_changed && !maintenance_schedule.is_changed() && !documents.is_changed() {
        return;
    }
    for (action_source, document_owner, mut selected) in &mut action_nodes {
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
        }
    }
}
