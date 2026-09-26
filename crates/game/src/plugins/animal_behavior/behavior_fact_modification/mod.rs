use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::behavior::{
    action::modification::{
        BehaviorFact, BehaviorFactModification, BehaviorFactModificationOperation,
    },
    action_record::BehaviorAction,
};

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        animal_health::types::{FreezeAnimal, ThawAnimal},
        animal_lifecycle::types::Animal,
        animal_welfare::types::{
            AdjustBathroom, AdjustExercise, AdjustHealthNeed, AdjustHunger, AdjustHygiene,
            AdjustPrivacy, AdjustRest, AdjustSocial, AdjustStimulation, AdjustThirst,
        },
        feeding::container_quantity::{
            apply_authored_container_quantity_write, DrinkContainer, FoodContainer,
        },
        simulation_time::simulation_clock_types::ZooClock,
        staff::{
            staff_behavior_fact_completion_execution::behavior_fact_modification_is_owned_by_terminal_maintenance_system,
            staff_employment_types::Staff,
            staff_job_types::{CurrentJob, StaffJob},
        },
    },
};

use super::behavior_entity_role_resolution::resolve_behavior_entity_role_to_live_entity;
use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::plugins::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionState,
};

#[derive(SystemParam)]
pub(super) struct BehaviorFactModificationMessageWriters<'w> {
    bathroom: MessageWriter<'w, AdjustBathroom>,
    hunger: MessageWriter<'w, AdjustHunger>,
    thirst: MessageWriter<'w, AdjustThirst>,
    rest: MessageWriter<'w, AdjustRest>,
    privacy: MessageWriter<'w, AdjustPrivacy>,
    social: MessageWriter<'w, AdjustSocial>,
    exercise: MessageWriter<'w, AdjustExercise>,
    stimulation: MessageWriter<'w, AdjustStimulation>,
    health: MessageWriter<'w, AdjustHealthNeed>,
    hygiene: MessageWriter<'w, AdjustHygiene>,
    freeze_animal: MessageWriter<'w, FreezeAnimal>,
    thaw_animal: MessageWriter<'w, ThawAnimal>,
}

