use bevy::prelude::*;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;

use super::challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState};

pub(super) fn expire_active_objectives(
    clock: Res<ZooClock>,
    mut objectives: Query<(
        Entity,
        &ScenarioObjectiveDeadline,
        &mut ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
    )>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
) {
    for (entity, deadline, mut status, progress) in &mut objectives {
        if *status == ScenarioObjectiveStatus::Active && clock.tick >= deadline.end_tick {
            *status = ScenarioObjectiveStatus::Failed;
            changed.write(ScenarioObjectiveStatusChanged {
                objective: entity,
                status: *status,
                current: progress.current,
                target: progress.target,
            });
        }
    }
}

pub(super) fn remove_expired_challenge_offers(
    mut commands: Commands,
    clock: Res<ZooClock>,
    offers: Query<(Entity, &ScenarioChallengeOffer)>,
) {
    for (entity, offer) in &offers {
        if offer.state == ScenarioChallengeOfferState::Offered && clock.tick >= offer.expires_tick {
            commands.entity(entity).despawn();
        }
    }
}
