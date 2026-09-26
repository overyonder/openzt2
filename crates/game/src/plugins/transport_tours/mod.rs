//! Ground and sky transport expressed as graph, vehicle, and rider ECS facts.
mod transport_track_endpoint_queries;
mod transport_track_piece_chain_geometry;
mod transport_track_preview_presentation;

mod sky_tower_navigation;
mod topology;
mod tour_score_calculations;
pub(crate) mod tour_scoring_types;
mod tour_view_scoring;
pub(crate) mod transport_circuit_types;
mod transport_circuit_validation;
mod transport_construction_intent_routing;
mod transport_definition_hydration;
mod transport_fact_validation;
mod transport_fare_settlement;
mod transport_fare_types;
mod transport_hierarchy_queries;
mod transport_relationship_cleanup;
mod transport_rider_boarding;
mod transport_rider_disembarkation;
pub(crate) mod transport_rider_types;
mod transport_route_position_calculations;
pub(crate) mod transport_topology_types;
mod transport_track_construction_preview;
mod transport_track_construction_transaction;
pub(crate) mod transport_track_construction_types;
mod transport_track_piece_presentation;
mod transport_ui_action_routing;
mod transport_vehicle_movement;
pub(crate) mod transport_vehicle_types;
mod vehicle_generation;

use bevy::prelude::*;

use crate::application_lifecycle::GamePhase;
use crate::application_schedule::{FixedGameSet, GameSet};
use crate::plugins::construction::{
    FixedConstructionPreparedDomainEditMaterializationSet, FixedConstructionSet,
};

use self::{
    tour_scoring_types::TransportTripCompleted,
    transport_circuit_types::OpenCircuitRequest,
    transport_rider_types::BoardTransportRequest,
    transport_topology_types::{
        AttachTransportStationRequest, ConnectTransportTrackRequest, DetachTransportStationRequest,
        DisconnectTransportTrackRequest, SkyTowerTransitionRequest,
        TransportStationAssignmentRejected, TransportTrackConnectionApplied,
        TransportTrackConnectionRejected, TransportTrackDisconnectionApplied,
    },
    transport_track_construction_types::{
        ApplyPreparedTransportTrackConstructionEditRequest,
        TransportTrackConstructionEditApplicationAcknowledged,
        TransportTrackConstructionEditPreparationRejected, TransportTrackConstructionEditPrepared,
    },
    transport_vehicle_types::GenerateTransportVehicleForCircuitRequest,
};

pub struct TransportToursPlugin;

impl Plugin for TransportToursPlugin {
    fn build(&self, app: &mut App) {
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
            .add_message::<GenerateTransportVehicleForCircuitRequest>()
            .add_message::<BoardTransportRequest>()
            .add_message::<SkyTowerTransitionRequest>()
            .add_message::<TransportTripCompleted>()
            .add_systems(
                Update,
                (
                    transport_track_construction_preview::update_transport_track_construction_preview_from_pointer_and_station_endpoints,
                    transport_track_construction_preview::begin_or_commit_transport_track_construction_from_primary_pointer_action,
                )
                    .chain()
                    .after(
                        crate::plugins::construction::construction_cursor_terrain_surface_tracking::update_construction_cursor,
                    )
                    .in_set(GameSet::Intent)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    transport_track_piece_presentation::invalidate_sky_tower_presentations_after_connection_topology_changes,
                    transport_track_preview_presentation::project_transport_track_construction_preview_authored_prefabs,
                    transport_track_piece_presentation::project_committed_transport_track_piece_authored_prefabs,
                )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                transport_track_construction_transaction::prepare_transport_track_construction_edit
                    .in_set(FixedConstructionSet::Prepare)
                    .after(
                        crate::plugins::construction::FixedConstructionTransactionMaterializationSet,
                    )
                    .before(FixedConstructionPreparedDomainEditMaterializationSet)
                    .before(
                        crate::plugins::construction::construction_domain_preparation_collection::collect_construction_domain_preparation_results,
                    )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                transport_track_construction_transaction::apply_prepared_transport_track_construction_edits
                    .in_set(FixedConstructionSet::Apply)
                    .after(
                        crate::plugins::construction::construction_domain_application_dispatch::dispatch_authorized_construction_application_to_participating_domains,
                    )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                transport_track_construction_transaction::acknowledge_completed_transport_track_topology_applications
                    .in_set(FixedConstructionSet::Cleanup)
                    .before(
                        crate::plugins::construction::construction_domain_acknowledgement_collection::collect_construction_domain_application_acknowledgements,
                    )
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    transport_ui_action_routing::select_authored_transport_vehicle_for_selected_compatible_circuit,
                    transport_ui_action_routing::route_authored_transport_ui_actions_to_selected_circuit_and_path_deletion_operations,
                    vehicle_generation::request_payment_for_generated_transport_vehicles,
                    vehicle_generation::record_transport_vehicle_purchase_results,
                    vehicle_generation::spawn_paid_transport_vehicle_prefab_instances,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    transport_rider_boarding::accept_affordable_transport_boarding_requests_at_reachable_stations,
                    sky_tower_navigation::request_sky_tower_rider_navigation_to_authored_endpoint,
                    tour_view_scoring::observe_visible_tour_subjects_and_accumulate_rider_scores,
                    transport_rider_disembarkation::disembark_transport_riders_and_request_service_fare_transactions,
                    transport_rider_boarding::board_waiting_guests_when_transport_vehicle_arrives,
                )
                    .chain()
                    .in_set(FixedGameSet::Act)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                Update,
                (
                    transport_definition_hydration::hydrate_transport_entities_from_authored_world_definitions,
                    transport_construction_intent_routing::attach_committed_transport_stations_to_compatible_or_new_circuits,
                    topology::apply_transport_station_and_track_topology_mutation_requests,
                    transport_definition_hydration::initialize_transport_vehicle_routes_from_first_stable_circuit_edge,
                )
                    .chain()
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments,
                    transport_vehicle_movement::project_consumed_transport_vehicle_movement_into_animation_speed
                        .after(transport_vehicle_movement::advance_transport_vehicles_along_canonical_track_segments),
                )
                    .in_set(FixedGameSet::Navigate)
                    .run_if(in_state(GamePhase::InGame)),
            )
            .add_systems(
                FixedUpdate,
                (
                    transport_fare_settlement::complete_paid_transport_trips_and_publish_guest_reactions,
                    transport_fare_settlement::discard_rejected_transport_fares,
                    transport_relationship_cleanup::remove_transport_relationships_broken_by_removed_circuits_stations_tracks_or_vehicles,
                    transport_circuit_validation::close_changed_transport_circuits_and_apply_open_requests,
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
