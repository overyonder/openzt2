use bevy::prelude::*;
use openzt2_game_data::behavior::action::feedback::BehaviorFeedbackEntry;
use openzt2_game_data::behavior::action_record::BehaviorAction;

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::behavior_task_execution_types::advance_behavior_task_to_next_action;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackDispatched;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackLine;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackLog;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorFeedbackName;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

pub(super) fn dispatch_current_authored_behavior_feedback_and_advance(
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    mut behavior_tasks: Query<(Entity, &mut BehaviorTaskExecutionState)>,
    mut dispatched_feedback: MessageWriter<BehaviorFeedbackDispatched>,
) {
    for (actor, mut behavior_task) in &mut behavior_tasks {
        if behavior_task.next_action_tick > simulation_clock.tick {
            continue;
        }
        let Some(BehaviorAction::Feedback(feedback)) = behavior_document_assets
            .get(&behavior_task.document)
            .and_then(|document| find_current_behavior_task_action(document, &behavior_task))
        else {
            continue;
        };
        let dispatch = BehaviorFeedbackDispatched {
            actor,
            document: behavior_task.document.clone(),
            declaration: behavior_task.declaration,
            phase: behavior_task.phase,
            action: behavior_task.action,
        };
        debug!(
            actor = ?dispatch.actor,
            document = ?dispatch.document.id(),
            declaration = dispatch.declaration,
            phase = ?dispatch.phase,
            action = dispatch.action,
            entries = feedback.entries.len(),
            "dispatched authored behavior feedback"
        );
        dispatched_feedback.write(dispatch);
        advance_behavior_task_to_next_action(&mut behavior_task, simulation_clock.tick);
    }
}

/// Keeps each actor's latest authored action and its recent thoughts for its
/// information panel. The action stays until the next one: an idle animal
/// starts a new task about every second, so clearing it between tasks made
/// the panel blink.
pub(super) fn record_dispatched_behavior_feedback(
    mut commands: Commands,
    mut dispatched: MessageReader<BehaviorFeedbackDispatched>,
    behavior_document_assets: Res<Assets<BehaviorDocumentAsset>>,
    simulation_clock: Res<ZooClock>,
    fixed_time: Res<Time<Fixed>>,
    tasks: Query<&BehaviorTaskExecutionState>,
    mut logs: Query<&mut BehaviorFeedbackLog>,
    occupancy: Res<super::interaction_container_occupancy::InteractionContainerOccupancy>,
) {
    let ticks_per_second = 1.0 / fixed_time.timestep().as_secs_f64().max(1e-6);
    for dispatch in dispatched.read() {
        let Some(BehaviorAction::Feedback(feedback)) = behavior_document_assets
            .get(&dispatch.document)
            .and_then(|document| feedback_action(document, dispatch))
        else {
            continue;
        };
        let task = tasks.get(dispatch.actor).ok();
        let target = task.and_then(BehaviorTaskExecutionState::target);
        let target_task = target.and_then(|target| tasks.get(target).ok());
        let related = FeedbackRelated {
            actor: dispatch.actor,
            target,
            object: task.and_then(|task| task.object),
            // The food in a dish, say, rather than the dish.
            target_contents: target
                .and_then(|target| occupancy.members_of_container(target).next()),
            target_target: target_task.and_then(BehaviorTaskExecutionState::target),
            target_object: target_task.and_then(|task| task.object),
        };
        let named = |flags: FeedbackNameFlags| feedback_name(flags, &related);
        let mut log = logs.get(dispatch.actor).cloned().unwrap_or_default();
        for entry in &feedback.entries {
            match entry {
                BehaviorFeedbackEntry::Action {
                    localization_key,
                    use_entity_name,
                    use_target_name,
                    use_target_contents,
                    use_object_name,
                } => {
                    log.action = Some(BehaviorFeedbackLine {
                        localization_key: localization_key.clone(),
                        named: named(FeedbackNameFlags {
                            entity: *use_entity_name,
                            target: *use_target_name,
                            target_contents: *use_target_contents,
                            object: *use_object_name,
                            ..FeedbackNameFlags::default()
                        }),
                    });
                }
                BehaviorFeedbackEntry::Thought {
                    localization_key,
                    timeout_seconds,
                    use_entity_name,
                    use_target_name,
                    use_target_target,
                    use_target_object,
                    use_target_species,
                    ..
                } => {
                    let timeout_ticks =
                        (f64::from(timeout_seconds.unwrap_or(60)) * ticks_per_second).ceil() as u64;
                    log.think(
                        BehaviorFeedbackLine {
                            localization_key: localization_key.clone(),
                            named: named(FeedbackNameFlags {
                                entity: *use_entity_name,
                                target: *use_target_name,
                                target_target: *use_target_target,
                                target_object: *use_target_object,
                                target_species: *use_target_species,
                                ..FeedbackNameFlags::default()
                            }),
                        },
                        simulation_clock.tick,
                        simulation_clock.tick.saturating_add(timeout_ticks),
                    );
                }
                BehaviorFeedbackEntry::Message { .. } | BehaviorFeedbackEntry::Emoticon { .. } => {}
            }
        }
        if let Ok(mut current) = logs.get_mut(dispatch.actor) {
            if *current != log {
                *current = log;
            }
        } else if let Ok(mut actor) = commands.get_entity(dispatch.actor) {
            actor.insert(log);
        }
    }
}

