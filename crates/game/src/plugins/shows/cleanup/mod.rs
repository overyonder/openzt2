use bevy::prelude::*;

use crate::plugins::animal_lifecycle::types::Animal;

use super::show_schedule_types::ScheduledShowPerformancePlan;

pub(super) fn remove_departed_animals_from_scheduled_show_performances(
    mut removed_animals: RemovedComponents<Animal>,
    mut schedules: Query<(Entity, &mut ScheduledShowPerformancePlan)>,
) {
    for animal in removed_animals.read() {
        for (_, mut schedule) in &mut schedules {
            remove_performer_from_scheduled_show_trick_performances(&mut schedule, animal);
        }
    }
}

fn remove_performer_from_scheduled_show_trick_performances(
    scheduled_show_performance_plan: &mut ScheduledShowPerformancePlan,
    removed_performer: Entity,
) {
    scheduled_show_performance_plan
        .trick_performances
        .retain(|trick_performance| trick_performance.performer != removed_performer);
}
