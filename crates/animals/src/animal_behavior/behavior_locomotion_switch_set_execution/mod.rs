//! Selection of the behavior set authored for the actor's locomotion medium
//! or current animation graph node.

use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_game_data::{behavior::action_record::BehaviorAction, AssetId};

use openzt2_game_assets::{
    animation::animation_set_asset_types::AnimationSetAsset,
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::behavior::action::locomotion_switch_set::BehaviorLocomotionSwitchSetAction;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::behavior_task_execution_types::find_current_behavior_task_action;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskReturnFrame;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorAnimationClipCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorDockingCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorMoveCompletion;
use openzt2_zoo_state::behavior_task_execution_types::PendingBehaviorTaskFailure;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_zoo_state::locomotion::locomotion_types::{
    Destination, Docking, LocomotionMode, SwimLayer,
};
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;

/// A medium-following set the actor is playing. When the actor's locomotion
/// medium changes, the switch runs again and plays the new medium's set.
#[derive(Component, Debug, Clone)]
pub(super) struct MediumFollowingBehaviorSet {
    execution_id: u64,
    origin: BehaviorTaskReturnFrame,
    switch: BehaviorTaskReturnFrame,
    depth: usize,
    medium: LocomotionMedium,
}

/// The behavior-table row that a subject's current locomotion selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocomotionMedium {
    Ground,
    Water,
    Underwater,
}

impl LocomotionMedium {
    /// Actors without a locomotion mode of their own, such as staff, walk.
    fn of(mode: Option<&LocomotionMode>, layer: Option<&SwimLayer>) -> Self {
        match (mode, layer) {
            (Some(LocomotionMode::Swim), Some(SwimLayer::Submerged)) => Self::Underwater,
            (Some(LocomotionMode::Swim), _) => Self::Water,
            _ => Self::Ground,
        }
    }

    /// The row for this medium, or the ground row when that one is absent.
    fn select(self, sets: &BehaviorLocomotionSwitchSetAction) -> Option<AssetId> {
        match self {
            Self::Ground => None,
            Self::Water => sets.water,
            Self::Underwater => sets.underwater,
        }
        .or(sets.ground)
    }
}

#[derive(SystemParam)]
pub(super) struct BehaviorSetLookup<'w> {
    documents: Res<'w, Assets<BehaviorDocumentAsset>>,
    loaded: Res<'w, LoadedBehaviorDocumentCollection>,
    definitions: Res<'w, WorldDefinitions>,
    definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
}

/// Enters `program` as a nested set after the current action.
fn enter_selected_behavior_set(
    task: &mut BehaviorTaskExecutionState,
    program: AssetId,
    lookup: &BehaviorSetLookup,
    animal_types: &[AssetId],
    subject: (
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
    ),
    tick: u64,
) -> bool {
    let (Some(view), Some(catalogue)) = (
        lookup
            .loaded
            .create_declaration_index_view(&lookup.documents),
        lookup.definitions.get(&lookup.definition_assets),
    ) else {
        return false;
    };
    let Some((document, declaration)) = view.find_behavior_set_location(
        program,
        super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
            subject.0,
            subject.1,
            subject.2,
            catalogue,
            animal_types,
        ),
    ) else {
        return false;
    };
    let frame = task.create_return_frame_after_action(task.action.saturating_add(1));
    if !task.push_return_frame(frame) {
        return false;
    }
    task.program = program;
    task.document = document;
    task.declaration = declaration;
    task.phase = BehaviorTaskExecutionPhase::Set;
    task.action = 0;
    task.repetitions = 0;
    task.next_action_tick = tick.saturating_add(1);
    true
}

