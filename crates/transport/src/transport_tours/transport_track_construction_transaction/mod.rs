use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::construction::construction_interaction_types::ConstructionPreview;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementFailure;
use openzt2_zoo_state::construction::construction_interaction_types::PlacementValidity;
use openzt2_zoo_state::construction::construction_tool_and_placement_policy_types::ConstructionTool;
use openzt2_zoo_state::construction::construction_transaction_types::EditApplication;
use openzt2_zoo_state::construction::construction_transaction_types::PrepareConstruction;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_zoo_state::transport_tours::transport_topology_types::ConnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::DisconnectTransportTrackRequest;
use openzt2_zoo_state::transport_tours::transport_topology_types::TrackProfile;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionApplied;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackConnectionRejected;
use openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackDisconnectionApplied;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::ApplyPreparedTransportTrackConstructionEditRequest;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::PreparedTransportTrackConstructionEdit;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditApplicationAcknowledged;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPreparationRejected;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditPrepared;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionEditSteps;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPiece;
use openzt2_zoo_state::transport_tours::transport_track_construction_types::TransportTrackConstructionPreview;

fn selected_transport_track_definition(
    tool: ConstructionTool,
) -> Option<openzt2_game_data::AssetId> {
    match tool {
        ConstructionTool::GroundTrackPlacement(Some(definition))
        | ConstructionTool::SkyTrackPlacement(Some(definition)) => Some(definition),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_transport_track_construction_edit(
    mut commands: Commands,
    mut requests: MessageReader<PrepareConstruction>,
    tool: Res<ConstructionTool>,
    previews: Query<(
        &ConstructionPreview,
        &TransportTrackConstructionPreview,
        &WorldMember,
    )>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut allocator: ResMut<PersistentIdAllocator>,
    mut prepared: MessageWriter<TransportTrackConstructionEditPrepared>,
    mut rejected: MessageWriter<TransportTrackConstructionEditPreparationRejected>,
) {
    let Some(selected_definition) = selected_transport_track_definition(*tool) else {
        return;
    };
    let catalogue = active_definitions.get(&definitions);
    for request in requests.read() {
        let reject =
            |rejected: &mut MessageWriter<TransportTrackConstructionEditPreparationRejected>,
             reason| {
                rejected.write(TransportTrackConstructionEditPreparationRejected {
                    transaction: request.transaction,
                    reason,
                });
            };
        let Ok((preview, track_preview, world_member)) = previews.get(request.preview) else {
            reject(&mut rejected, PlacementFailure::InvalidTopology);
            continue;
        };
        let Some(track_definition) = catalogue
            .filter(|_| preview.definition == selected_definition)
            .and_then(|catalogue| catalogue.find_track(preview.definition))
        else {
            reject(
                &mut rejected,
                PlacementFailure::AuthoredRule(preview.definition),
            );
            continue;
        };
        let PlacementValidity::Valid { cost } = preview.validity else {
            reject(&mut rejected, PlacementFailure::InvalidTopology);
            continue;
        };
        let Some(piece_count) = track_preview.path_points.len().checked_sub(1) else {
            reject(&mut rejected, PlacementFailure::InvalidTopology);
            continue;
        };
        let piece_entities = (0..piece_count)
            .map(|_| allocator.allocate(world_member.root))
            .collect::<Result<Vec<_>, _>>();
        let Ok(piece_entities) = piece_entities else {
            reject(&mut rejected, PlacementFailure::InvalidTopology);
            continue;
        };
        let destination_junction_entity = if track_preview.to_station.is_none() {
            match allocator.allocate(world_member.root) {
                Ok(entity) => Some(entity),
                Err(_) => {
                    reject(&mut rejected, PlacementFailure::InvalidTopology);
                    continue;
                }
            }
        } else {
            None
        };
        commands
            .entity(request.transaction)
            .insert(PreparedTransportTrackConstructionEdit {
                removes_track: false,
                piece_entities: piece_entities.into_boxed_slice(),
                destination_junction_entity,
                source_junction_entity: None,
                definition: preview.definition,
                kind: track_definition.kind,
                maximum_grade_permille: track_definition.max_grade_permille,
                circuit: track_preview.circuit,
                from_endpoint: track_preview.from_endpoint,
                from_endpoint_index: track_preview.from_endpoint_index,
                to_endpoint: track_preview.to_station,
                to_endpoint_index: track_preview.to_endpoint_index,
                path_points: track_preview.path_points.clone().into_boxed_slice(),
                applied_entities: Vec::new(),
                preview: Some(request.preview),
                application_in_flight: None,
            });
        prepared.write(TransportTrackConstructionEditPrepared {
            transaction: request.transaction,
            cost,
        });
    }
}

pub(super) fn transport_track_piece_transform(
    start_position: Vec3,
    end_position: Vec3,
) -> Transform {
    let direction = end_position - start_position;
    let mut transform = Transform::from_translation(start_position.midpoint(end_position));
    if direction.xz().length_squared() > f32::EPSILON {
        transform.rotation = Quat::from_rotation_y(direction.x.atan2(direction.z));
    }
    transform
}

fn transport_track_application_creates_entity(
    application: EditApplication,
    removes_track: bool,
    entity_exists: bool,
) -> bool {
    match application {
        EditApplication::InitialCommit | EditApplication::Redo => !removes_track,
        EditApplication::Undo => removes_track,
        EditApplication::FailureRollback => !entity_exists,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_prepared_transport_track_construction_edits(
    mut commands: Commands,
    mut requests: MessageReader<ApplyPreparedTransportTrackConstructionEditRequest>,
    mut edits: Query<(&mut PreparedTransportTrackConstructionEdit, &WorldMember)>,
    persistent_entities: Query<(Entity, &PersistentId)>,
    sales: openzt2_zoo_state::transport_tours::transport_track_sale::TransportTrackSaleQueries,
    mut connections: MessageWriter<ConnectTransportTrackRequest>,
    mut disconnections: MessageWriter<DisconnectTransportTrackRequest>,
    mut acknowledged: MessageWriter<TransportTrackConstructionEditApplicationAcknowledged>,
    stepped: Query<(), With<TransportTrackConstructionEditSteps>>,
) {
    for request in requests.read() {
        if stepped.contains(request.transaction) {
            continue;
        }
        let Ok((mut edit, world_member)) = edits.get_mut(request.transaction) else {
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        };
        if edit.application_in_flight.is_some() {
            continue;
        }
        if edit.removes_track
            && matches!(
                request.application,
                EditApplication::InitialCommit | EditApplication::Redo
            )
            && !sales.removal_still_matches(&edit)
        {
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        }
        let live_entities = edit
            .piece_entities
            .iter()
            .filter_map(|wanted_id| {
                persistent_entities
                    .iter()
                    .find_map(|(entity, id)| (*id == *wanted_id).then_some(entity))
            })
            .collect::<Vec<_>>();
        let live_destination_junction_entity =
            edit.destination_junction_entity.and_then(|wanted_id| {
                persistent_entities
                    .iter()
                    .find_map(|(entity, id)| (*id == wanted_id).then_some(entity))
            });
        let live_source_junction_entity = edit.source_junction_entity.and_then(|wanted_id| {
            persistent_entities
                .iter()
                .find_map(|(entity, id)| (*id == wanted_id).then_some(entity))
        });
        if live_entities.len() != edit.piece_entities.len() && !live_entities.is_empty() {
            for entity in live_entities {
                commands.entity(entity).despawn();
            }
            if let Some(endpoint) = live_source_junction_entity {
                commands.entity(endpoint).despawn();
            }
            edit.applied_entities.clear();
            if let Some(endpoint) = live_destination_junction_entity {
                commands.entity(endpoint).despawn();
            }
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        }
        if !transport_track_application_creates_entity(
            request.application,
            edit.removes_track,
            !live_entities.is_empty(),
        ) {
            if let Some(&route_segment) = live_entities.first() {
                edit.applied_entities = live_entities;
                edit.application_in_flight = Some(request.application);
                disconnections.write(DisconnectTransportTrackRequest {
                    transaction: request.transaction,
                    application: request.application,
                    track: route_segment,
                });
                continue;
            }
            edit.applied_entities.clear();
            if let Some(endpoint) = live_destination_junction_entity {
                commands.entity(endpoint).despawn();
            }
            if let Some(endpoint) = live_source_junction_entity {
                commands.entity(endpoint).despawn();
            }
            if let Some(preview) = edit.preview.take() {
                commands.entity(preview).despawn();
            }
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: true,
            });
            continue;
        }
        let applied_entities = if live_entities.is_empty() {
            edit.piece_entities
                .iter()
                .zip(edit.path_points.windows(2))
                .map(|(persistent_id, points)| {
                    commands
                        .spawn((
                            *persistent_id,
                            DefinitionId(edit.definition),
                            TrackProfile {
                                definition: edit.definition,
                                kind: edit.kind,
                                maximum_grade_permille: edit.maximum_grade_permille,
                            },
                            transport_track_piece_transform(points[0], points[1]),
                            GlobalTransform::IDENTITY,
                            // The piece's projected meshes are its children;
                            // without visibility here they never become visible.
                            Visibility::Inherited,
                            *world_member,
                        ))
                        .id()
                })
                .collect::<Vec<_>>()
        } else {
            live_entities
        };
        let destination_endpoint = if let Some(to_endpoint) = edit.to_endpoint {
            to_endpoint
        } else if let Some(destination_junction_entity) = live_destination_junction_entity {
            destination_junction_entity
        } else {
            let Some(destination_junction_identifier) = edit.destination_junction_entity else {
                acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                    transaction: request.transaction,
                    application: request.application,
                    accepted: false,
                });
                continue;
            };
            let Some(&destination_position) = edit.path_points.last() else {
                acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                    transaction: request.transaction,
                    application: request.application,
                    accepted: false,
                });
                continue;
            };
            commands
                .spawn((
                    destination_junction_identifier,
                    openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction,
                    Transform::from_translation(destination_position),
                    GlobalTransform::from_translation(destination_position),
                    Visibility::Inherited,
                    *world_member,
                ))
                .id()
        };
        let from_endpoint = match (edit.source_junction_entity, live_source_junction_entity) {
            (_, Some(endpoint)) => endpoint,
            (Some(source_id), None) => {
                let Some(&source_position) = edit.path_points.first() else {
                    acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                        transaction: request.transaction,
                        application: request.application,
                        accepted: false,
                    });
                    continue;
                };
                commands
                    .spawn((
                        source_id,
                        openzt2_zoo_state::transport_tours::transport_topology_types::TransportTrackJunction,
                        openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember(edit.circuit),
                        Transform::from_translation(source_position),
                        GlobalTransform::from_translation(source_position),
                        Visibility::Inherited,
                        *world_member,
                    ))
                    .id()
            }
            (None, None) => edit.from_endpoint,
        };
        let Some(&track) = applied_entities.first() else {
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction: request.transaction,
                application: request.application,
                accepted: false,
            });
            continue;
        };
        for (piece_index, &entity) in (0_u16..).zip(&applied_entities) {
            commands
                .entity(entity)
                .insert(TransportTrackConstructionPiece {
                    route_segment: track,
                    piece_index,
                });
        }
        edit.applied_entities = applied_entities;
        edit.application_in_flight = Some(request.application);
        connections.write(ConnectTransportTrackRequest {
            transaction: request.transaction,
            application: request.application,
            track,
            circuit: edit.circuit,
            from: from_endpoint,
            from_endpoint_index: edit.from_endpoint_index,
            to: destination_endpoint,
            to_endpoint_index: edit.to_endpoint_index,
            path_points: edit.path_points.clone(),
        });
    }
}

