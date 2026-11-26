use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use std::collections::BTreeMap;

use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;

use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveDeadline;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveProgress;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatus;
use openzt2_zoo_state::scenario::scenario_objective_types::ScenarioObjectiveStatusChanged;

use super::challenge_offer_types::{ScenarioChallengeOffer, ScenarioChallengeOfferState};

#[derive(Component, Default)]
pub(super) struct ScenarioObjectiveDeadlines {
    due: BTreeMap<(u64, Entity), ()>,
    by_objective: bevy::ecs::entity::EntityHashMap<u64>,
}

pub(super) fn index_changed_objective_deadlines(
    mut worlds: Query<&mut ScenarioObjectiveDeadlines>,
    changed: Query<
        (
            Entity,
            &WorldMember,
            &ScenarioObjectiveStatus,
            Option<&ScenarioObjectiveDeadline>,
        ),
        Or<(
            Changed<ScenarioObjectiveStatus>,
            Changed<ScenarioObjectiveDeadline>,
        )>,
    >,
    mut removed: RemovedComponents<ScenarioObjectiveDeadline>,
) {
    for entity in removed.read() {
        for mut deadlines in &mut worlds {
            if let Some(tick) = deadlines.by_objective.remove(&entity) {
                deadlines.due.remove(&(tick, entity));
            }
        }
    }
    for (entity, member, status, deadline) in &changed {
        let Ok(mut deadlines) = worlds.get_mut(member.root) else {
            continue;
        };
        if let Some(prior) = deadlines.by_objective.remove(&entity) {
            deadlines.due.remove(&(prior, entity));
        }
        if let Some(deadline) = deadline.filter(|_| *status == ScenarioObjectiveStatus::Active) {
            deadlines.by_objective.insert(entity, deadline.end_tick);
            deadlines.due.insert((deadline.end_tick, entity), ());
        }
    }
}

pub(super) fn expire_active_objectives(
    mut worlds: Query<(&ZooClock, &mut ScenarioObjectiveDeadlines)>,
    mut objectives: Query<(&mut ScenarioObjectiveStatus, &ScenarioObjectiveProgress)>,
    mut changed: MessageWriter<ScenarioObjectiveStatusChanged>,
) {
    for (clock, mut deadlines) in &mut worlds {
        while deadlines
            .due
            .first_key_value()
            .is_some_and(|((tick, _), _)| *tick <= clock.tick)
        {
            let Some(((_, entity), ())) = deadlines.due.pop_first() else {
                break;
            };
            deadlines.by_objective.remove(&entity);
            let Ok((mut status, progress)) = objectives.get_mut(entity) else {
                continue;
            };
            if *status == ScenarioObjectiveStatus::Active {
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
