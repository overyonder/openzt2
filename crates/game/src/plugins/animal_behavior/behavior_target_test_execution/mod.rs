//! Canonical behavior target-test execution and authored branch entry.

use bevy::prelude::*;
use openzt2_game_data::behavior::{
    action::target_test::BehaviorTargetTestKind, action_record::BehaviorAction,
};

use crate::assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use crate::assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_lifecycle::types::SpeciesHandle;
use crate::plugins::guests::guest_simulation_types::GuestArchetype;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::staff::staff_employment_types::StaffRole;
use crate::plugins::topology::topology_graph_types::FenceEdge;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use crate::plugins::behavior_task_execution_types::{
    advance_behavior_task_to_next_action, find_current_behavior_task_action,
    BehaviorTaskExecutionPhase, BehaviorTaskExecutionState,
};

pub(super) fn find_target_and_enter_success_or_failure_behavior_set(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Res<ZooClock>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    mut tasks: Query<(
        Entity,
        &mut BehaviorTaskExecutionState,
        Option<&GlobalTransform>,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
    )>,
    candidates: Query<(
        Entity,
        Option<&DefinitionId>,
        Option<&FenceEdge>,
        &GlobalTransform,
    )>,
    mut commands: Commands,
) {
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, mut task, origin, species, staff, guest) in &mut tasks {
        if task.next_action_tick > clock.tick {
            continue;
        }
        let Some(BehaviorAction::TargetTest(action)) = assets
            .get(&task.document)
            .and_then(|asset| find_current_behavior_task_action(asset, &task))
            .cloned()
        else {
            continue;
        };
        let maximum_distance_squared = (action.target_radius_cm as f32 * 0.01).powi(2);
        let matches = |candidate: Option<&DefinitionId>, fence: Option<&FenceEdge>| {
            let definition = match action.target_test_kind {
                BehaviorTargetTestKind::Fence => fence.map(|fence| fence.definition),
                BehaviorTargetTestKind::Position => candidate.map(|candidate| candidate.0),
            };
            action
                .target_definition_asset_id
                .is_none_or(|required| definition == Some(required))
        };
        let selected = task
            .target
            .filter(|target| {
                candidates
                    .get(*target)
                    .is_ok_and(|(_, definition, fence, transform)| {
                        matches(definition, fence)
                            && candidate_is_within_authored_target_radius(
                                origin,
                                transform,
                                maximum_distance_squared,
                                action.target_radius_cm,
                            )
                    })
            })
            .or_else(|| {
                candidates
                    .iter()
                    .filter(|(candidate, definition, fence, transform)| {
                        *candidate != entity
                            && matches(*definition, *fence)
                            && candidate_is_within_authored_target_radius(
                                origin,
                                transform,
                                maximum_distance_squared,
                                action.target_radius_cm,
                            )
                    })
                    .min_by(|left, right| {
                        calculate_squared_distance_from_optional_origin(origin, left.3).total_cmp(
                            &calculate_squared_distance_from_optional_origin(origin, right.3),
                        )
                    })
                    .map(|(candidate, ..)| candidate)
            });
        task.target = selected;
        let branch = if selected.is_some() {
            action.success_behavior_set_asset_id
        } else {
            action.failure_behavior_set_asset_id
        };
        let Some(program) = branch else {
            if selected.is_some() {
                advance_behavior_task_to_next_action(&mut task, clock.tick);
            } else {
                mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            }
            continue;
        };
        let Some((document, declaration)) = view.find_behavior_set_location(
            program,
            super::behavior_subject_type_resolution::behavior_subject_type_identifiers(
                species,
                staff,
                guest,
                definitions,
            ),
        ) else {
            mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            continue;
        };
        let frame = task.create_return_frame_after_action(task.action.saturating_add(1));
        if !task.push_return_frame(frame) {
            mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            continue;
        }
        task.program = program;
        task.document = document;
        task.declaration = declaration;
        task.phase = BehaviorTaskExecutionPhase::Set;
        task.action = 0;
        task.repetitions = 0;
        task.next_action_tick = clock.tick.saturating_add(1);
    }
}

fn candidate_is_within_authored_target_radius(
    origin: Option<&GlobalTransform>,
    target: &GlobalTransform,
    maximum_distance_squared: f32,
    radius_cm: u32,
) -> bool {
    radius_cm == 0
        || origin.is_some_and(|origin| {
            origin.translation().distance_squared(target.translation()) <= maximum_distance_squared
        })
}

fn calculate_squared_distance_from_optional_origin(
    origin: Option<&GlobalTransform>,
    target: &GlobalTransform,
) -> f32 {
    origin.map_or(0.0, |origin| {
        origin.translation().distance_squared(target.translation())
    })
}
