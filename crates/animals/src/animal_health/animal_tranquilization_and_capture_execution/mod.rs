use bevy::{ecs::entity::EntityHashMap, platform::collections::HashSet, prelude::*};
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use std::time::Duration;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::camera::camera_runtime_state_types::ZooCamera;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::staff::staff_employment_types::Staff;
use openzt2_zoo_state::staff::staff_job_types::CurrentJob;
use openzt2_zoo_state::staff::staff_job_types::JobClaim;
use openzt2_zoo_state::staff::staff_job_types::JobProgress;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobBehaviorTask;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    tranquilizer_eligibility_calculation::authored_tranquilizer_allows_animal_state,
    types::{
        AnimalCaptured, CaptureAnimalRequest, Dead, Escaped, Rampaging, RecoveringFromTranquilizer,
        TranquilizeRequest, Tranquilized, TranquilizerTool,
    },
};

#[cfg(test)]
mod tests;

#[derive(Component)]
pub(super) struct TranquilizerDoseTimer(Timer);

#[derive(Component)]
pub(super) struct TranquilizerRecoveryTimer(Timer);

// Each fixed update consumes one authored timer tick.
const DOSE_TICK: Duration = Duration::from_nanos(1);

fn fixed_tick_timer(ticks: u32) -> Timer {
    Timer::new(Duration::from_nanos(u64::from(ticks)), TimerMode::Once)
}

fn remaining_dose_ticks(timer: &Timer) -> u32 {
    u32::try_from(timer.remaining().as_nanos()).unwrap_or(u32::MAX)
}

pub(super) fn discard_tranquilizer_dose_timer(
    removed: On<Remove<Tranquilized>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<TranquilizerDoseTimer>();
}

pub(super) fn discard_tranquilizer_recovery_timer(
    removed: On<Remove<RecoveringFromTranquilizer>>,
    mut commands: Commands,
) {
    commands
        .entity(removed.entity)
        .try_remove::<TranquilizerRecoveryTimer>();
}

