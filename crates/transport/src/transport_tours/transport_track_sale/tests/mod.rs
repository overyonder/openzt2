use super::*;
use crate::transport_tours::topology::apply_transport_station_and_track_topology_mutation_requests;
use crate::transport_tours::transport_track_construction_transaction::acknowledge_completed_transport_track_topology_applications;
use crate::transport_tours::transport_track_construction_transaction::apply_prepared_transport_track_construction_edits;
use openzt2_construction::construction::deletion_transaction_preparation::begin_deletion_transaction_preparation;
use openzt2_game_data::world_definitions::document::WorldDefinitionDocument;
use openzt2_game_data::world_definitions::economy_transactions::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;
use openzt2_game_data::world_definitions::world_objects::WorldObjectDefinition;
use openzt2_game_data::world_definitions::world_objects::WorldObjectKind;
use openzt2_game_data::world_definitions::world_objects::WorldObjectPropertyFlags;
use openzt2_game_data::AssetId;
use openzt2_interface_state::information::entity_selection_types::SelectedEntity;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::construction::construction_edit_history_types::ConstructionEditHistory;
use openzt2_zoo_state::construction::construction_interaction_types::DeleteEntity;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::economy::facility_economy_types::CurrentSellQuote;
use openzt2_zoo_state::transport_tours::transport_circuit_types::{
    CircuitMember, TransportCircuit,
};
use openzt2_zoo_state::transport_tours::transport_topology_types::*;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::*;

