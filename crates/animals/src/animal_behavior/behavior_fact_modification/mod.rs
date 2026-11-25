use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_zoo_state::authored_float_attributes::AuthoredFloatAttributes;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorFactModifications, RunningBehaviorInstruction,
};
use openzt2_zoo_state::locomotion::locomotion_types::CaffeineFlags;
mod consumable_compatibility;
use openzt2_game_data::behavior::action::modification::{
    BehaviorFact, BehaviorFactModification, BehaviorFactModificationOperation,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_behavior::animal_lineage::{AnimalMate, PregnancyFather};
use openzt2_zoo_state::animal_health::types::ThawAnimal;
use openzt2_zoo_state::animal_health::types::{Escaped, FreezeAnimal, Rampaging};
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::Pregnancy;
use openzt2_zoo_state::animal_welfare::types::AdjustBathroom;
use openzt2_zoo_state::animal_welfare::types::AdjustExercise;
use openzt2_zoo_state::animal_welfare::types::AdjustHealthNeed;
use openzt2_zoo_state::animal_welfare::types::AdjustHunger;
use openzt2_zoo_state::animal_welfare::types::AdjustHygiene;
use openzt2_zoo_state::animal_welfare::types::AdjustPrivacy;
use openzt2_zoo_state::animal_welfare::types::AdjustReproduction;
use openzt2_zoo_state::animal_welfare::types::AdjustRest;
use openzt2_zoo_state::animal_welfare::types::AdjustSocial;
use openzt2_zoo_state::animal_welfare::types::AdjustStimulation;
use openzt2_zoo_state::animal_welfare::types::AdjustThirst;
use openzt2_zoo_state::feeding::container_quantity::apply_authored_container_quantity_write;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::staff::staff_employment_types::Staff;

use openzt2_zoo_state::guests::guest_simulation_types::{
    AdjustGuestSatisfaction, GuestAttacked, GuestDeparturePoints,
};

use super::behavior_entity_role_resolution::resolve_behavior_entity_role_to_live_entity;
use super::behavior_random_stream_state::BehaviorRandomStream;
use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

#[derive(SystemParam)]
pub(super) struct BehaviorFactModificationMessageWriters<'w> {
    bathroom: MessageWriter<'w, AdjustBathroom>,
    reproduction: MessageWriter<'w, AdjustReproduction>,
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
    guest_satisfaction: MessageWriter<'w, AdjustGuestSatisfaction>,
}

use openzt2_zoo_state::{
    animal_health::types::Dead,
    aquatic::aquatic_simulation_types::{
        AquaticAnimal, Tank, TankGeometry, WaterFilter, WaterQuality,
    },
    habitat::habitat_types::{Containment, HabitatMember},
    maintenance::maintenance_types::{AdjustFacilityContainedWaste, FacilityContainedWaste},
    staff::staff_job_types::TankCleaningRequestSite,
};

#[derive(SystemParam)]
pub(super) struct MaintenanceFacts<'w, 's> {
    tanks: Query<'w, 's, (&'static TankGeometry, &'static mut WaterQuality), With<Tank>>,
    sites: Query<'w, 's, &'static TankCleaningRequestSite>,
    residents: Query<
        'w,
        's,
        (&'static HabitatMember, &'static Containment),
        (With<AquaticAnimal>, With<Animal>, Without<Dead>),
    >,
    filters: Query<'w, 's, &'static mut WaterFilter>,
    trash: Query<'w, 's, &'static FacilityContainedWaste>,
    trash_changes: MessageWriter<'w, AdjustFacilityContainedWaste>,
    object_ids: Query<'w, 's, &'static openzt2_world_state::world_spawn::world_membership_types::DefinitionId>,
    object_facts: Query<'w, 's, &'static mut AuthoredFloatAttributes>,
    definitions: Option<Res<'w, openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions>>,
    assets: Option<Res<'w, Assets<openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset>>>,
}

impl MaintenanceFacts<'_, '_> {
    fn initial_object_fact(&self, object: Entity, key: openzt2_game_data::AssetId) -> Option<f32> {
        let definition = self.object_ids.get(object).ok()?;
        let definitions = self.definitions.as_ref()?.get(self.assets.as_ref()?)?;
        let values = &definitions
            .find_object(definition.0)?
            .authored_runtime
            .numeric_behavior_properties;
        let index = values.binary_search_by_key(&key, |(id, _)| *id).ok()?;
        Some(values[index].1)
    }