/// Plays the set authored for the subject's locomotion medium, falling back
/// to its ground set.
pub(super) fn play_locomotion_medium_behavior_sets(
    mut commands: Commands,
    lookup: BehaviorSetLookup,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    clock: Res<ZooClock>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&LocomotionMode>,
            Option<&SwimLayer>,
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, mut task, mode, layer, species, staff, guest) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::LocomotionSwitchSet(sets)) = lookup
            .documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
            .cloned()
        else {
            continue;
        };
        let medium = LocomotionMedium::of(mode, layer);
        let Some(program) = medium.select(&sets) else {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        };
        let switch = task.create_return_frame_after_action(task.action);
        let origin = task.create_return_frame_after_action(task.action.saturating_add(1));
        let depth = task.stack.len();
        if !enter_selected_behavior_set(
            &mut task,
            program,
            &lookup,
            animal_types.borrow_for(actor),
            (species, staff, guest),
            clock.tick,
        ) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
            continue;
        }
        if sets.follows_medium_changes {
            commands.entity(actor).insert(MediumFollowingBehaviorSet {
                execution_id: task.execution_id,
                origin,
                switch,
                depth,
                medium,
            });
        } else {
            commands
                .entity(actor)
                .remove::<MediumFollowingBehaviorSet>();
        }
    }
}

/// A medium-following set ends with its frame. While it plays, a change of
/// locomotion medium abandons the playing set's actions and runs the switch
/// again, which selects the set for the new medium.
pub(super) fn switch_medium_following_sets_after_medium_changes(
    mut commands: Commands,
    clock: Res<ZooClock>,
    mut actors: Query<(
        Entity,
        &MediumFollowingBehaviorSet,
        Option<&mut BehaviorTaskExecutionState>,
        Option<&LocomotionMode>,
        Option<&SwimLayer>,
        Has<PendingBehaviorTaskFailure>,
    )>,
) {
    for (actor, following, task, mode, layer, failing) in &mut actors {
        let Some(mut task) = task.filter(|task| {
            !failing
                && task.execution_id == following.execution_id
                && task.stack.len() > following.depth
                && task.is_at_or_nested_under(&following.origin, following.depth)
        }) else {
            commands
                .entity(actor)
                .remove::<MediumFollowingBehaviorSet>();
            continue;
        };
        if LocomotionMedium::of(mode, layer) == following.medium {
            continue;
        }
        task.restore_suspended_instruction(&following.switch, following.depth, clock.tick);
        commands.entity(actor).remove::<(
            MediumFollowingBehaviorSet,
            Destination,
            Docking,
            PendingBehaviorAnimationClipCompletion,
            PendingBehaviorDockingCompletion,
            PendingBehaviorMoveCompletion,
        )>();
    }
}

/// Plays the set authored for the graph node the actor's animation is in,
/// or the `default` row for any other node.
pub(super) fn play_animation_node_behavior_sets(
    mut commands: Commands,
    lookup: BehaviorSetLookup,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    clock: Res<ZooClock>,
    animation_sets: Res<Assets<AnimationSetAsset>>,
    controllers: Query<(
        &AnimationPresentationOwner,
        &AnimationPlaybackController,
        &AnimationGraphPlaybackState,
    )>,
    mut actors: Query<
        (
            Entity,
            &mut BehaviorTaskExecutionState,
            Option<&SpeciesHandle>,
            Option<&StaffRole>,
            Option<&GuestArchetype>,
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
) {
    for (actor, mut task, species, staff, guest) in &mut actors {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::AnimationSwitchSet(sets)) = lookup
            .documents
            .get(&task.document)
            .and_then(|document| find_current_behavior_task_action(document, &task))
            .cloned()
        else {
            continue;
        };
        let current_node = controllers
            .iter()
            .find(|(owner, _, _)| owner.gameplay_entity == actor)
            .and_then(|(_, controller, state)| {
                animation_sets
                    .get(&controller.animation_set_asset)?
                    .animation_graph_nodes
                    .get(usize::try_from(state.current_animation_graph_node_index).ok()?)
                    .map(|node| node.animation_graph_node_asset_key.clone())
            });
        let program = current_node
            .and_then(|node| {
                sets.sets_by_animation_graph_node
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&node))
                    .map(|(_, set)| *set)
            })
            .or(sets.default_set);
        if !program.is_some_and(|program| {
            enter_selected_behavior_set(
                &mut task,
                program,
                &lookup,
                animal_types.borrow_for(actor),
                (species, staff, guest),
                clock.tick,
            )
        }) {
            mark_behavior_task_for_failure_and_stop_navigation(actor, &mut commands);
        }
    }
}
