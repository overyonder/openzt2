use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    action::persistence::UiPersistenceAction, document::UiDocumentRole,
};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiPersistenceActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::GameViewDocumentOwner;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::UiRoleRequests;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::simulation_time::simulation_control_types::AuthoredUiTemporarySimulationPause;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationControl;
use openzt2_world_state::simulation_time::simulation_control_types::SimulationPauseRequest;

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    VerificationSessionStage,
};

#[derive(Resource, Default)]
pub(super) struct SavePauseEvidence {
    journey: Option<(usize, u64)>,
    transfer_clock: Option<u64>,
    transfer_owner: Option<Entity>,
    transfers: u32,
    cold_transfers: u32,
    completions: u32,
    frames: u32,
    pending_frames: u32,
    unpaused_frames: u32,
    advanced_ticks: u64,
    toggle_requests: u32,
}

fn prepare_save_pause_evidence(
    session: &VerificationJourneySession,
    evidence: &mut SavePauseEvidence,
) -> bool {
    if session.stage != VerificationSessionStage::Journey
        || !session
            .journeys
            .get(session.journey_index)
            .is_some_and(|journey| journey.name == "save-dialog-cancel")
    {
        return false;
    }
    let identity = session
        .journey_started_at
        .map(|(frame, _)| (session.journey_index, frame));
    if evidence.journey != identity {
        *evidence = SavePauseEvidence {
            journey: identity,
            ..default()
        };
    }
    true
}

pub(super) fn observe_authored_save_pause_transfer_start(
    session: Res<VerificationJourneySession>,
    mut evidence: ResMut<SavePauseEvidence>,
    mut activations: MessageReader<UiNodeActivated>,
    actions: Query<(&UiPersistenceActions, &UiDocumentOwner)>,
    roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    dialog_owner: GameViewDocumentOwner,
    documents: Res<Assets<UiDocumentAsset>>,
    control: Option<Res<SimulationControl>>,
    clock: Option<Res<ZooClock>>,
) {
    let active = prepare_save_pause_evidence(&session, &mut evidence);
    for activation in activations.read() {
        if !active
            || !control
                .as_deref()
                .is_some_and(SimulationControl::temporary_ui_pause_is_held)
        {
            continue;
        }
        let Ok((actions, owner)) = actions.get(activation.node) else {
            continue;
        };
        let Ok((_, root, lifecycle_owner)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        if !actions.authored_action_records(document).any(|record| {
            record.trigger == activation.trigger
                && record.action == UiPersistenceAction::OpenSaveSlotCatalogueForSaving
        }) {
            continue;
        }
        let Some(clock) = clock.as_ref() else {
            continue;
        };
        let receiver_owner = dialog_owner.owner(lifecycle_owner.parent());
        evidence.transfers += 1;
        if !roots.iter().any(|(_, root, parent)| {
            parent.parent() == receiver_owner
                && documents.get(&root.document).is_some_and(|document| {
                    document.canonical_ui_document().role == UiDocumentRole::SaveGame
                })
        }) {
            evidence.cold_transfers += 1;
        }
        evidence.transfer_clock = Some(clock.tick);
        evidence.transfer_owner = Some(receiver_owner);
    }
}

pub(super) fn collect_save_pause_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut evidence: ResMut<SavePauseEvidence>,
    mut requests: MessageReader<SimulationPauseRequest>,
    control: Option<Res<SimulationControl>>,
    clock: Option<Res<ZooClock>>,
    roots: Query<(Entity, &UiDocumentRoot, &ChildOf)>,
    documents: Res<Assets<UiDocumentAsset>>,
    receiver_nodes: Query<(
        &UiNodeId,
        &UiDocumentOwner,
        Option<&AuthoredUiTemporarySimulationPause>,
        &InheritedVisibility,
    )>,
    pending_holds: Query<&ChildOf, (With<AuthoredUiTemporarySimulationPause>, Without<UiNodeId>)>,
    roles: Res<UiRoleRequests>,
) {
    let active = prepare_save_pause_evidence(&session, &mut evidence);
    for request in requests.read() {
        if active && *request == SimulationPauseRequest::ToggleRequested {
            evidence.toggle_requests += 1;
        }
    }
    if !active {
        return;
    }
    let pending_transfer = evidence.transfer_owner.is_some_and(|owner| {
        pending_holds.iter().any(|parent| parent.parent() == owner)
            || roles
                .temporary_pause_transfer_owners()
                .any(|pending| pending == owner)
    });
    let receiver_hold_acquired = roots.iter().any(|(entity, root, parent)| {
        if Some(parent.parent()) != evidence.transfer_owner {
            return false;
        }
        let Some(document) = documents.get(&root.document) else {
            return false;
        };
        let document = document.canonical_ui_document();
        if document.role != UiDocumentRole::SaveGame {
            return false;
        }
        // Save Game Shell owns the authored hold, while UI_ROOT_CHILD hides
        // its first child surface without hiding the shell itself.
        let Ok((_, _, Some(hold), shell_visibility)) = receiver_nodes.get(entity) else {
            return false;
        };
        if !hold.hold_is_requested() || !shell_visibility.get() {
            return false;
        }
        let surface_index = document
            .nodes
            .first()
            .and_then(|root| root.children.first());
        receiver_nodes.iter().any(|(id, owner, _, visibility)| {
            owner.0 == entity && Some(&id.index) == surface_index && visibility.get()
        })
    }) && control
        .as_ref()
        .is_some_and(|control| control.paused && control.temporary_ui_pause_is_held());
    if let Some(previous_tick) = evidence.transfer_clock {
        evidence.frames += 1;
        if !control.as_ref().is_some_and(|control| control.paused) {
            evidence.unpaused_frames += 1;
        }
        if let Some(clock) = clock.as_ref() {
            evidence.advanced_ticks += clock.tick.saturating_sub(previous_tick);
            evidence.transfer_clock = Some(clock.tick);
        }
        if pending_transfer {
            evidence.pending_frames += 1;
        }
        if !pending_transfer && receiver_hold_acquired {
            evidence.completions += 1;
            evidence.transfer_clock = None;
        }
    }
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    for (name, value) in [
        (
            "save_pause_receiver_hold_acquired",
            u32::from(receiver_hold_acquired),
        ),
        ("save_pause_transfer_pending", u32::from(pending_transfer)),
        ("save_pause_transfer_requests", evidence.transfers),
        ("save_pause_transfer_cold_requests", evidence.cold_transfers),
        ("save_pause_transfer_completions", evidence.completions),
        ("save_pause_transfer_observed_frames", evidence.frames),
        (
            "save_pause_transfer_pending_frames",
            evidence.pending_frames,
        ),
        (
            "save_pause_transfer_unpaused_frames",
            evidence.unpaused_frames,
        ),
        ("simulation_pause_toggle_requests", evidence.toggle_requests),
    ] {
        report
            .current_facts
            .insert(name.to_owned(), f64::from(value));
    }
    report.current_facts.insert(
        "save_pause_transfer_advanced_ticks".to_owned(),
        evidence.advanced_ticks as f64,
    );
    report.current_facts.insert(
        "simulation_requested_paused".to_owned(),
        control.as_ref().map_or(-1.0, |control| {
            f64::from(u8::from(control.requested_pause_state()))
        }),
    );
}