    fn tank(&self, recipient: Entity) -> Option<Entity> {
        let tank = if let Ok(site) = self.sites.get(recipient) {
            site.tank
        } else if let Ok((member, containment)) = self.residents.get(recipient) {
            if !containment.is_contained {
                return None;
            }
            member.habitat_entity
        } else {
            recipient
        };
        self.tanks.contains(tank).then_some(tank)
    }

    fn write_water(
        &mut self,
        recipient: Entity,
        operation: BehaviorFactModificationOperation,
        value_q16: i32,
    ) {
        let tank = self.tank(recipient).expect("validated tank recipient");
        let (geometry, mut quality) = self.tanks.get_mut(tank).expect("validated tank");
        let next = match operation {
            BehaviorFactModificationOperation::Clear => 1000,
            BehaviorFactModificationOperation::Add => {
                let dirt = if geometry.volume > 0.0 {
                    (value_q16 as f32 / 65_536.0 / geometry.volume * 1000.0)
                        .round()
                        .clamp(0.0, 1000.0) as u16
                } else {
                    0
                };
                quality.0.min(1000).saturating_sub(dirt)
            }
        };
        quality.set_if_neq(WaterQuality(next));
    }
}

pub(super) fn apply_supported_current_behavior_fact_modifications(
    activation: On<WhenBehaviorAction<RunBehaviorFactModifications>>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Single<&ZooClock>,
    mut tasks: Query<
        (
            Entity,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorFactModifications,
            ),
            Option<&mut BehaviorRandomStream>,
        ),
        Or<(With<Animal>, With<Staff>)>,
    >,
    mut modification_messages: BehaviorFactModificationMessageWriters,
    mut food_containers: Query<(Entity, &mut FoodContainer)>,
    mut drink_containers: Query<(Entity, &mut DrinkContainer)>,
    animals: Query<(), With<Animal>>,
    pregnancies: Query<(), With<Pregnancy>>,
    mates: Query<&AnimalMate>,
    mut caffeine_flags: Query<&mut CaffeineFlags>,
    mut facts: ParamSet<(
        MaintenanceFacts,
        consumable_compatibility::AnimalConsumableCompatibility,
    )>,
    price_effect: openzt2_zoo_state::economy::price_effect::BehaviorPriceEffectContext,
    // A rampaging animal's attack scares its guest off (`AttackGuest_Rampage`).
    mut guests: Query<&mut GuestDeparturePoints>,
    mut commands: Commands,
) {
    for (entity, (task, instruction, running_action), mut random) in
        std::iter::once(tasks.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let modifications = &running_action.0;
        if !modifications.iter().all(|modification| {
            let Some(affected_entity) = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                entity,
                task.target,
            ) else {
                return false;
            };
            let scalar_supported = price_effect.sample(modification.modification_value, task.target, |_| 0).is_some();
            if let Some(key) = authored_float_key(modification.modified_fact) {
                return scalar_supported && (if modification.modified_fact == BehaviorFact::Breath {
                    animals.contains(affected_entity)
                } else {
                    facts.p0().initial_object_fact(affected_entity, key).is_some()
                });
            }
            match modification.modified_fact {
                BehaviorFact::WaterDirtiness => return scalar_supported && facts.p0().tank(affected_entity).is_some(),
                BehaviorFact::FilterDirtiness => return scalar_supported && facts.p0().filters.contains(affected_entity),
                BehaviorFact::TrashLevel => return facts.p0().trash.contains(affected_entity)
                    && (scalar_supported || modification.modification_operation == BehaviorFactModificationOperation::Clear),
                _ => {}
            }
            if animals.contains(entity) && modification.modified_fact == BehaviorFact::FoodLevel {
                use openzt2_game_data::world_definitions::world_objects::WorldObjectContainerContent;
                let source = food_containers.get(affected_entity).ok().map(|(_, food)| {
                    (food.food, WorldObjectContainerContent::Food)
                }).or_else(|| drink_containers.get(affected_entity).ok().map(|(_, drink)| {
                    (drink.drink, WorldObjectContainerContent::Drink)
                }));
                let Some((source, content)) = source else {
                    return false;
                };
                if !facts.p1().accepts(entity, affected_entity, source, content, &task, &assets) {
                    return false;
                }
            }
            if guests.contains(affected_entity) {
                return matches!(
                    modification.modified_fact,
                    BehaviorFact::DeparturePoints | BehaviorFact::Happiness | BehaviorFact::Attacked
                ) && !matches!(
                    modification.modification_value,
                    openzt2_game_data::behavior::scalar::BehaviorScalarQ16::PriceEffectQ16(_)
                );
            }
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
        let mut new_caffeine_flags = bevy::ecs::entity::EntityHashMap::<CaffeineFlags>::default();
        let mut new_authored_floats =
            bevy::ecs::entity::EntityHashMap::<AuthoredFloatAttributes>::default();
        let mut maintenance = facts.p0();
        for modification in modifications {
            let animal = resolve_behavior_entity_role_to_live_entity(
                modification.affected_entity_role,
                entity,
                task.target,
            )
            .expect("modification roles were checked before application");
            let value_q16 = if modification.modified_fact == BehaviorFact::TrashLevel
                && modification.modification_operation == BehaviorFactModificationOperation::Clear
            {
                0
            } else {
                price_effect
                    .sample(modification.modification_value, task.target, |count| {
                        random
                            .as_deref_mut()
                            .and_then(|random| random.range_u32(count))
                            .unwrap_or(0)
                    })
                    .expect("scalar context was validated before application")
            };
            if let Some(key) = authored_float_key(modification.modified_fact) {
                let initial = maintenance
                    .initial_object_fact(animal, key)
                    .unwrap_or_default();
                let updated = |current: f32| {
                    if modification.modification_operation
                        == BehaviorFactModificationOperation::Clear
                    {
                        0.0
                    } else {
                        (current + value_q16 as f32 / 65_536.0).clamp(0.0, 100.0)
                    }
                };
                if let Ok(mut values) = maintenance.object_facts.get_mut(animal) {
                    let current = values.get(key).unwrap_or(initial);
                    let next = updated(current);
                    if current != next {
                        values.set(key, next);
                    }
                } else {
                    let values = new_authored_floats.entry(animal).or_default();
                    let current = values.get(key).unwrap_or(initial);
                    values.set(key, updated(current));
                }
                continue;
            }
            // Authored needs measure deprivation on 0..100; welfare messages
            // adjust wellness on 0..1000, as in species need initialization.
            let wellness_delta_q16 = value_q16.saturating_mul(-10);
            debug!(?animal, fact = ?modification.modified_fact, value_q16,
                "behavior fact modification applied");
            match modification.modified_fact {
                BehaviorFact::Bathroom => {
                    modification_messages.bathroom.write(AdjustBathroom {
                        animal,
                        delta_q16: wellness_delta_q16,
                    });
                }
                BehaviorFact::WaterDirtiness => {
                    maintenance.write_water(animal, modification.modification_operation, value_q16);
                }
                BehaviorFact::FilterDirtiness => {
                    let mut filter = maintenance
                        .filters
                        .get_mut(animal)
                        .expect("validated water filter");
                    filter.dirt_percent = match modification.modification_operation {
                        BehaviorFactModificationOperation::Clear => 0.0,
                        BehaviorFactModificationOperation::Add => {
                            (filter.dirt_percent + value_q16 as f32 / 65_536.0).clamp(0.0, 100.0)
                        }
                    };
                }
                BehaviorFact::TrashLevel => {
                    maintenance
                        .trash_changes
                        .write(AdjustFacilityContainedWaste {
                            facility: animal,
                            operation: modification.modification_operation,
                            delta_q16: value_q16,
                        });
                }
                BehaviorFact::Reproduction => {
                    modification_messages
                        .reproduction
                        .write(AdjustReproduction {
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
                BehaviorFact::Pregnancy => {
                    if modification.modification_operation
                        == BehaviorFactModificationOperation::Clear
                        || value_q16 == 0
                    {
                        commands
                            .entity(animal)
                            .remove::<(Pregnancy, PregnancyFather)>();
                    } else if !pregnancies.contains(animal) {
                        let mut animal = commands.entity(animal);
                        animal.insert(Pregnancy { father: None });
                        if let Ok(mate) = mates.get(animal.id()) {
                            animal.insert(PregnancyFather(mate.0));
                        }
                    }
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
                        debug!(
                            ?animal,
                            ?target,
                            amount_q16 = container.amount_q16,
                            capacity_q16 = container.capacity_q16,
                            "behavior food level written"
                        );
                    } else if let Ok((_, mut container)) = drink_containers.get_mut(target) {
                        container.amount_q16 = apply_authored_container_quantity_write(
                            container.amount_q16,
                            container.capacity_q16,
                            value_q16,
                        );
                        debug!(
                            ?animal,
                            ?target,
                            amount_q16 = container.amount_q16,
                            capacity_q16 = container.capacity_q16,
                            "behavior food level written"
                        );
                    } else {
                        unreachable!("FoodLevel target was validated before application");
                    }
                }
                BehaviorFact::CaffeineMode | BehaviorFact::DeCaffeineMode => {
                    let active = modification.modification_operation
                        != BehaviorFactModificationOperation::Clear
                        && value_q16 != 0;
                    let flag: fn(&mut CaffeineFlags) -> &mut bool =
                        if modification.modified_fact == BehaviorFact::CaffeineMode {
                            |flags| &mut flags.caffeinated
                        } else {
                            |flags| &mut flags.decaffeinated
                        };
                    if let Ok(current) = caffeine_flags.get_mut(animal) {
                        current.map_unchanged(flag).set_if_neq(active);
                    } else {
                        *flag(new_caffeine_flags.entry(animal).or_default()) = active;
                    }
                }
                BehaviorFact::DeparturePoints => {
                    if let Ok(mut departure_points) = guests.get_mut(animal) {
                        departure_points.0 = departure_points.0.saturating_add(value_q16);
                    }
                }
                // Source happiness is deprivation; the satisfaction message is wellness.
                BehaviorFact::Happiness => {
                    modification_messages
                        .guest_satisfaction
                        .write(AdjustGuestSatisfaction {
                            guest: animal,
                            delta_q16: value_q16.saturating_neg(),
                        });
                }
                BehaviorFact::Attacked => {
                    if modification.modification_operation
                        == BehaviorFactModificationOperation::Clear
                        || value_q16 == 0
                    {
                        commands.entity(animal).remove::<GuestAttacked>();
                    } else {
                        commands.entity(animal).insert(GuestAttacked);
                    }
                }
                BehaviorFact::Escaped | BehaviorFact::Rampage => {
                    let active = modification.modification_operation
                        != BehaviorFactModificationOperation::Clear
                        && value_q16 != 0;
                    let mut affected = commands.entity(animal);
                    match (modification.modified_fact, active) {
                        (BehaviorFact::Escaped, true) => {
                            affected.insert_if_new(Escaped {
                                since_tick: clock.tick,
                            });
                        }
                        (BehaviorFact::Escaped, false) => {
                            affected.remove::<Escaped>();
                        }
                        (BehaviorFact::Rampage, true) => {
                            affected.insert_if_new(Rampaging { elapsed_ticks: 0 });
                        }
                        (BehaviorFact::Rampage, false) => {
                            affected.remove::<Rampaging>();
                        }
                        _ => unreachable!(),
                    }
                }
                _ => {
                    unreachable!("unsupported fact modifications were rejected before application")
                }
            };
        }
        for (animal, flags) in new_caffeine_flags {
            if flags != CaffeineFlags::default() {
                commands.entity(animal).try_insert(flags);
            }
        }
        for (entity, values) in new_authored_floats {
            commands.entity(entity).try_insert(values);
        }
        instruction.complete(entity, clock.tick, &mut commands);
    }
}

fn authored_float_key(fact: BehaviorFact) -> Option<openzt2_game_data::AssetId> {
    let name = match fact {
        BehaviorFact::Breath => "breath",
        BehaviorFact::Construction => "f_constructionlevel",
        BehaviorFact::Damage => "f_damage",
        _ => return None,
    };
    Some(openzt2_game_data::AssetId::from_key(name))
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
    if matches!(
        modification.modified_fact,
        BehaviorFact::CaffeineMode | BehaviorFact::DeCaffeineMode
    ) {
        return true;
    }
    if !animals.contains(affected_entity) {
        return false;
    }
    matches!(
        modification.modified_fact,
        BehaviorFact::Hunger
            | BehaviorFact::Bathroom
            | BehaviorFact::Reproduction
            | BehaviorFact::Pregnancy
            | BehaviorFact::Thirst
            | BehaviorFact::Rest
            | BehaviorFact::Privacy
            | BehaviorFact::Social
            | BehaviorFact::Exercise
            | BehaviorFact::Stimulation
            | BehaviorFact::Health
            | BehaviorFact::Hygiene
            | BehaviorFact::Frozen
            | BehaviorFact::Escaped
            | BehaviorFact::Rampage
    )
}

#[cfg(test)]
mod tests;