pub(super) fn acknowledge_completed_transport_track_topology_applications(
    mut commands: Commands,
    mut applied_connections: MessageReader<TransportTrackConnectionApplied>,
    mut rejected_connections: MessageReader<TransportTrackConnectionRejected>,
    mut applied_disconnections: MessageReader<TransportTrackDisconnectionApplied>,
    mut edits: Query<&mut PreparedTransportTrackConstructionEdit>,
    persistent_entities: Query<(Entity, &PersistentId)>,
    mut acknowledged: MessageWriter<TransportTrackConstructionEditApplicationAcknowledged>,
) {
    for applied in applied_connections.read() {
        let Ok(mut edit) = edits.get_mut(applied.transaction) else {
            continue;
        };
        if edit.applied_entities.first() != Some(&applied.track)
            || edit.application_in_flight != Some(applied.application)
        {
            continue;
        }
        edit.application_in_flight = None;
        if applied.application == EditApplication::InitialCommit {
            if let Some(preview) = edit.preview.take() {
                commands.entity(preview).despawn();
            }
        }
        acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
            transaction: applied.transaction,
            application: applied.application,
            accepted: true,
        });
    }
    for rejected in rejected_connections.read() {
        let Ok(mut edit) = edits.get_mut(rejected.transaction) else {
            continue;
        };
        if edit.applied_entities.first() != Some(&rejected.track)
            || edit.application_in_flight != Some(rejected.application)
        {
            continue;
        }
        for entity in edit.applied_entities.drain(..) {
            commands.entity(entity).despawn();
        }
        if let Some(endpoint) = edit.destination_junction_entity.and_then(|wanted_id| {
            persistent_entities
                .iter()
                .find_map(|(entity, id)| (*id == wanted_id).then_some(entity))
        }) {
            commands.entity(endpoint).despawn();
        }
        despawn_source_junction(&mut commands, &edit, &persistent_entities);
        edit.application_in_flight = None;
        acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
            transaction: rejected.transaction,
            application: rejected.application,
            accepted: false,
        });
    }
    for disconnected in applied_disconnections.read() {
        let Ok(mut edit) = edits.get_mut(disconnected.transaction) else {
            continue;
        };
        if edit.applied_entities.first() != Some(&disconnected.track)
            || edit.application_in_flight != Some(disconnected.application)
        {
            continue;
        }
        for entity in edit.applied_entities.drain(..) {
            commands.entity(entity).despawn();
        }
        if let Some(endpoint) = edit.destination_junction_entity.and_then(|wanted_id| {
            persistent_entities
                .iter()
                .find_map(|(entity, id)| (*id == wanted_id).then_some(entity))
        }) {
            commands.entity(endpoint).despawn();
        }
        despawn_source_junction(&mut commands, &edit, &persistent_entities);
        edit.application_in_flight = None;
        if let Some(preview) = edit.preview.take() {
            commands.entity(preview).despawn();
        }
        acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
            transaction: disconnected.transaction,
            application: disconnected.application,
            accepted: true,
        });
    }
}