/// The entities a feedback text may name, around the actor's task.
struct FeedbackRelated {
    actor: Entity,
    target: Option<Entity>,
    object: Option<Entity>,
    target_contents: Option<Entity>,
    target_target: Option<Entity>,
    target_object: Option<Entity>,
}

/// What the flags ask a feedback text to name, the most specific first.
fn feedback_name(
    flags: FeedbackNameFlags,
    related: &FeedbackRelated,
) -> Option<BehaviorFeedbackName> {
    use BehaviorFeedbackName::{Entity as Named, Species};
    if flags.target_contents {
        related.target_contents.map(Named)
    } else if flags.target_target {
        related.target_target.map(Named)
    } else if flags.target_object {
        related.target_object.map(Named)
    } else if flags.target_species {
        related.target.map(Species)
    } else if flags.target {
        related.target.map(Named)
    } else if flags.object {
        related.object.map(Named)
    } else if flags.entity {
        Some(Named(related.actor))
    } else {
        None
    }
}

/// The authored `use*` flags saying what a feedback text names.
#[derive(Clone, Copy, Default)]
struct FeedbackNameFlags {
    entity: bool,
    target: bool,
    /// The target container's contents, such as the food in a dish.
    target_contents: bool,
    /// The target's own task target, such as the dish an animal eats from.
    target_target: bool,
    /// The target's own task object.
    target_object: bool,
    target_species: bool,
    object: bool,
}

fn feedback_action<'a>(
    document: &'a BehaviorDocumentAsset,
    dispatch: &BehaviorFeedbackDispatched,
) -> Option<&'a BehaviorAction> {
    use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
    let phase = match dispatch.phase {
        BehaviorTaskExecutionPhase::Set => {
            &document
                .behavior_set_at_index(dispatch.declaration)?
                .actions
        }
        BehaviorTaskExecutionPhase::Execution => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .execution
        }
        BehaviorTaskExecutionPhase::Completion => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .completion
        }
        BehaviorTaskExecutionPhase::Failure => {
            &document
                .behavior_task_at_index(dispatch.declaration)?
                .failure
        }
    };
    phase.lowered_actions().get(dispatch.action)
}

#[cfg(test)]
mod feedback_name_tests {
    use super::*;

    #[test]
    fn a_text_names_what_its_flags_ask_for() {
        let [actor, target, contents, dish] =
            [1, 2, 3, 4].map(|index| Entity::from_raw_u32(index).unwrap());
        let related = FeedbackRelated {
            actor,
            target: Some(target),
            object: None,
            target_contents: Some(contents),
            target_target: Some(dish),
            target_object: None,
        };
        let name = |flags| feedback_name(flags, &related);
        let flags = FeedbackNameFlags::default;
        assert_eq!(
            name(FeedbackNameFlags {
                target_contents: true,
                target: true,
                ..flags()
            }),
            Some(BehaviorFeedbackName::Entity(contents))
        );
        assert_eq!(
            name(FeedbackNameFlags {
                target_target: true,
                ..flags()
            }),
            Some(BehaviorFeedbackName::Entity(dish))
        );
        assert_eq!(
            name(FeedbackNameFlags {
                target_species: true,
                ..flags()
            }),
            Some(BehaviorFeedbackName::Species(target))
        );
        assert_eq!(
            name(FeedbackNameFlags {
                entity: true,
                ..flags()
            }),
            Some(BehaviorFeedbackName::Entity(actor))
        );
        assert_eq!(name(flags()), None);
    }
}