fn sale_application(refund_percent: f32) -> (App, Entity, [Entity; 2], Entity, Entity) {
    let mut app = App::new();
    app.init_resource::<Assets<WorldDefinitionAsset>>()
        .init_resource::<WorldDefinitions>()
        .init_resource::<SelectedEntity>()
        .init_resource::<ConstructionEditHistory>()
        .add_message::<DeleteEntity>()
        .add_message::<PrepareDeletion>()
        .add_message::<TransportTrackConstructionEditPrepared>()
        .add_message::<TransportTrackConstructionEditPreparationRejected>()
        .add_message::<ApplyPreparedTransportTrackConstructionEditRequest>()
        .add_message::<TransportTrackConstructionEditApplicationAcknowledged>()
        .add_message::<AttachTransportStationRequest>()
        .add_message::<DetachTransportStationRequest>()
        .add_message::<ConnectTransportTrackRequest>()
        .add_message::<DisconnectTransportTrackRequest>()
        .add_message::<TransportTrackConnectionApplied>()
        .add_message::<TransportTrackConnectionRejected>()
        .add_message::<TransportTrackDisconnectionApplied>()
        .add_message::<TransportStationAssignmentRejected>()
        .add_systems(
            Update,
            (
                refresh_selected_transport_sale_quote,
                begin_deletion_transaction_preparation,
                prepare_transport_track_sale,
                crate::transport_tours::transport_track_construction_transaction::apply_stepped_transport_track_construction_edits,
                apply_prepared_transport_track_construction_edits,
                apply_transport_station_and_track_topology_mutation_requests,
                acknowledge_completed_transport_track_topology_applications,
            )
                .chain(),
        );
    let definition = AssetId::from_key("track");
    let document = WorldDefinitionDocument {
        objects: vec![WorldObjectDefinition {
            audio_cue_prefixes: Vec::new(),
            surface_effect_strength: None,
            presentation_caustic_policies: Vec::new(),
            id: definition,
            kind: WorldObjectKind::Scenery,
            water_filter: None,
            supports_show_tricks: false,
            behavior_type_identifiers: Vec::new(),
            view_data: Vec::new(),
            detach_actions: Vec::new(),
            selected_ui_broadcasts: Vec::new(),
            information_panel: AssetId::default(),
            view_class: None,
            biome_automatic_placement_class: None,
            name_key: AssetId::default(),
            description_key: AssetId::default(),
            zoopedia_subject: AssetId::default(),
            prefab: AssetId::default(),
            catalogue_preview_prefab: None,
            presentation_attachments: Vec::new(),
            named_physical_presentations: Vec::new(),
            interaction_slots: Vec::new(),
            container_quantity: None,
            transactions: vec![EconomyTransactionDefinition {
                name: AssetId::from_key("destroy"),
                category: AssetId::from_key("recycling"),
                kind: EconomyTransactionKind::Credit,
                cost_basis: EconomyTransactionCostBasis::PercentParent,
                cost: refund_percent,
                minimum_cost: 0.0,
                maximum_cost: 0.0,
                cost_step: 0.0,
                cost_choices: Vec::new(),
                initial_cost_index: 0,
                period: EconomyTransactionPeriod::Demand,
                frequency: 1,
                aggregate: false,
                track_on_parent: false,
                target: None,
                next_transaction: None,
            }],
            prefab_scale: 1.0,
            icon: AssetId::default(),
            biomes: Vec::new(),
            location: AssetId::default(),
            preview_offset_cm: [0; 3],
            preview_scale: 1.0,
            terrain_fitted: false,
            terrain_fit_height_offset_m: 0.0,
            terrain_fit_water_offset_m: None,
            real_physics_water_impact: None,
            price_cents: 10_000,
            upkeep_cents_per_month: 0,
            properties: WorldObjectPropertyFlags::DELETABLE,
            affordances: default(),
            destruction: None,
        }],
        tracks: vec![TransportationTrackDefinition {
            id: definition,
            object: definition,
            kind: TransportationTrackKind::Ground,
            cardinal_endpoint_offsets_metres: [[0.0, -0.5, 0.0], [0.0, 0.5, 0.0]],
            diagonal_endpoint_offsets_metres: [[0.0, -0.75, 0.0], [0.0, 0.75, 0.0]],
            max_grade_permille: 100,
            purchase_cost_cents: 10_000,
            sky_maximum_connection_distance_metres: None,
            sky_rope_clearance_metres: None,
            ground_piece_presentations: Vec::new(),
            sky_tower_presentation: None,
        }],
        ..default()
    };
    let handle = app
        .world_mut()
        .resource_mut::<Assets<WorldDefinitionAsset>>()
        .add(WorldDefinitionAsset::from_test_document(document.clone()));
    app.world_mut()
        .resource_mut::<WorldDefinitions>()
        .index_test_document(handle, &document);
    let root = app.world_mut().spawn_empty().id();
    let world = WorldMember { root };
    app.insert_resource(
        openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator::from_snapshot(
            root, 100,
        )
        .unwrap(),
    );
    let circuit = app
        .world_mut()
        .spawn((
            TransportCircuit {
                definition,
                closed: false,
            },
            world,
        ))
        .id();
    let station = app
        .world_mut()
        .spawn((
            TransportStation {
                definition,
                capacity: 4,
                queued: 0,
                occupied: 0,
            },
            CircuitMember(circuit),
            world,
        ))
        .id();
    let junction = app
        .world_mut()
        .spawn((TransportTrackJunction, PersistentId(3), world))
        .id();
    let profile = TrackProfile {
        definition,
        kind: TransportationTrackKind::Ground,
        maximum_grade_permille: 100,
    };
    let first = app
        .world_mut()
        .spawn((PersistentId(1), profile, world))
        .id();
    let second = app
        .world_mut()
        .spawn((
            PersistentId(2),
            profile,
            world,
            TransportTrackConstructionPiece {
                route_segment: first,
                piece_index: 1,
            },
        ))
        .id();
    app.world_mut().entity_mut(first).insert((
        TransportTrackConstructionPiece {
            route_segment: first,
            piece_index: 0,
        },
        CircuitMember(circuit),
        TrackSegment {
            definition,
            from: station,
            from_endpoint_index: 0,
            from_position: Vec3::ZERO,
            to: junction,
            to_endpoint_index: 0,
            to_position: Vec3::X * 2.0,
            path_points: vec![Vec3::ZERO, Vec3::X, Vec3::X * 2.0].into_boxed_slice(),
            travel_path_points: vec![Vec3::ZERO, Vec3::X, Vec3::X * 2.0].into_boxed_slice(),
            length: 2.0,
        },
    ));
    app.world_mut().resource_mut::<SelectedEntity>().0 = Some(second);
    (app, circuit, [first, second], junction, station)
}

#[test]
fn terminal_route_quote_and_removal_include_all_owned_pieces_once() {
    for (percent, expected) in [(80.0, 16_000), (15.0, 3_000), (0.0, 0)] {
        let (mut app, circuit, pieces, junction, station) = sale_application(percent);
        let attachment = app.world_mut().spawn(ChildOf(junction)).id();
        let presentation = app.world_mut().spawn(ChildOf(pieces[1])).id();
        app.update();
        // The delete tool takes one piece of a longer route.
        assert_eq!(
            app.world().get::<CurrentSellQuote>(pieces[1]).unwrap().0,
            Money(expected / 2)
        );
        // Leaving a quoted selection without a deletion intent cannot mutate the route.
        app.world_mut().resource_mut::<SelectedEntity>().0 = None;
        app.update();
        assert!(app.world().get_entity(attachment).is_ok());
        assert!(app
            .world_mut()
            .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
            .drain()
            .next()
            .is_none());

        app.world_mut().resource_mut::<SelectedEntity>().0 = Some(junction);
        app.update();
        assert_eq!(
            app.world().get::<CurrentSellQuote>(junction).unwrap().0,
            Money(expected)
        );
        app.world_mut().write_message(DeleteEntity(junction));
        app.update();
        let prepared: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
            .drain()
            .collect();
        assert_eq!(prepared.len(), 1);
        assert_eq!(prepared[0].cost, Money(-expected));
        let request = ApplyPreparedTransportTrackConstructionEditRequest {
            transaction: prepared[0].transaction,
            application: EditApplication::InitialCommit,
        };
        app.world_mut().write_message(request);
        app.world_mut().write_message(request);
        app.update();
        for removed in [pieces[0], pieces[1], junction, attachment, presentation] {
            assert!(app.world().get_entity(removed).is_err());
        }
        assert!(app.world().get_entity(station).is_ok());
        assert!(app.world().get::<TransportCircuit>(circuit).unwrap().closed);
        let acknowledgements: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<TransportTrackConstructionEditApplicationAcknowledged>>()
            .drain()
            .collect();
        assert_eq!(acknowledgements.len(), 1);
        assert!(acknowledgements[0].accepted);
        app.world_mut().despawn(request.transaction);
        app.world_mut().write_message(DeleteEntity(junction));
        app.update();
        assert!(app
            .world_mut()
            .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
            .drain()
            .next()
            .is_none());
    }
}