pub(super) fn validate_tranquilizer_requests_and_apply_timed_tranquilization(
    mut commands: Commands,
    mut tranquilizer_requests: MessageReader<TranquilizeRequest>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut animals: Query<
        (
            &GlobalTransform,
            Option<&Escaped>,
            Option<&Rampaging>,
            Option<&mut Tranquilized>,
            Option<&mut TranquilizerDoseTimer>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    tranquilizer_sources: Query<
        &GlobalTransform,
        Or<(With<Staff>, With<TranquilizerTool>, With<ZooCamera>)>,
    >,
) {
    let Some(world_definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let Some(tranquilizer_mode) = world_definitions.tranquilizer_mode() else {
        return;
    };
    let maximum_range_m = tranquilizer_mode.range_cm as f32 / 100.0;
    let maximum_range_squared = maximum_range_m * maximum_range_m;
    let mut first_doses: Vec<(Entity, Tranquilized)> = Vec::new();
    let mut first_dose_indices: EntityHashMap<usize> = EntityHashMap::default();
    for request in tranquilizer_requests.read() {
        let Ok(source_transform) = tranquilizer_sources.get(request.source) else {
            continue;
        };
        let Ok((animal_transform, escaped, rampaging, existing_tranquilization, timer)) =
            animals.get_mut(request.animal)
        else {
            continue;
        };
        let Some(tranquilizer_definition) =
            world_definitions.find_tranquilizer(request.tranquilizer)
        else {
            continue;
        };
        if !authored_tranquilizer_allows_animal_state(
            tranquilizer_definition.eligible_states,
            escaped.is_some(),
            rampaging.is_some(),
        ) || source_transform
            .translation()
            .distance_squared(animal_transform.translation())
            > maximum_range_squared
        {
            continue;
        }
        let dose = Tranquilized {
            remaining_ticks: tranquilizer_definition.duration_ticks,
            recovery_ticks: tranquilizer_definition.recovery_ticks,
        };
        if let Some(mut current) = existing_tranquilization {
            let mut next = *current;
            refresh_tranquilization(&mut next, dose);
            if let Some(mut timer) = timer {
                if remaining_dose_ticks(&timer.0) < next.remaining_ticks {
                    timer.0 = fixed_tick_timer(next.remaining_ticks);
                }
            }
            current.set_if_neq(next);
        } else {
            let index = *first_dose_indices.entry(request.animal).or_insert_with(|| {
                first_doses.push((request.animal, dose));
                first_doses.len() - 1
            });
            refresh_tranquilization(&mut first_doses[index].1, dose);
        }
    }
    for (animal, dose) in first_doses {
        commands.entity(animal).insert((
            dose,
            TranquilizerDoseTimer(fixed_tick_timer(dose.remaining_ticks)),
        ));
    }
}

fn refresh_tranquilization(current: &mut Tranquilized, dose: Tranquilized) {
    current.remaining_ticks = current.remaining_ticks.max(dose.remaining_ticks);
    current.recovery_ticks = dose.recovery_ticks;
}

pub(super) fn validate_staff_capture_requests_and_clear_animal_emergency_state(
    mut commands: Commands,
    mut capture_requests: MessageReader<CaptureAnimalRequest>,
    animals: Query<
        (),
        (
            With<Animal>,
            With<WorldMember>,
            With<Escaped>,
            With<Tranquilized>,
            Without<Dead>,
        ),
    >,
    staff: Query<&CurrentJob, With<Staff>>,
    jobs: Query<(&StaffJob, &JobClaim, &StaffJobBehaviorTask), With<JobProgress>>,
    mut cancellations: MessageReader<CancelStaffJobRequest>,
    mut cancelled_jobs: Local<HashSet<Entity>>,
    mut animal_captured_messages: MessageWriter<AnimalCaptured>,
) {
    cancelled_jobs.clear();
    cancelled_jobs.extend(cancellations.read().map(|request| request.job));
    for request in capture_requests.read() {
        if cancelled_jobs.contains(&request.job)
            || !animals.contains(request.animal)
            || !staff
                .get(request.staff)
                .is_ok_and(|current| current.job() == request.job)
            || !jobs.get(request.job).is_ok_and(|(job, claim, behavior)| {
                job.kind == Some(StaffJobKind::Capture)
                    && job.target == request.animal
                    && claim.staff == request.staff
                    && behavior.execution_id == Some(request.execution_id)
            })
        {
            continue;
        }
        commands
            .entity(request.animal)
            .remove::<(Escaped, Rampaging, Tranquilized)>();
        animal_captured_messages.write(AnimalCaptured {
            animal: request.animal,
            staff: request.staff,
            job: request.job,
            execution_id: request.execution_id,
        });
    }
}

pub(super) fn advance_animal_tranquilization_and_begin_recovery_period(
    mut commands: Commands,
    mut tranquilized_animals: Query<
        (
            Entity,
            &mut Tranquilized,
            Option<&mut TranquilizerDoseTimer>,
        ),
        (With<Animal>, Without<Dead>),
    >,
    mut recovery: Query<(
        &mut RecoveringFromTranquilizer,
        Option<&mut TranquilizerRecoveryTimer>,
    )>,
) {
    for (animal, mut tranquilized, timer) in &mut tranquilized_animals {
        let remaining = if let Some(mut timer) = timer {
            remaining_dose_ticks(timer.0.tick(DOSE_TICK))
        } else {
            let mut timer = fixed_tick_timer(tranquilized.remaining_ticks);
            let remaining = remaining_dose_ticks(timer.tick(DOSE_TICK));
            if remaining != 0 {
                commands.entity(animal).insert(TranquilizerDoseTimer(timer));
            }
            remaining
        };
        tranquilized.remaining_ticks = remaining;
        if remaining == 0 {
            let recovery_ticks = tranquilized.recovery_ticks;
            let mut animal_commands = commands.entity(animal);
            animal_commands.remove::<Tranquilized>();
            if recovery_ticks != 0 {
                let next = RecoveringFromTranquilizer {
                    remaining_ticks: recovery_ticks,
                };
                let timer = TranquilizerRecoveryTimer(fixed_tick_timer(recovery_ticks));
                if let Ok((mut current, current_timer)) = recovery.get_mut(animal) {
                    current.set_if_neq(next);
                    if let Some(mut current_timer) = current_timer {
                        *current_timer = timer;
                    } else {
                        animal_commands.insert(timer);
                    }
                } else {
                    animal_commands.insert((next, timer));
                }
            }
        }
    }
}

pub(super) fn advance_and_complete_animal_tranquilizer_recovery_periods(
    mut commands: Commands,
    mut recovering_animals: Query<
        (
            Entity,
            &mut RecoveringFromTranquilizer,
            Option<&mut TranquilizerRecoveryTimer>,
        ),
        (With<Animal>, Without<Dead>),
    >,
) {
    for (animal, mut recovery, timer) in &mut recovering_animals {
        let remaining = if let Some(mut timer) = timer {
            remaining_dose_ticks(timer.0.tick(DOSE_TICK))
        } else {
            let mut timer = fixed_tick_timer(recovery.remaining_ticks);
            let remaining = remaining_dose_ticks(timer.tick(DOSE_TICK));
            if remaining != 0 {
                commands
                    .entity(animal)
                    .insert(TranquilizerRecoveryTimer(timer));
            }
            remaining
        };
        recovery.remaining_ticks = remaining;
        if remaining == 0 {
            commands
                .entity(animal)
                .remove::<RecoveringFromTranquilizer>();
        }
    }
}