pub(super) fn apply_supported_current_behavior_fact_modifications(
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    mut tasks: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&mut BehaviorRandomStream>,
            Option<&CurrentJob>,
        ),
        Or<(With<Animal>, With<Staff>)>,
    >,
    mut modification_messages: BehaviorFactModificationMessageWriters,
    mut food_containers: Query<(Entity, &mut FoodContainer)>,
    mut drink_containers: Query<(Entity, &mut DrinkContainer)>,
    animals: Query<(), With<Animal>>,
    jobs: Query<&StaffJob>,
    mut commands: Commands,
) {
    for (entity, mut task, mut random, current_job) in &mut tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::FactModifications(modifications)) = assets
            .get(&task.document)
            .and_then(|asset| find_current_behavior_task_action(asset, &task))
        else {
            continue;
        };
        let current_staff_job_kind =
            current_job.and_then(|current_job| jobs.get(current_job.job).ok().map(|job| job.kind));
        let action_has_terminal_maintenance_owner = modifications.iter().any(|modification| {
            behavior_fact_modification_is_owned_by_terminal_maintenance_system(
                modification,
                current_staff_job_kind,
            )
        });
        let action_is_entirely_terminal_maintenance = !modifications.is_empty()
            && modifications.iter().all(|modification| {
                behavior_fact_modification_is_owned_by_terminal_maintenance_system(
                    modification,
                    current_staff_job_kind,
                )
            });
        if action_has_terminal_maintenance_owner && !action_is_entirely_terminal_maintenance {
            mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            continue;
        }
        if action_is_entirely_terminal_maintenance {
            // Leave these actions for the existing staff completion owner in Act;
            // this shared executor must not advance maintenance-owned work.
            continue;
        }
        if !modifications.iter().all(|modification| {
            let Some(affected_entity) = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                entity,
                task.target,
            ) else {
                return false;
            };
            behavior_fact_modification_has_runtime_owner(
                modification,
                affected_entity,
                &food_containers,
                &drink_containers,
                &animals,
            )
        }) {
            mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            continue;
        }
        for modification in modifications {
            let animal = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                entity,
                task.target,
            )
            .expect("modification roles were checked before application");
            let value_q16 = modification
                .modification_value
                .sample_q16(|count| {
                    random
                        .as_deref_mut()
                        .and_then(|random| random.range_u32(count))
                        .unwrap_or(0)
                })
                .expect("context-free scalar was validated before application");
            // Authored needs measure deprivation on 0..100; welfare messages
            // adjust wellness on 0..1000, as in species need initialization.
            let wellness_delta_q16 = value_q16.saturating_mul(-10);
            match modification.modified_fact {
                BehaviorFact::Bathroom => {
                    modification_messages.bathroom.write(AdjustBathroom {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Hunger => {
                    modification_messages.hunger.write(AdjustHunger {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Thirst => {
                    modification_messages.thirst.write(AdjustThirst {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Rest => {
                    modification_messages.rest.write(AdjustRest {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Privacy => {
                    modification_messages.privacy.write(AdjustPrivacy {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Social => {
                    modification_messages.social.write(AdjustSocial {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Exercise => {
                    modification_messages.exercise.write(AdjustExercise {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Stimulation => {
                    modification_messages.stimulation.write(AdjustStimulation {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Health => {
                    modification_messages.health.write(AdjustHealthNeed {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Hygiene => {
                    modification_messages.hygiene.write(AdjustHygiene {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::Frozen => {
                    if modification.modification_operation
                        == BehaviorFactModificationOperation::Clear
                        || value_q16 == 0
                    {
                        modification_messages
                            .thaw_animal
                            .write(ThawAnimal { animal });
                    } else {
                        modification_messages
                            .freeze_animal
                            .write(FreezeAnimal { animal });
                    }
                }
                BehaviorFact::FoodLevel => {
                    let target = resolve_behavior_entity_role_to_live_entity(
                        modification.affected_entity_role,
                        entity,
                        task.target,
                    )
                    .expect("FoodLevel target was validated before application");
                    if let Ok((_, mut container)) = food_containers.get_mut(target) {
                        container.amount_q16 = apply_authored_container_quantity_write(
                            container.amount_q16,
                            container.capacity_q16,
                            value_q16,
                        );
                    } else if let Ok((_, mut container)) = drink_containers.get_mut(target) {
                        container.amount_q16 = apply_authored_container_quantity_write(
                            container.amount_q16,
                            container.capacity_q16,
                            value_q16,
                        );
                    } else {
                        unreachable!("FoodLevel target was validated before application");
                    }
                }
                _ => {
                    unreachable!("unsupported fact modifications were rejected before application")
                }
            };
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}

fn behavior_fact_modification_has_runtime_owner(
    modification: &BehaviorFactModification,
    affected_entity: Entity,
    food_containers: &Query<(Entity, &mut FoodContainer)>,
    drink_containers: &Query<(Entity, &mut DrinkContainer)>,
    animals: &Query<(), With<Animal>>,
) -> bool {
    if matches!(
        modification.modification_value,
        openzt2_game_data::behavior::scalar::BehaviorScalarQ16::PriceEffectQ16(_)
    ) {
        return false;
    }
    if modification.modified_fact == BehaviorFact::FoodLevel {
        return modification.affected_entity_role
            == openzt2_game_data::behavior::action::entity_role::BehaviorEntityRole::Target
            && modification.modification_operation == BehaviorFactModificationOperation::Add
            && (food_containers.contains(affected_entity)
                ^ drink_containers.contains(affected_entity));
    }
    if !animals.contains(affected_entity) {
        return false;
    }
    matches!(
        modification.modified_fact,
        BehaviorFact::Hunger
            | BehaviorFact::Bathroom
            | BehaviorFact::Thirst
            | BehaviorFact::Rest
            | BehaviorFact::Privacy
            | BehaviorFact::Social
            | BehaviorFact::Exercise
            | BehaviorFact::Stimulation
            | BehaviorFact::Health
            | BehaviorFact::Hygiene
            | BehaviorFact::Frozen
    )
}

#[cfg(test)]
mod tests;
