use super::panel_binding_families::*;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_zoo_state::transport_tours::tour_scoring_types::TourScore;
use openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation;
use openzt2_zoo_state::transport_tours::transport_rider_types::{StationBoardedRiders, StationQueue, VehicleRiders};
use openzt2_zoo_state::transport_tours::transport_vehicle_types::TransportVehicle;

use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn project_selected_transport_status_to_visible_information_panels(
    information_panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Option<Ref<TransportFields>>,
            Option<Ref<TransportVisibilityFields>>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    transport_subjects: Query<(
        Option<&TransportCircuit>,
        Option<&TransportStation>,
        Option<&TransportVehicle>,
        Option<&WaitingForTransport>,
        Option<&TransportRider>,
        Option<&TourScore>,
        Option<&StationQueue>,
        Option<&StationBoardedRiders>,
        Option<&VehicleRiders>,
    )>,
    mut authored_integer_values: Query<(&UiValueBinding, &mut UiValue), TransportField>,
    mut authored_boolean_visibility: Query<
        (&UiVisibleBinding, &mut Visibility),
        TransportVisibilityField,
    >,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
) {
    let world_definitions = active_world_definitions.get(&world_definition_assets);
    for (parent, inherited_visibility, fields, visible_fields) in &information_panels {
        let Ok(selection) = selections.get(parent.parent()) else {
            continue;
        };
        if !inherited_visibility.get() {
            continue;
        }
        let (circuit, station, vehicle, waiting, rider, tour_score, queued, boarded, riders) =
            transport_subjects.get(selection.0).unwrap_or_default();

        for entity in fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((property_binding, mut projected_value)) =
                authored_integer_values.get_mut(entity)
            else {
                continue;
            };
            let next_value = match &property_binding.0 {
                UiIntegerPropertyBindingSource::TransportStationQueued => {
                    queued.map(|queue| queue.len() as i64)
                }
                UiIntegerPropertyBindingSource::TransportStationOccupied => {
                    boarded.map(|riders| riders.len() as i64)
                }
                UiIntegerPropertyBindingSource::TransportStationCapacity => {
                    station.map(|station| i64::from(station.capacity))
                }
                UiIntegerPropertyBindingSource::TransportVehicleOccupied => {
                    riders.map(|riders| riders.len() as i64)
                }
                UiIntegerPropertyBindingSource::TransportVehicleSeats => {
                    vehicle.map(|vehicle| i64::from(vehicle.seats))
                }
                UiIntegerPropertyBindingSource::TransportTripScoreMilli => {
                    tour_score.map(|score| convert_finite_f32_to_nearest_milli_integer(score.value))
                }
                _ => continue,
            };
            projected_value.set_if_neq(UiValue(next_value.unwrap_or(0)));
        }

        for entity in visible_fields
            .as_deref()
            .into_iter()
            .flat_map(RelationshipTarget::iter)
        {
            let Ok((property_binding, mut projected_visibility)) =
                authored_boolean_visibility.get_mut(entity)
            else {
                continue;
            };
            let should_be_visible = match &property_binding.0 {
                UiBooleanPropertyBindingSource::TransportCircuitClosed => {
                    circuit.is_some_and(|circuit| circuit.closed)
                }
                UiBooleanPropertyBindingSource::TransportStationRunsSkyTrack { sky } => station
                    .and_then(|station| world_definitions?.find_station(station.definition))
                    .is_some_and(|definition| {
                        (definition.kind == TransportationTrackKind::Sky) == *sky
                    }),
                UiBooleanPropertyBindingSource::TransportWaiting => waiting.is_some(),
                UiBooleanPropertyBindingSource::TransportRiding => rider.is_some(),
                _ => continue,
            };
            projected_visibility.set_if_neq(if should_be_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
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