fn despawn_source_junction(
    commands: &mut Commands,
    edit: &PreparedTransportTrackConstructionEdit,
    persistent_entities: &Query<(Entity, &PersistentId)>,
) {
    if let Some(endpoint) = edit.source_junction_entity.and_then(|wanted_id| {
        persistent_entities
            .iter()
            .find_map(|(entity, id)| (*id == wanted_id).then_some(entity))
    }) {
        commands.entity(endpoint).despawn();
    }
}

/// The step to apply after `done` steps of `application`, and how.
fn step_at(
    steps: &TransportTrackConstructionEditSteps,
    application: EditApplication,
    done: usize,
) -> Option<(Entity, EditApplication)> {
    match application {
        EditApplication::InitialCommit | EditApplication::Redo => {
            steps.steps.get(done).map(|step| (*step, application))
        }
        EditApplication::Undo => steps
            .steps
            .len()
            .checked_sub(done + 1)
            .map(|index| (steps.steps[index], application)),
        // Reverse the steps the failed application completed.
        EditApplication::FailureRollback => {
            let undo = match steps.last_application? {
                EditApplication::Undo => EditApplication::Redo,
                _ => EditApplication::Undo,
            };
            let applied_forward = steps.last_application != Some(EditApplication::Undo);
            let index = if applied_forward {
                steps.completed.checked_sub(done + 1)?
            } else {
                steps.steps.len().checked_sub(steps.completed)? + done
            };
            (done < steps.completed).then(|| (steps.steps[index], undo))
        }
    }
}

