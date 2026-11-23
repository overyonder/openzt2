use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;

use super::challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState};

pub(super) fn expire_active_objectives(
    clocks: Query<&ZooClock>,
    mut objectives: Query<(
        Entity,
        &ScenarioObjectiveDeadline,
        &WorldMember,
        &mut ScenarioObjectiveStatus,
        &ScenarioObjectiveProgress,
    )>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
) {
    for (entity, deadline, member, mut status, progress) in &mut objectives {
        if *status == ScenarioObjectiveStatus::Active
            && clocks
                .get(member.root)
                .is_ok_and(|clock| clock.tick >= deadline.end_tick)
        {
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
    clocks: Query<&ZooClock>,
    offers: Query<(Entity, &ScenarioChallengeOffer, &WorldMember)>,
) {
    for (entity, offer, member) in &offers {
        if offer.state == ScenarioChallengeOfferState::Offered
            && clocks
                .get(member.root)
                .is_ok_and(|clock| clock.tick >= offer.expires_tick)
        {
            commands.entity(entity).despawn();
        }
    }
}
