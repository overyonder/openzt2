//! Ground and sky transport expressed as graph, vehicle, and rider ECS facts.
mod transport_passenger_mounts;
mod transport_track_endpoint_queries;
mod transport_track_piece_chain_geometry;
mod transport_track_preview_presentation;

mod sky_tower_navigation;
mod topology;
mod tour_score_calculations;
mod tour_view_scoring;
mod transport_circuit_validation;
mod transport_construction_intent_routing;
mod transport_definition_hydration;
mod transport_fact_validation;
mod transport_fare_settlement;
mod transport_fare_types;
mod transport_guest_arrival;
mod transport_relationship_cleanup;
mod transport_rider_boarding;
mod transport_rider_disembarkation;
mod transport_route_position_calculations;
mod transport_track_construction_preview;
mod transport_track_construction_transaction;
mod transport_track_piece_presentation;
mod transport_track_sale;
mod transport_track_sale_steps;
mod transport_ui_action_routing;
mod transport_vehicle_movement;
mod vehicle_generation;

use bevy::prelude::*;

use openzt2_game_assets::application_lifecycle::GamePhase;
use openzt2_world_state::application_schedule::FixedGameSet;
use openzt2_world_state::application_schedule::GameSet;
use openzt2_zoo_state::construction::FixedConstructionPreparedDomainEditMaterializationSet;
use openzt2_zoo_state::construction::FixedConstructionSet;

use openzt2_zoo_state::transport_tours::tour_scoring_types::TransportTripCompleted;
use openzt2_zoo_state::transport_tours::transport_circuit_types::OpenCircuitRequest;
use openzt2_zoo_state::transport_tours::transport_rider_types::BoardTransportRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::AttachTransportStationRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::ConnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::DetachTransportStationRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::DisconnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::SkyTowerTransitionRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportStationAssignmentRejected;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionApplied;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionRejected;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackDisconnectionApplied;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::ApplyPreparedTransportTrackConstructionEditRequest;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditApplicationAcknowledged;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;
use openzt2_zoo_state::transport_tours::transport_vehicle_types::GenerateTransportVehicleForCircuitRequest;

pub struct TransportToursPlugin;

