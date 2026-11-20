//! Emits authored view events and delivers them to the guests watching.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::{RunBehaviorViewAnimals, RunBehaviorViewEvent, RunningBehaviorInstruction};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use bevy::prelude::*;
use openzt2_game_data::{
        world_definitions::guest_simulation_definitions::GuestMemoryKind, AssetId,
};

use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_zoo_state::{
    animal_behavior::behavior_set_start_request_types::PendingBehaviorSet,
    economy::service_types::ServiceReservation,
    guests::{
        guest_simulation_types::{
            Guest, GuestArchetype, GuestDestination, GuestEducation, GuestMemories,
            GuestMemoryEntry, GuestNavigationRequest, GuestPhase, GuestReaction, GuestSatisfaction,
            GuestViewers, Viewing,
        },
        guest_survey_types::GuestSurveyExperience,
    },
    locomotion::locomotion_types::Destination,
};

use super::{
    guest_definition_queries::{find_guest_definition, find_reaction},
    guest_simulation_calculations::{apply_permille_delta, memory_value, push_memory},
    guest_viewing_lifecycle::viewing_owned_by_current_view_action,
};

/// References immutable authored data rather than copying feedback or view rows.
#[derive(Message)]
pub(crate) struct BehaviorViewEventEmitted {
    pub(crate) source: Entity,
    pub(crate) source_definition: AssetId,
    pub(crate) view_key: AssetId,
}

pub(super) fn emit_authored_view_events(
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    mut tasks: Query<
        (
            Entity,
            Option<&DefinitionId>,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorViewEvent,
            ),
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    mut events: MessageWriter<BehaviorViewEventEmitted>,
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (source, definition, (task, instruction, running_action)) in &mut tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let event = &running_action.0;
        let Some(definition) = definition.and_then(|id| definitions.find_object(id.0)) else {
            mark_behavior_task_for_failure_and_stop_navigation(source, &mut commands);
            continue;
        };
        if !definition
            .view_data
            .iter()
            .any(|row| row.name == event.view_key)
        {
            mark_behavior_task_for_failure_and_stop_navigation(source, &mut commands);
            continue;
        }
        events.write(BehaviorViewEventEmitted {
            source,
            source_definition: definition.id,
            view_key: event.view_key,
        });
        instruction.complete(source, clock.tick, &mut commands);
    }
}

#[derive(Message)]
pub(crate) struct AppliedAnimalView {
    pub(crate) guest: Entity,
    pub(crate) subject: Entity,
    pub(crate) viewing: Viewing,
    pub(crate) view_score: f32,
    pub(crate) entertainment_score: f32,
    pub(crate) donation_score: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn apply_guest_reactions(
    mut events: MessageReader<BehaviorViewEventEmitted>,
    mut reactions: MessageReader<GuestReaction>,
    mut applied_views: MessageWriter<AppliedAnimalView>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    viewers: Query<&GuestViewers>,
    members: Query<(&WorldMember, Option<&DefinitionId>)>,
    observations: Query<
        (
            &Viewing,
            &GuestPhase,
            Option<&GuestDestination>,
            Option<&GuestNavigationRequest>,
        ),
        With<Guest>,
    >,
    tasks: Query<
        (&BehaviorTaskExecutionState, &RunningBehaviorInstruction),
        With<RunBehaviorViewAnimals>,
    >,
    interruptions: Query<
        (),
        Or<(
            With<BehaviorTaskExecutionState>,
            With<PendingBehaviorSet>,
            With<ServiceReservation>,
            With<Destination>,
        )>,
    >,
    mut guests: Query<
        (
            &GuestArchetype,
            &mut GuestSatisfaction,
            &mut GuestEducation,
            &mut GuestMemories,
            &mut GuestSurveyExperience,
        ),
        (With<Guest>, With<GuestPhase>),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        reactions.clear();
        events.clear();
        return;
    };
    let mut apply = |reaction: GuestReaction| {
        let Ok((archetype, mut satisfaction, mut education, mut memories, mut experience)) =
            guests.get_mut(reaction.guest)
        else {
            return false;
        };
        let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
            return false;
        };
        let (wellness_delta, education_delta) = if reaction.kind == GuestMemoryKind::AnimalView {
            find_reaction(definition, GuestMemoryKind::AnimalView).map_or((0, 0), |policy| {
                (policy.satisfaction_delta, policy.education_delta)
            })
        } else {
            (
                reaction.satisfaction_delta_permille,
                reaction.education_delta_permille,
            )
        };
        satisfaction.add_wellness_q16(i32::from(wellness_delta) * (1 << 16));
        education.0 = apply_permille_delta(education.0, education_delta);
        experience.education_points += f32::from(education_delta) / 10.0;
        push_memory(
            &mut memories,
            GuestMemoryEntry {
                subject: reaction.subject,
                kind: reaction.kind,
                value_permille: memory_value(reaction.kind, wellness_delta, education_delta),
                age_ticks: 0,
            },
            definition.memory.replacement,
        );
        true
    };

    for reaction in reactions
        .read()
        .filter(|reaction| reaction.kind != GuestMemoryKind::AnimalView)
    {
        apply(*reaction);
    }
    for (event, _) in events.read_with_id() {
        let Ok(watching) = viewers.get(event.source) else {
            continue;
        };
        let Ok((subject_world, Some(subject_definition))) = members.get(event.source) else {
            continue;
        };
        if subject_definition.0 != event.source_definition {
            continue;
        }
        let Some(row) = definitions
            .find_object(event.source_definition)
            .and_then(|object| {
                object
                    .view_data
                    .iter()
                    .find(|row| row.name == event.view_key)
            })
        else {
            continue;
        };
        for guest in watching.iter() {
            let Ok((viewing, phase, destination, request)) = observations.get(guest) else {
                continue;
            };
            if *phase != GuestPhase::Visiting || viewing.subject != event.source {
                continue;
            }
            let Some(area) = viewing.area else {
                continue;
            };
            if !members
                .get(guest)
                .is_ok_and(|(world, _)| world.root == subject_world.root)
                || !members
                    .get(area)
                    .is_ok_and(|(world, _)| world.root == subject_world.root)
            {
                continue;
            }
            let watching_from_destination = viewing.task_execution_id.is_none()
                && !interruptions.contains(guest)
                && destination.is_some_and(|destination| {
                    destination.purpose == GuestVisitPurpose::View && destination.entity == area
                })
                && request.is_none_or(|request| request.0.is_none());
            let watching_through_task = tasks.get(guest).is_ok_and(|(task, instruction)| {
                viewing_owned_by_current_view_action(task, viewing, instruction)
            });
            if !(watching_from_destination || watching_through_task) {
                continue;
            }
            if apply(GuestReaction {
                guest,
                subject: event.source,
                kind: GuestMemoryKind::AnimalView,
                satisfaction_delta_permille: 0,
                education_delta_permille: 0,
            }) {
                applied_views.write(AppliedAnimalView {
                    guest,
                    subject: event.source,
                    viewing: *viewing,
                    view_score: row.view_score,
                    entertainment_score: row.entertainment_score,
                    donation_score: row.donation_score,
                });
            }
        }
    }
}
