use bevy::prelude::*;

use crate::plugins::immersive_modes::{
    immersive_mode_message_types::{
        EnterImmersiveMode, ImmersiveModeEntered, ImmersiveModeExited, ModeExitReason,
    },
    immersive_mode_state_types::{ActiveImmersiveMode, ImmersiveMode},
};

use super::verification_journey_state_types::{
    VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
    VerificationSessionStage,
};

pub(super) fn collect_walking_mode_entry_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut requests: MessageReader<EnterImmersiveMode>,
    mut entries: MessageReader<ImmersiveModeEntered>,
    mut exits: MessageReader<ImmersiveModeExited>,
    mut journey_counts: Local<(usize, [u32; 3])>,
    active: Query<&ActiveImmersiveMode>,
) {
    // Count canonical requests/outcomes throughout this journey, including
    // negative waits. Navigation between journeys does not earn input credit.
    let in_journey = matches!(
        session.stage,
        VerificationSessionStage::Journey | VerificationSessionStage::FinishAfterJourneyFailure
    );
    if journey_counts.0 != session.journey_index || !in_journey {
        *journey_counts = (session.journey_index, [0; 3]);
    }
    for request in requests.read() {
        if in_journey && request.mode == ImmersiveMode::SuperStaff {
            journey_counts.1[0] = journey_counts.1[0].saturating_add(1);
        }
    }
    for entry in entries.read() {
        if in_journey && entry.mode == ImmersiveMode::SuperStaff {
            journey_counts.1[1] = journey_counts.1[1].saturating_add(1);
        }
    }
    for exit in exits.read() {
        if in_journey
            && exit.mode == ImmersiveMode::SuperStaff
            && exit.reason == ModeExitReason::Cancelled
        {
            journey_counts.1[2] = journey_counts.1[2].saturating_add(1);
        }
    }
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    for (name, count) in [
        "walking_mode_entry_requests",
        "walking_mode_entries",
        "walking_mode_cancelled_exits",
    ]
    .into_iter()
    .zip(journey_counts.1)
    {
        report
            .current_facts
            .insert(name.to_owned(), f64::from(count));
    }
    let controllers = active
        .iter()
        .filter(|active| active.mode == ImmersiveMode::SuperStaff)
        .count();
    report.current_facts.insert(
        "walking_mode_controllers".to_owned(),
        f64::from(u32::try_from(controllers).unwrap_or(u32::MAX)),
    );
}
