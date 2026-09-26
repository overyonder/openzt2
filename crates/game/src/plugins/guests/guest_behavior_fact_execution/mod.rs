//! Applies authored behavior changes to the guest owner's existing need facts.

use bevy::prelude::*;
use openzt2_game_data::behavior::{
    action::modification::{BehaviorFact, BehaviorFactModificationOperation},
    action_record::BehaviorAction,
};

use crate::{
    assets::behavior::behavior_asset_types::BehaviorDocumentAsset,
    plugins::{
        animal_behavior::{
            behavior_entity_role_resolution::resolve_behavior_entity_role_to_live_entity,
            behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation,
        },
        behavior_task_execution_types::{
            advance_behavior_task_to_next_action, find_current_behavior_task_action,
            BehaviorTaskExecutionState, PendingBehaviorTaskFailure,
        },
        economy::price_effect::BehaviorPriceEffectContext,
        simulation_time::simulation_clock_types::ZooClock,
    },
};

use super::{guest_simulation_calculations::apply_q16, guest_simulation_types::*};

/// Source instance attributes start at zero and accumulate authored purchase outcomes.
#[derive(Component, Default)]
pub(crate) struct GuestFoodPurchaseCounts {
    pub(crate) favorite_q16: i32,
    pub(crate) nonfavorite_q16: i32,
}

pub(super) fn apply_authored_guest_behavior_fact_modifications(
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    price_effect: BehaviorPriceEffectContext,
    satisfaction: Query<(), With<GuestSatisfaction>>,
    mut satisfaction_changes: MessageWriter<AdjustGuestSatisfaction>,
    mut tasks: Query<
        (Entity, &mut BehaviorTaskExecutionState, &mut GuestRng),
        (With<Guest>, Without<PendingBehaviorTaskFailure>),
    >,
    mut needs: Query<
        (
            &mut GuestHunger,
            &mut GuestThirst,
            &mut GuestDessert,
            &mut GuestGift,
            &mut GuestEnergy,
            &mut GuestRestroom,
            &mut GuestSocial,
            &mut GuestDeparturePoints,
        ),
        With<Guest>,
    >,
    mut viewing_needs: Query<&mut GuestViewingNeed, With<Guest>>,
    mut commands: Commands,
) {
    for (actor, mut task, mut random) in &mut tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::FactModifications(modifications)) = assets
            .get(&task.document)
            .and_then(|asset| find_current_behavior_task_action(asset, &task))
        else {
            continue;
        };
        // Validate the whole action before changing a need or consuming RNG.
        // An unresolved role or unsupported fact must enter the authored failure path.
        if !modifications.iter().all(|modification| {
            modification.modification_operation == BehaviorFactModificationOperation::Add
                && matches!(
                    modification.modified_fact,
                    BehaviorFact::Hunger
                        | BehaviorFact::Thirst
                        | BehaviorFact::Dessert
                        | BehaviorFact::Gift
                        | BehaviorFact::ViewAnimals
                        | BehaviorFact::Rest
                        | BehaviorFact::Bathroom
                        | BehaviorFact::Social
                        | BehaviorFact::DeparturePoints
                        | BehaviorFact::Happiness
                        | BehaviorFact::AteFavoriteFood
                        | BehaviorFact::AteNonFavoriteFood
                )
                && resolve_behavior_entity_role_to_live_entity(
                    modification.affected_entity_role,
                    actor,
                    task.target,
                )
                .is_some_and(|subject| {
                    needs.contains(subject)
                        && (modification.modified_fact != BehaviorFact::ViewAnimals
                            || viewing_needs.contains(subject))
                        && (modification.modified_fact != BehaviorFact::Happiness
                            || satisfaction.contains(subject))
                })
                && price_effect
                    .sample(modification.modification_value, task.target, |_| 0)
                    .is_some()
        }) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        for modification in modifications {
            let subject = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                actor,
                task.target,
            )
            .expect("validated behavior subject");
            let authored_delta = price_effect
                .sample(modification.modification_value, task.target, |count| {
                    random.0.range_u32(count).unwrap_or(0)
                })
                .expect("scalar context was validated before application");
            if modification.modified_fact == BehaviorFact::ViewAnimals {
                if let Ok(mut need) = viewing_needs.get_mut(subject) {
                    need.add_source_delta(authored_delta);
                }
                continue;
            }
            if modification.modified_fact == BehaviorFact::Happiness {
                // Source happiness is deprivation: bad-smell +3 and a good
                // tour -8 in Guest.xml. The satisfaction message is wellness.
                satisfaction_changes.write(AdjustGuestSatisfaction {
                    guest: subject,
                    delta_q16: authored_delta.saturating_neg(),
                });
                continue;
            }
            if matches!(
                modification.modified_fact,
                BehaviorFact::AteFavoriteFood | BehaviorFact::AteNonFavoriteFood
            ) {
                let favorite = modification.modified_fact == BehaviorFact::AteFavoriteFood;
                commands.queue(move |world: &mut World| {
                    let Ok(mut entity) = world.get_entity_mut(subject) else {
                        return;
                    };
                    if !entity.contains::<GuestFoodPurchaseCounts>() {
                        entity.insert(GuestFoodPurchaseCounts::default());
                    }
                    let mut counts = entity
                        .get_mut::<GuestFoodPurchaseCounts>()
                        .expect("inserted purchase counters");
                    let value = if favorite {
                        &mut counts.favorite_q16
                    } else {
                        &mut counts.nonfavorite_q16
                    };
                    *value = value.saturating_add(authored_delta);
                });
                continue;
            }
            let Ok((
                mut hunger,
                mut thirst,
                mut dessert,
                mut gift,
                mut rest,
                mut bathroom,
                mut social,
                mut departure_points,
            )) = needs.get_mut(subject)
            else {
                continue;
            };
            if modification.modified_fact == BehaviorFact::DeparturePoints {
                departure_points.0 = departure_points.0.saturating_add(authored_delta);
                continue;
            }
            // Native needs are pressure percentages; canonical guest components
            // are wellness permille, as in their source initialization and decay.
            let delta = authored_delta.saturating_mul(-10);
            let (value, residual) = match modification.modified_fact {
                BehaviorFact::Hunger => {
                    let need = &mut *hunger;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Thirst => {
                    let need = &mut *thirst;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Dessert => {
                    let need = &mut *dessert;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Gift => {
                    let need = &mut *gift;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Rest => {
                    let need = &mut *rest;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Bathroom => {
                    let need = &mut *bathroom;
                    (&mut need.value, &mut need.residual_q16)
                }
                BehaviorFact::Social => {
                    let need = &mut *social;
                    (&mut need.value, &mut need.residual_q16)
                }
                _ => unreachable!("validated behavior fact"),
            };
            apply_q16(value, residual, delta);
        }
        advance_behavior_task_to_next_action(&mut task, clock.tick);
    }
}
