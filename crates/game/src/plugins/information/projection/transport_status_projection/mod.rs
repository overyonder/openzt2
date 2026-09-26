use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::plugins::{
    transport_tours::{
        tour_scoring_types::TourScore,
        transport_circuit_types::TransportCircuit,
        transport_rider_types::{TransportRider, WaitingForTransport},
        transport_topology_types::TransportStation,
        transport_vehicle_types::TransportVehicle,
    },
    ui::{
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiValue,
        authored_ui_node_projection_components::UiValueBinding,
        authored_ui_node_projection_components::UiVisibleBinding,
    },
};

use super::super::entity_selection_types::InfoPanel;

pub(in crate::plugins::information) fn project_selected_transport_status_to_visible_information_panels(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    transport_subjects: Query<(
        Option<&TransportCircuit>,
        Option<&TransportStation>,
        Option<&TransportVehicle>,
        Option<&WaitingForTransport>,
        Option<&TransportRider>,
        Option<&TourScore>,
    )>,
    mut authored_integer_values: Query<(&UiDocumentOwner, &UiValueBinding, &mut UiValue)>,
    mut authored_boolean_visibility: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    for (panel_entity, information_panel, inherited_visibility) in &information_panels {
        if !inherited_visibility.get() {
            continue;
        }
        let (circuit, station, vehicle, waiting, rider, tour_score) = transport_subjects
            .get(information_panel.subject)
            .unwrap_or_default();

        for (document_owner, property_binding, mut projected_value) in &mut authored_integer_values
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let next_value = match &property_binding.0 {
                UiIntegerPropertyBindingSource::TransportStationQueued => {
                    station.map(|station| i64::from(station.queued))
                }
                UiIntegerPropertyBindingSource::TransportStationOccupied => {
                    station.map(|station| i64::from(station.occupied))
                }
                UiIntegerPropertyBindingSource::TransportStationCapacity => {
                    station.map(|station| i64::from(station.capacity))
                }
                UiIntegerPropertyBindingSource::TransportVehicleOccupied => {
                    vehicle.map(|vehicle| i64::from(vehicle.occupied))
                }
                UiIntegerPropertyBindingSource::TransportVehicleSeats => {
                    vehicle.map(|vehicle| i64::from(vehicle.seats))
                }
                UiIntegerPropertyBindingSource::TransportTripScoreMilli => {
                    tour_score.map(|score| convert_finite_f32_to_nearest_milli_integer(score.value))
                }
                _ => continue,
            };
            projected_value.0 = next_value.unwrap_or(0);
        }

        for (document_owner, property_binding, mut projected_visibility) in
            &mut authored_boolean_visibility
        {
            if document_owner.0 != panel_entity {
                continue;
            }
            let should_be_visible = match &property_binding.0 {
                UiBooleanPropertyBindingSource::TransportCircuitClosed => {
                    circuit.is_some_and(|circuit| circuit.closed)
                }
                UiBooleanPropertyBindingSource::TransportWaiting => waiting.is_some(),
                UiBooleanPropertyBindingSource::TransportRiding => rider.is_some(),
                _ => continue,
            };
            *projected_visibility = if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn convert_finite_f32_to_nearest_milli_integer(value: f32) -> i64 {
    if value.is_finite() {
        (value * 1000.0).round() as i64
    } else {
        0
    }
}