/// Applies a stepped transaction's route edits one after another, each once
/// the one before it is acknowledged; an undo runs them in reverse, and a
/// rollback reverses only the steps the failed application completed.
// It reads and writes the same messages, so it holds them with its own cursors.
pub(super) fn apply_stepped_transport_track_construction_edits(
    mut step_requests: ResMut<Messages<ApplyPreparedTransportTrackConstructionEditRequest>>,
    mut acknowledged: ResMut<Messages<TransportTrackConstructionEditApplicationAcknowledged>>,
    mut request_cursor: Local<MessageCursor<ApplyPreparedTransportTrackConstructionEditRequest>>,
    mut acknowledgement_cursor: Local<
        MessageCursor<TransportTrackConstructionEditApplicationAcknowledged>,
    >,
    mut transactions: Query<(Entity, &mut TransportTrackConstructionEditSteps)>,
) {
    let wanted = request_cursor
        .read(&step_requests)
        .filter(|request| transactions.contains(request.transaction))
        .copied()
        .collect::<Vec<_>>();
    for request in wanted {
        let Ok((_, mut steps)) = transactions.get_mut(request.transaction) else {
            continue;
        };
        if steps.in_flight.is_some() {
            continue;
        }
        if request.application != EditApplication::FailureRollback {
            steps.last_application = Some(request.application);
            steps.completed = 0;
        }
        match step_at(&steps, request.application, 0) {
            Some((step, application)) => {
                steps.in_flight = Some((request.application, 0));
                step_requests.write(ApplyPreparedTransportTrackConstructionEditRequest {
                    transaction: step,
                    application,
                });
            }
            None => {
                acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                    transaction: request.transaction,
                    application: request.application,
                    accepted: true,
                });
            }
        }
    }
    let step_acknowledgements = acknowledgement_cursor
        .read(&acknowledged)
        .copied()
        .collect::<Vec<_>>();
    for step_acknowledgement in step_acknowledgements {
        let Some((transaction, mut steps)) = transactions.iter_mut().find(|(_, steps)| {
            steps.steps.contains(&step_acknowledgement.transaction) && steps.in_flight.is_some()
        }) else {
            continue;
        };
        let Some((application, done)) = steps.in_flight else {
            continue;
        };
        if step_at(&steps, application, done).map(|(step, _)| step)
            != Some(step_acknowledgement.transaction)
        {
            continue;
        }
        if !step_acknowledgement.accepted {
            // Reverse the steps already done, then report the failure.
            let failed = steps.failed.take().unwrap_or(application);
            if application != EditApplication::FailureRollback {
                if let Some((step, step_application)) =
                    step_at(&steps, EditApplication::FailureRollback, 0)
                {
                    steps.failed = Some(failed);
                    steps.in_flight = Some((EditApplication::FailureRollback, 0));
                    step_requests.write(ApplyPreparedTransportTrackConstructionEditRequest {
                        transaction: step,
                        application: step_application,
                    });
                    continue;
                }
            }
            steps.in_flight = None;
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction,
                application: failed,
                accepted: false,
            });
            continue;
        }
        let done = done + 1;
        if application != EditApplication::FailureRollback {
            steps.completed = done;
        }
        if let Some((step, step_application)) = step_at(&steps, application, done) {
            steps.in_flight = Some((application, done));
            step_requests.write(ApplyPreparedTransportTrackConstructionEditRequest {
                transaction: step,
                application: step_application,
            });
        } else {
            steps.in_flight = None;
            let failed = steps.failed.take();
            acknowledged.write(TransportTrackConstructionEditApplicationAcknowledged {
                transaction,
                application: failed.unwrap_or(application),
                accepted: failed.is_none(),
            });
        }
    }
}
