use bevy::prelude::*;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        running_actions::{RunBehaviorTermination, RunningBehaviorInstruction, WhenBehaviorAction},
        BehaviorTaskExecutionState,
    },
    guests::guest_simulation_types::{
        DeparturePending, Guest, GuestDeparted, GuestEducation, GuestSatisfaction,
    },
};

pub(super) fn remove_guest_after_authored_departure(
    activation: On<WhenBehaviorAction<RunBehaviorTermination>>,
    guests: Query<
        (
            &BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTermination,
            &WorldMember,
            &GuestSatisfaction,
            &GuestEducation,
        ),
        (With<Guest>, Without<DeparturePending>),
    >,
    mut departed: MessageWriter<GuestDeparted>,
    mut commands: Commands,
) {
    let guest = activation.entity;
    let Ok((task, instruction, termination, world, satisfaction, education)) = guests.get(guest)
    else {
        return;
    };
    if !instruction.is_current(task) || !termination.0.kill_subject {
        return;
    }
    departed.write(GuestDeparted {
        root: world.root,
        guest,
        satisfaction_permille: satisfaction.0,
        education_permille: education.0,
    });
    commands.entity(guest).insert_if_new(DeparturePending);
}

pub(super) fn departure_capacity_allows_score(
    admissions_open: bool,
    departure_points_q16: i32,
    observed: usize,
    required: usize,
    available_tour: Option<(f32, f32)>,
) -> bool {
    !admissions_open
        || departure_points_q16 <= 0
        || (observed >= required && available_tour.is_none_or(|(need, cutoff)| need <= cutoff))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_gate_applies_view_capacity_only_while_departure_points_are_positive() {
        assert!(!departure_capacity_allows_score(true, 1, 0, 2, None));
        assert!(departure_capacity_allows_score(true, 0, 0, 2, None));
        assert!(departure_capacity_allows_score(false, 1, 0, 2, None));
    }

    #[test]
    fn available_tour_only_blocks_departure_above_the_tour_cutoff() {
        assert!(departure_capacity_allows_score(
            true,
            1,
            2,
            2,
            Some((50.0, 50.0))
        ));
        assert!(!departure_capacity_allows_score(
            true,
            1,
            2,
            2,
            Some((50.01, 50.0))
        ));
    }
}

#[derive(Component)]
pub struct GuestDeparturePressurePolicy {
    pub low_happiness_cutoff_q16: i32,
    pub low_happiness_delta_q16: i32,
    pub low_cash_delta_q16: i32,
}

pub(super) fn update_due_guest_departure_pressure(
    mut updates: MessageReader<
        openzt2_zoo_state::guests::guest_simulation_types::GuestNeedAdjustmentDue,
    >,
    mut guests: Query<(
        &GuestDeparturePressurePolicy,
        &GuestSatisfaction,
        &openzt2_zoo_state::economy::facility_economy_types::Wallet,
        &mut openzt2_zoo_state::guests::guest_simulation_types::GuestDeparturePoints,
    )>,
) {
    for update in updates.read() {
        let Ok((policy, happiness, bank, mut points)) = guests.get_mut(update.0) else {
            continue;
        };
        let mut delta = 0_i32;
        if happiness.source_q16() > policy.low_happiness_cutoff_q16 {
            delta = delta.saturating_add(policy.low_happiness_delta_q16);
        }
        if !bank.0.is_positive() {
            delta = delta.saturating_add(policy.low_cash_delta_q16);
        }
        let next = points.0.saturating_add(delta).max(0);
        points
            .reborrow()
            .map_unchanged(|points| &mut points.0)
            .set_if_neq(next);
    }
}
