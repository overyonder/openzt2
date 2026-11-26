use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::{
    animal_health::types::Dead,
    animal_lifecycle::types::{Animal, SpeciesHandle},
    shows::{
        performer_training::PerformerTraining,
        show_schedule_types::{
            ScheduledShowPerformancePlan, ScheduledShowRow, ScheduledShowTrickPerformance,
            ScheduledTrickPerformer, ShowTrickSlotOf, ShowTrickSlots,
        },
        show_stage_types::ShowStage,
    },
};

use super::{
    scheduled_trick_execution::performance_level,
    show_editor_interaction_types::{EditedShow, ShowEditing},
    show_timing_operations::convert_authored_nanoseconds_to_fixed_simulation_ticks,
};

#[derive(Message, Clone, Copy)]
pub struct EditShowMixerSlot {
    pub controller: Entity,
    pub slot: Option<Entity>,
    pub time_slot: Option<u16>,
    pub choice: ShowMixerSlotChoice,
}

#[derive(Clone, Copy)]
pub enum ShowMixerSlotChoice {
    Trick { animal: Entity, trick: AssetId },
    Wait { animal: Entity, ticks: u32 },
    Remove,
    RemovePerformer { animal: Entity },
    ReplacePerformer { previous: Entity, animal: Entity },
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ShowMixerWait(pub u32);

/// Zero-based time slot within a performance. Each animal may act once per slot.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub struct ShowMixerTimeSlot(pub u16);

enum SlotEdit {
    Trick {
        animal: Entity,
        performance: ScheduledShowTrickPerformance,
    },
    Wait {
        animal: Entity,
        wait: ShowMixerWait,
    },
    Remove,
}

pub(super) fn apply_show_mixer_slot_edits(
    mut edits: MessageReader<EditShowMixerSlot>,
    controllers: Query<&EditedShow, With<ShowEditing>>,
    rows: Query<(
        &ScheduledShowRow,
        &ScheduledShowPerformancePlan,
        &WorldMember,
        Option<&ShowTrickSlots>,
    )>,
    stages: Query<&ShowStage>,
    mut slots: Query<(
        &ShowTrickSlotOf,
        Option<&ScheduledTrickPerformer>,
        Option<&mut ScheduledShowTrickPerformance>,
        Option<&mut ShowMixerWait>,
        Option<&ShowMixerTimeSlot>,
    )>,
    performers: Query<
        (&WorldMember, &SpeciesHandle, Option<&PerformerTraining>),
        (With<Animal>, Without<Dead>),
    >,
    definitions: Res<WorldDefinitions>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
) {
    let Some(definitions) = definitions.get(&assets) else {
        edits.clear();
        return;
    };
    let mut pending =
        bevy::ecs::entity::EntityHashMap::<Vec<(Entity, Option<Entity>, u16)>>::default();
    let mut changes = std::collections::BTreeMap::<Entity, SlotEdit>::new();
    for edit in edits.read() {
        let Ok(edited) = controllers.get(edit.controller) else {
            continue;
        };
        let Ok((row, plan, world, members)) = rows.get(edited.0) else {
            continue;
        };
        let Ok(stage) = stages.get(row.stage) else {
            continue;
        };
        let Some(stage) = definitions.find_show_stage(stage.show_stage_definition) else {
            continue;
        };
        let pending = pending.entry(edited.0).or_insert_with(|| {
            members
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .enumerate()
                .filter_map(|(index, slot)| {
                    let (owner, performer, _, _, time) = slots.get(slot).ok()?;
                    (owner.0 == edited.0).then_some((
                        slot,
                        performer.map(|performer| performer.0),
                        time.map_or(index as u16, |time| time.0),
                    ))
                })
                .collect()
        });
        if let ShowMixerSlotChoice::RemovePerformer { animal } = edit.choice {
            pending.retain(|&(slot, performer, _)| {
                if performer != Some(animal) {
                    return true;
                }
                changes.insert(slot, SlotEdit::Remove);
                false
            });
            continue;
        }
        if let ShowMixerSlotChoice::ReplacePerformer { previous, animal } = edit.choice {
            let Ok((owner, species, training)) = performers.get(animal) else {
                continue;
            };
            if owner.root != world.root
                || pending
                    .iter()
                    .any(|&(_, performer, _)| performer == Some(animal))
            {
                continue;
            }
            let replacements: Option<Vec<_>> = pending
                .iter()
                .filter_map(|&(slot, performer, _)| (performer == Some(previous)).then_some(slot))
                .map(|slot| {
                    let next = match changes.get(&slot) {
                        Some(SlotEdit::Trick { performance, .. }) => SlotEdit::Trick {
                            animal,
                            performance: *performance,
                        },
                        Some(SlotEdit::Wait { wait, .. }) => SlotEdit::Wait {
                            animal,
                            wait: *wait,
                        },
                        Some(SlotEdit::Remove) => return None,
                        None => {
                            let (_, _, performance, wait, _) = slots.get(slot).ok()?;
                            if let Some(performance) = performance {
                                SlotEdit::Trick {
                                    animal,
                                    performance: *performance,
                                }
                            } else {
                                SlotEdit::Wait {
                                    animal,
                                    wait: *wait?,
                                }
                            }
                        }
                    };
                    if let SlotEdit::Trick { performance, .. } = next {
                        let trick = definitions.find_trick(performance.trick)?;
                        if !trick.species.contains(&species.species)
                            || performance_level(trick, training).is_none()
                        {
                            return None;
                        }
                    }
                    Some((slot, next))
                })
                .collect();
            if let Some(replacements) = replacements {
                changes.extend(replacements);
                for (_, performer, _) in pending.iter_mut() {
                    if *performer == Some(previous) {
                        *performer = Some(animal);
                    }
                }
            }
            continue;
        }
        if edit
            .slot
            .is_some_and(|slot| pending.iter().all(|&(candidate, ..)| candidate != slot))
        {
            continue;
        }
        if let (
            Some(slot),
            ShowMixerSlotChoice::Trick { animal, .. } | ShowMixerSlotChoice::Wait { animal, .. },
        ) = (edit.slot, edit.choice)
        {
            if pending.iter().any(|&(candidate, performer, _)| {
                candidate == slot && performer.is_some_and(|current| current != animal)
            }) {
                continue;
            }
        }
        let change = match edit.choice {
            ShowMixerSlotChoice::Trick { animal, trick } => {
                let Ok((member, species, training)) = performers.get(animal) else {
                    continue;
                };
                let Some(definition) = definitions.find_trick(trick) else {
                    continue;
                };
                if member.root != world.root
                    || !definition.species.contains(&species.species)
                    || (!stage.supported_tricks.is_empty()
                        && !stage.supported_tricks.contains(&trick))
                    || performance_level(definition, training).is_none()
                {
                    continue;
                }
                let assigned: bevy::ecs::entity::EntityHashSet = pending
                    .iter()
                    .filter_map(|&(slot, performer, _)| {
                        (Some(slot) != edit.slot).then_some(performer).flatten()
                    })
                    .collect();
                let performer_limit = definitions
                    .show_editor_presentation()
                    .map_or(stage.performer_slots, |editor| {
                        editor.maximum_animals.min(stage.performer_slots)
                    });
                if !assigned.contains(&animal) && assigned.len() >= usize::from(performer_limit) {
                    continue;
                }
                let Some(timing) = definitions.show_scheduling() else {
                    continue;
                };
                let duration_ticks = convert_authored_nanoseconds_to_fixed_simulation_ticks(
                    timing.trick_treat_timeout_ns,
                    definitions,
                )
                .max(1);
                SlotEdit::Trick {
                    animal,
                    performance: ScheduledShowTrickPerformance {
                        trick,
                        duration_ticks,
                    },
                }
            }
            ShowMixerSlotChoice::Wait { animal, ticks } => {
                if !pending
                    .iter()
                    .any(|&(_, performer, _)| performer == Some(animal))
                    || !performers
                        .get(animal)
                        .is_ok_and(|(member, ..)| member.root == world.root)
                {
                    continue;
                }
                SlotEdit::Wait {
                    animal,
                    wait: ShowMixerWait(ticks),
                }
            }
            ShowMixerSlotChoice::Remove => SlotEdit::Remove,
            ShowMixerSlotChoice::RemovePerformer { .. }
            | ShowMixerSlotChoice::ReplacePerformer { .. } => continue,
        };
        let Some(slot) = edit.slot.or_else(|| {
            let animal = match change {
                SlotEdit::Trick { animal, .. } | SlotEdit::Wait { animal, .. } => animal,
                SlotEdit::Remove => return None,
            };
            let time = edit.time_slot.unwrap_or_else(|| {
                pending
                    .iter()
                    .filter_map(|&(_, performer, time)| (performer == Some(animal)).then_some(time))
                    .max()
                    .map_or(0, |time| time.saturating_add(1))
            });
            if time >= plan.trick_performance_capacity
                || pending
                    .iter()
                    .any(|&(_, performer, slot)| performer == Some(animal) && slot == time)
            {
                return None;
            }
            let slot = commands
                .spawn((ShowTrickSlotOf(edited.0), *world, ShowMixerTimeSlot(time)))
                .id();
            pending.push((slot, None, time));
            Some(slot)
        }) else {
            continue;
        };
        match &change {
            SlotEdit::Remove => pending.retain(|&(candidate, ..)| candidate != slot),
            SlotEdit::Trick { animal, .. } | SlotEdit::Wait { animal, .. } => {
                if let Some((_, performer, _)) = pending
                    .iter_mut()
                    .find(|(candidate, ..)| *candidate == slot)
                {
                    *performer = Some(*animal);
                }
            }
        }
        changes.insert(slot, change);
    }
    for (slot, change) in changes {
        match change {
            SlotEdit::Trick {
                animal,
                performance,
            } => {
                if let Ok((_, performer, current, _, _)) = slots.get_mut(slot) {
                    if performer.is_none_or(|performer| performer.0 != animal) {
                        commands
                            .entity(slot)
                            .insert(ScheduledTrickPerformer(animal));
                    }
                    if let Some(mut current) = current {
                        current.set_if_neq(performance);
                    } else {
                        commands
                            .entity(slot)
                            .insert(performance)
                            .remove::<ShowMixerWait>();
                    }
                } else {
                    commands
                        .entity(slot)
                        .insert((performance, ScheduledTrickPerformer(animal)));
                }
            }
            SlotEdit::Wait { animal, wait } => {
                if let Ok((_, performer, _, Some(mut current), _)) = slots.get_mut(slot) {
                    current.set_if_neq(wait);
                    if performer.is_none_or(|performer| performer.0 != animal) {
                        commands
                            .entity(slot)
                            .insert(ScheduledTrickPerformer(animal));
                    }
                } else {
                    commands
                        .entity(slot)
                        .remove::<ScheduledShowTrickPerformance>()
                        .insert((wait, ScheduledTrickPerformer(animal)));
                }
            }
            SlotEdit::Remove => {
                commands.entity(slot).try_despawn();
            }
        }
    }
}