impl Plugin for TransportToursPlugin {
    fn build(&self, app: &mut App) {
        use openzt2_zoo_state::topology::persistent_edit_lookup::PersistentEditLookupPlugin;
        if !app.is_plugin_added::<PersistentEditLookupPlugin>() {
            app.add_plugins(PersistentEditLookupPlugin);
        }
        transport_relationship_cleanup::install(app);
        app.add_observer(transport_passenger_mounts::eject_passengers_after_vehicle_charge)
            .add_systems(FixedUpdate, transport_passenger_mounts::project_seated_passenger_poses
                .after(transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments)
                .in_set(FixedGameSet::Navigate).run_if(in_state(GamePhase::InGame)));
        vehicle_generation::install(app);
        app.init_resource::<openzt2_zoo_state::transport_tours::transport_highlight_style::TransportHighlightStyle>()
            .init_resource::<transport_track_construction_preview::TransportHighlightBlink>()
            .add_systems(Update, transport_track_construction_preview::tick_transport_highlight_blink
                .before(transport_track_construction_preview::highlight_transport_track_connection_stations));
        app.add_systems(
            Update,
            crate::transport_tours::transport_track_sale::refresh_selected_transport_sale_quote
                .after(openzt2_zoo_state::economy::EconomyUiPolicyApplicationSet),
        );
        app.add_message::<OpenCircuitRequest>()
            .add_message::<AttachTransportStationRequest>()
            .add_message::<DetachTransportStationRequest>()
            .add_message::<ConnectTransportTrackRequest>()
            .add_message::<DisconnectTransportTrackRequest>()
            .add_message::<TransportTrackConnectionRejected>()
            .add_message::<TransportTrackConnectionApplied>()
            .add_message::<TransportTrackDisconnectionApplied>()
            .add_message::<TransportStationAssignmentRejected>()
            .add_message::<TransportTrackConstructionEditPrepared>()
            .add_message::<TransportTrackConstructionEditPreparationRejected>()
            .add_message::<ApplyPreparedTransportTrackConstructionEditRequest>()
            .add_message::<TransportTrackConstructionEditApplicationAcknowledged>()
            .add_message::<transport_track_sale_steps::ApplyTransportTrackSaleStep>()
            .add_message::<transport_track_sale_steps::TransportTrackSaleStepAcknowledged>()
            .add_message::<GenerateTransportVehicleForCircuitRequest>()
            .add_message::<openzt2_zoo_state::transport_tours::transport_circuit_types::SetTransportCircuitFare>()
            .add_message::<BoardTransportRequest>()
            .add_message::<SkyTowerTransitionRequest>()
            .add_message::<TransportTripCompleted>()
            .add_message::<transport_track_piece_presentation::DiscardedTrackJointEndpoints>()
            .add_observer(transport_track_piece_presentation::capture_discarded_track_joint_endpoints)
            .add_observer(transport_track_piece_presentation::remove_discarded_route_drawings)
            .add_systems(
                Update,
                (
                    crate::transport_tours::transport_track_construction_preview::continue_track_preview_from_committed_junction,
                    crate::transport_tours::transport_track_construction_preview::update_transport_track_construction_preview_from_pointer_and_station_endpoints,
                    crate::transport_tours::transport_track_construction_preview::begin_or_commit_transport_track_construction_from_primary_pointer_action,
                )
                    .chain()
                    .after(openzt2_zoo_state::construction::ConstructionCursorUpdateSet)
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    sky_tower_navigation::hydrate_sky_tower_endpoints,
                    crate::transport_tours::transport_track_piece_presentation::invalidate_track_drawings_at_changed_joints,
                    crate::transport_tours::transport_track_piece_presentation::show_construction_markers_at_open_sky_towers,
                    crate::transport_tours::transport_track_preview_presentation::project_transport_track_construction_preview_authored_prefabs
                        .after(openzt2_zoo_state::placement::ObjectPlacementPreviewUpdateSet),
                    crate::transport_tours::transport_track_piece_presentation::project_committed_transport_track_piece_authored_prefabs
                        .after(crate::transport_tours::transport_track_piece_presentation::invalidate_track_drawings_at_changed_joints)
                        .after(sky_tower_navigation::hydrate_sky_tower_endpoints),
                    crate::transport_tours::transport_track_piece_presentation::show_construction_markers_at_unconnected_road_ends,
                    crate::transport_tours::transport_track_piece_presentation::show_construction_markers_at_unconnected_station_ends,
                )
                    .in_set(GameSet::Presentation)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                crate::transport_tours::transport_track_construction_preview::highlight_transport_track_connection_stations
                    .in_set(GameSet::Presentation)
                    .after(crate::transport_tours::transport_track_preview_presentation::project_transport_track_construction_preview_authored_prefabs)
                    .after(crate::transport_tours::transport_track_piece_presentation::project_committed_transport_track_piece_authored_prefabs)
                    .run_if(in_state(GamePhase::InGame)),
                    transport_track_construction_preview::apply_changed_track_highlights
                        .after(transport_track_construction_preview::highlight_transport_track_connection_stations)
                        .in_set(GameSet::Presentation)
                        .run_if(in_state(GamePhase::InGame)),
                ),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::transport_tours::transport_track_construction_transaction::prepare_transport_track_construction_edit,
                    crate::transport_tours::transport_track_sale::prepare_transport_track_sale,
                )
                    .in_set(FixedConstructionSet::Prepare)
                    .after(
                        openzt2_zoo_state::construction::FixedConstructionTransactionMaterializationSet,
                    )
                    .before(FixedConstructionPreparedDomainEditMaterializationSet)
                    .before(openzt2_zoo_state::construction::ConstructionDomainPreparationCollectionSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    transport_track_sale_steps::begin_requested_transport_track_sales,
                    crate::transport_tours::transport_track_construction_transaction::apply_prepared_transport_track_construction_edits,
                    crate::transport_tours::topology::apply_transport_station_and_track_topology_mutation_requests,
                )
                    .chain()
                    .in_set(FixedConstructionSet::Apply)
                    .after(openzt2_zoo_state::construction::ConstructionDomainApplicationDispatchSet)
                    .after(openzt2_zoo_state::construction::ConstructionFailureRollbackDispatchSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::transport_tours::transport_track_construction_transaction::acknowledge_completed_transport_track_topology_applications,
                    transport_track_sale_steps::settle_acknowledged_transport_track_sale_steps,
                )
                    .chain()
                    .in_set(FixedConstructionSet::Cleanup)
                    .before(openzt2_zoo_state::construction::ConstructionDomainAcknowledgementCollectionSet)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    transport_ui_action_routing::set_transport_circuit_fares,
                    crate::transport_tours::transport_ui_action_routing::select_authored_transport_vehicle_for_selected_compatible_circuit,
                    crate::transport_tours::transport_ui_action_routing::route_authored_transport_ui_actions_to_selected_circuit_and_path_deletion_operations,
                    crate::transport_tours::vehicle_generation::start_transport_vehicle_purchases,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::transport_tours::transport_guest_arrival::request_transport_boarding_for_owned_guest_arrivals,
                    crate::transport_tours::transport_guest_arrival::cancel_interrupted_transport_queues,
                    crate::transport_tours::transport_rider_boarding::accept_affordable_transport_boarding_requests_at_reachable_stations,
                    crate::transport_tours::sky_tower_navigation::request_sky_tower_rider_navigation_to_authored_endpoint,
                    crate::transport_tours::tour_view_scoring::observe_visible_tour_subjects_and_accumulate_rider_scores,
                    crate::transport_tours::transport_rider_disembarkation::disembark_transport_riders_and_request_service_fare_transactions,
                    crate::transport_tours::transport_rider_boarding::board_waiting_guests_when_transport_vehicle_arrives,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    crate::transport_tours::transport_definition_hydration::hydrate_transport_entities_from_authored_world_definitions,
                    crate::transport_tours::transport_construction_intent_routing::attach_committed_transport_stations_to_compatible_or_new_circuits,
                    transport_definition_hydration::materialize_restored_transport_prefabs,
                    transport_definition_hydration::request_reopening_saved_transport_circuits,
                    crate::transport_tours::transport_definition_hydration::initialize_transport_vehicle_routes_from_first_stable_circuit_edge,
                    transport_passenger_mounts::hydrate_vehicle_seats
                        .after(transport_definition_hydration::hydrate_transport_entities_from_authored_world_definitions),
                    transport_passenger_mounts::resolve_authored_vehicle_seat_mounts,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::transport_tours::transport_route_position_calculations::refresh_transport_track_travel_distances,
                    crate::transport_tours::transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments
                        .after(crate::transport_tours::transport_route_position_calculations::refresh_transport_track_travel_distances),
                    crate::transport_tours::transport_vehicle_movement::publish_transport_vehicle_arrivals
                        .after(crate::transport_tours::transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments),
                    crate::transport_tours::transport_vehicle_movement::project_consumed_transport_vehicle_movement_into_animation_speed
                        .after(crate::transport_tours::transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments),
                )
                    .in_set(FixedGameSet::Navigate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    crate::transport_tours::transport_fare_settlement::complete_paid_transport_trips_and_publish_guest_reactions,
                    crate::transport_tours::transport_fare_settlement::discard_rejected_transport_fares,
                    crate::transport_tours::transport_circuit_validation::close_changed_transport_circuits_and_apply_open_requests,
                )
                    .chain()
                    .in_set(FixedGameSet::Cleanup)
                    .run_if(in_state(GamePhase::InGame)),
            );
    }
}

#[cfg(test)]
mod transport_fact_validation_tests;
#[cfg(test)]
mod transport_fare_settlement_tests;
#[cfg(test)]
mod transport_relationship_cleanup_tests;

#[cfg(test)]
mod transport_rider_boarding_tests;
#[cfg(test)]
mod transport_route_position_calculations_tests;