#[test]
fn removing_tail_preserves_the_incoming_route_and_its_tower_attachment() {
    let (mut app, circuit, pieces, junction, source) = sale_application(80.0);
    let world = *app.world().get::<WorldMember>(source).unwrap();
    let definition = app
        .world()
        .get::<TrackSegment>(pieces[0])
        .unwrap()
        .definition;
    app.world_mut()
        .entity_mut(source)
        .remove::<TransportStation>()
        .insert((TransportTrackJunction, PersistentId(4)));
    let station = app
        .world_mut()
        .spawn((
            TransportStation {
                definition,
                capacity: 4,
                queued: 0,
                occupied: 0,
            },
            world,
        ))
        .id();
    let mut incoming = app.world().get::<TrackSegment>(pieces[0]).unwrap().clone();
    incoming.from = station;
    incoming.to = source;
    let incoming = app
        .world_mut()
        .spawn((incoming, CircuitMember(circuit), world))
        .id();
    let tower_attachment = app.world_mut().spawn(ChildOf(source)).id();
    app.world_mut().write_message(DeleteEntity(junction));
    app.update();
    let prepared = app
        .world_mut()
        .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
        .drain()
        .next()
        .unwrap();
    assert_eq!(prepared.cost, Money(-16_000));
    app.world_mut()
        .write_message(ApplyPreparedTransportTrackConstructionEditRequest {
            transaction: prepared.transaction,
            application: EditApplication::InitialCommit,
        });
    app.update();
    assert!(app.world().get_entity(junction).is_err());
    for survivor in [source, incoming, tower_attachment, station] {
        assert!(app.world().get_entity(survivor).is_ok());
    }
}

#[test]
fn removing_a_middle_route_keeps_the_road_beyond_it() {
    let (mut app, circuit, pieces, junction, _) = sale_application(80.0);
    let world = *app.world().get::<WorldMember>(junction).unwrap();
    let end = app
        .world_mut()
        .spawn((TransportTrackJunction, PersistentId(5), world))
        .id();
    let mut onward = app.world().get::<TrackSegment>(pieces[0]).unwrap().clone();
    onward.from = junction;
    onward.to = end;
    let onward = app
        .world_mut()
        .spawn((onward, CircuitMember(circuit), world))
        .id();
    app.world_mut().write_message(DeleteEntity(junction));
    app.update();
    let prepared = app
        .world_mut()
        .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
        .drain()
        .next()
        .unwrap();
    app.world_mut()
        .write_message(ApplyPreparedTransportTrackConstructionEditRequest {
            transaction: prepared.transaction,
            application: EditApplication::InitialCommit,
        });
    app.update();
    for removed in pieces {
        assert!(app.world().get_entity(removed).is_err());
    }
    for survivor in [junction, onward, end] {
        assert!(app.world().get_entity(survivor).is_ok());
    }
}

