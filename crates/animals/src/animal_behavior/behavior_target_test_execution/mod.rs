//! Canonical behavior target-test execution and authored branch entry.

use bevy::prelude::*;
use openzt2_game_data::behavior::action::target_test::BehaviorTargetTestKind;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    RunBehaviorTargetTest, RunningBehaviorInstruction, SelectedBehaviorSet,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::guests::guest_simulation_types::GuestArchetype;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};

use super::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;

pub(super) fn find_target_and_enter_success_or_failure_behavior_set(
    activation: On<WhenBehaviorAction<RunBehaviorTargetTest>>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    clock: Single<&ZooClock>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    animal_types: super::behavior_subject_type_resolution::AnimalBehaviorSubjectTypes,
    mut tasks: Query<(
        Entity,
        (
            &mut BehaviorTaskExecutionState,
            &RunningBehaviorInstruction,
            &RunBehaviorTargetTest,
        ),
        Option<&GlobalTransform>,
        Option<&SpeciesHandle>,
        Option<&StaffRole>,
        Option<&GuestArchetype>,
    )>,
    positions: Query<(Entity, Option<&DefinitionId>, &GlobalTransform), With<WorldMember>>,
    fences: Query<(Entity, &FenceEdge, &GlobalTransform)>,
    worlds: Query<&WorldMember>,
    spatial: super::behavior_spatial_candidates::BehaviorSpatialCandidates,
    mut commands: Commands,
) {
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (entity, (mut task, instruction, running_action), origin, species, staff, guest) in
        std::iter::once(tasks.get_mut(activation.entity)).flatten()
    {
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(&task) {
            continue;
        }
        let action = &running_action.0;
        let maximum_distance_squared = (action.target_radius_cm as f32 * 0.01).powi(2);
        let matches = |definition, transform: &GlobalTransform| {
            if action
                .target_definition_asset_id
                .is_some_and(|required| definition != Some(required))
            {
                return None;
            }
            let distance_squared = origin.map(|origin| {
                origin
                    .translation()
                    .distance_squared(transform.translation())
            });
            if action.target_radius_cm != 0
                && !distance_squared.is_some_and(|distance| distance <= maximum_distance_squared)
            {
                return None;
            }
            Some(distance_squared.unwrap_or_default())
        };
        let mut bounded_candidates = Vec::new();
        let bounded = action.target_radius_cm != 0
            && origin
                .zip(worlds.get(entity).ok())
                .is_some_and(|(origin, world)| {
                    let margin = Vec2::splat(action.target_radius_cm as f32 * 0.01);
                    let centre = origin.translation().xz();
                    spatial.in_bounds(
                        world.root,
                        centre - margin,
                        centre + margin,
                        &mut bounded_candidates,
                    );
                    spatial.preserve_query_order(&mut bounded_candidates);
                    true
                });
        let selected = match action.target_test_kind {
            BehaviorTargetTestKind::Fence => select_authored_target(
                task.target
                    .and_then(|target| fences.get(target).ok())
                    .map(|(entity, edge, pose)| (entity, Some(edge.definition), pose)),
                bounded
                    .then(|| {
                        fences
                            .iter_many(bounded_candidates.iter().copied())
                            .flatten()
                    })
                    .into_iter()
                    .flatten()
                    .chain((!bounded).then(|| fences.iter()).into_iter().flatten())
                    .map(|(entity, edge, pose)| (entity, Some(edge.definition), pose)),
                entity,
                matches,
            ),
            BehaviorTargetTestKind::Position => select_authored_target(
                task.target
                    .and_then(|target| positions.get(target).ok())
                    .map(|(entity, definition, pose)| {
                        (entity, definition.map(|definition| definition.0), pose)
                    }),
                bounded
                    .then(|| {
                        positions
                            .iter_many(bounded_candidates.iter().copied())
                            .flatten()
                    })
                    .into_iter()
                    .flatten()
                    .chain((!bounded).then(|| positions.iter()).into_iter().flatten())
                    .map(|(entity, definition, pose)| {
                        (entity, definition.map(|definition| definition.0), pose)
                    }),
                entity,
                matches,
            ),
        };
        let branch = if selected.is_some() {
            action.success_behavior_set_asset_id
        } else {
            action.failure_behavior_set_asset_id
        };
        let Some(program) = branch else {
            if selected.is_some() {
                task.target = selected;
                instruction.complete(entity, clock.tick, &mut commands);
            } else {
                task.target = None;
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
                animal_types.borrow_for(entity),
            ),
        ) else {
            task.target = selected;
            mark_behavior_task_for_failure_and_stop_navigation(entity, &mut commands);
            continue;
        };
        instruction.enter_set(
            entity,
            SelectedBehaviorSet {
                program,
                document,
                declaration,
                tick: clock.tick,
                repeat: None,
                target: Some(selected),
            },
            &mut commands,
        );
    }
}

fn select_authored_target<'a>(
    current: Option<(
        Entity,
        Option<openzt2_game_data::AssetId>,
        &'a GlobalTransform,
    )>,
    candidates: impl Iterator<
        Item = (
            Entity,
            Option<openzt2_game_data::AssetId>,
            &'a GlobalTransform,
        ),
    >,
    actor: Entity,
    distance_if_eligible: impl Fn(Option<openzt2_game_data::AssetId>, &GlobalTransform) -> Option<f32>,
) -> Option<Entity> {
    current
        .and_then(|(candidate, definition, pose)| {
            distance_if_eligible(definition, pose).map(|_| candidate)
        })
        .or_else(|| {
            candidates
                .filter(|(candidate, ..)| *candidate != actor)
                .filter_map(|(candidate, definition, pose)| {
                    distance_if_eligible(definition, pose).map(|distance| (candidate, distance))
                })
                .min_by(|left, right| left.1.total_cmp(&right.1))
                .map(|(candidate, _)| candidate)
        })
}