#[test]
fn removing_one_piece_leaves_a_gap_and_undo_restores_the_route() {
    let (mut app, circuit, pieces, junction, source) = sale_application(80.0);
    let world = *app.world().get::<WorldMember>(source).unwrap();
    // Start from a junction: this fixture's station is no transport station,
    // so a route can't be laid from it again.
    app.world_mut()
        .entity_mut(source)
        .remove::<TransportStation>()
        .insert((
            TransportTrackJunction,
            PersistentId(4),
            CircuitMember(circuit),
        ));
    let mut incoming = app.world().get::<TrackSegment>(pieces[0]).unwrap().clone();
    incoming.from = junction;
    incoming.to = source;
    app.world_mut()
        .spawn((incoming, CircuitMember(circuit), world));
    for entity in [source, junction] {
        let position = if entity == source {
            Vec3::ZERO
        } else {
            Vec3::X * 2.0
        };
        app.world_mut()
            .entity_mut(entity)
            .insert(GlobalTransform::from_translation(position));
    }
    app.world_mut().write_message(DeleteEntity(pieces[0]));
    app.update();
    let prepared = app
        .world_mut()
        .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
        .drain()
        .next()
        .unwrap();
    assert_eq!(prepared.cost, Money(-8_000));
    let apply = |app: &mut App, application| {
        app.world_mut()
            .write_message(ApplyPreparedTransportTrackConstructionEditRequest {
                transaction: prepared.transaction,
                application,
            });
        // Messages last two updates, so read them after each one, without
        // draining the steps' own acknowledgements.
        let mut cursor = bevy::ecs::message::MessageCursor::<
            TransportTrackConstructionEditApplicationAcknowledged,
        >::default();
        let mut accepted = Vec::new();
        for _ in 0..8 {
            app.update();
            let messages = app
                .world()
                .resource::<Messages<TransportTrackConstructionEditApplicationAcknowledged>>();
            accepted.extend(
                cursor
                    .read(messages)
                    .filter(|ack| ack.transaction == prepared.transaction)
                    .map(|ack| ack.accepted),
            );
        }
        accepted
    };
    assert_eq!(apply(&mut app, EditApplication::InitialCommit), [true]);
    // The second piece is laid again from a junction at its start.
    let routes = app
        .world_mut()
        .query::<&TrackSegment>()
        .iter(app.world())
        .filter(|route| route.from != junction)
        .map(|route| route.path_points.clone())
        .collect::<Vec<_>>();
    assert_eq!(routes, [vec![Vec3::X, Vec3::X * 2.0].into_boxed_slice()]);
    assert!(app.world().get_entity(pieces[0]).is_err());

    assert_eq!(apply(&mut app, EditApplication::Undo), [true]);
    let routes = app
        .world_mut()
        .query::<&TrackSegment>()
        .iter(app.world())
        .filter(|route| route.from != junction)
        .map(|route| route.path_points.len())
        .collect::<Vec<_>>();
    assert_eq!(routes, [3]);
}

#[test]
fn undo_lays_again_a_leg_or_piece_removed_after_a_gap() {
    // Removing the whole leg (through its junction), then one piece of it.
    for take_one_piece in [false, true] {
        let (mut app, circuit, pieces, junction, source) = sale_application(80.0);
        let world = *app.world().get::<WorldMember>(source).unwrap();
        let end = app
            .world_mut()
            .spawn((TransportTrackJunction, PersistentId(5), world))
            .id();
        let mut onward = app.world().get::<TrackSegment>(pieces[0]).unwrap().clone();
        onward.from = junction;
        onward.to = end;
        app.world_mut()
            .spawn((onward, CircuitMember(circuit), world));
        // The leg starts at a junction nothing leads into, as a road after a gap.
        app.world_mut()
            .entity_mut(source)
            .remove::<TransportStation>()
            .insert((
                TransportTrackJunction,
                PersistentId(4),
                CircuitMember(circuit),
            ));
        for (entity, position) in [(source, Vec3::ZERO), (junction, Vec3::X * 2.0)] {
            app.world_mut()
                .entity_mut(entity)
                .insert(GlobalTransform::from_translation(position));
        }
        app.world_mut()
            .write_message(DeleteEntity(if take_one_piece {
                pieces[0]
            } else {
                junction
            }));
        app.update();
        let prepared = app
            .world_mut()
            .resource_mut::<Messages<TransportTrackConstructionEditPrepared>>()
            .drain()
            .next()
            .unwrap();
        let apply = |app: &mut App, application| {
            app.world_mut()
                .write_message(ApplyPreparedTransportTrackConstructionEditRequest {
                    transaction: prepared.transaction,
                    application,
                });
            let mut cursor = bevy::ecs::message::MessageCursor::<
                TransportTrackConstructionEditApplicationAcknowledged,
            >::default();
            let mut accepted = Vec::new();
            for _ in 0..8 {
                app.update();
                let messages = app
                    .world()
                    .resource::<Messages<TransportTrackConstructionEditApplicationAcknowledged>>();
                accepted.extend(
                    cursor
                        .read(messages)
                        .filter(|ack| ack.transaction == prepared.transaction)
                        .map(|ack| ack.accepted),
                );
            }
            accepted
        };
        assert_eq!(apply(&mut app, EditApplication::InitialCommit), [true]);
        assert!(app.world().get_entity(pieces[0]).is_err());
        assert_eq!(apply(&mut app, EditApplication::Undo), [true]);
        let restored = app
            .world_mut()
            .query::<&TrackSegment>()
            .iter(app.world())
            .filter(|route| route.to == junction)
            .count();
        assert_eq!(restored, 1);
    }
}
