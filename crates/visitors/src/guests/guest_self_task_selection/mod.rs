//! Bounded entry into the authored task executor for idle guests.
mod target_index;
use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, ClaimedBehaviorSelfToken,
};

use openzt2_zoo_state::guests::guest_simulation_types::GuestNeedTriggers;
use openzt2_zoo_state::guests::guest_simulation_types::GuestTaskCooldowns;

use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::BehaviorCandidateRole,
    world_definitions::world_objects::WorldObjectDefinition, AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::interaction_container_membership::InteractionContainerMemberships;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRejections;
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::candidate_requirements_are_satisfied;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::score_supported_behavior_task;
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::BehaviorCandidateRanking;
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentationOwner;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionSequence;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::price_effect::BehaviorPriceEffectContext;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::{
    guest_target_eligibility::satisfies_guest_target_fact,
    guest_task_need_facts::GuestTaskNeedFacts,
};

use super::guest_simulation_types::{
    Guest, GuestArchetype, GuestDestination, GuestPhase, GuestRng, Viewing,
};
use target_index::{GuestTarget, GuestTargetIndex};

#[derive(Default)]
pub(super) struct GuestSubjectTaskIndex {
    by_subject_types: HashMap<Vec<AssetId>, Vec<(Handle<BehaviorDocumentAsset>, usize)>>,
}

pub(super) fn select_idle_guest_tasks(
    target_retry_delays: BehaviorTargetRejections,
    (documents, assets, mut subject_tasks): (
        Res<LoadedBehaviorDocumentCollection>,
        Res<Assets<BehaviorDocumentAsset>>,
        Local<GuestSubjectTaskIndex>,
    ),
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Res<ZooClock>,
    fixed_time: Res<Time<Fixed>>,
    price_effect: BehaviorPriceEffectContext,
    mut executions: ResMut<BehaviorTaskExecutionSequence>,
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            &GuestPhase,
            &mut GuestRng,
            GuestTaskNeedFacts,
            &GuestNeedTriggers,
            &GlobalTransform,
            &WorldMember,
            Option<&mut GuestTaskCooldowns>,
        ),
        (
            With<Guest>,
            Without<BehaviorTaskExecutionState>,
            Without<PendingBehaviorSet>,
            Without<GuestDestination>,
            Without<Destination>,
            Without<ServiceReservation>,
            Without<Viewing>,
            Without<WaitingForTransport>,
            Without<TransportRider>,
            Without<SynchronizedBehaviorParticipant>,
        ),
    >,
    targets: Query<(
        Entity,
        &DefinitionId,
        &GlobalTransform,
        &WorldMember,
        Option<&FacilityContainedWaste>,
    )>,
    held: Query<(&DefinitionId, Option<&FacilityContainedWaste>)>,
    (token_owners, tokens): (
        Query<&Children>,
        Query<(Entity, &BehaviorSelfToken), Without<ClaimedBehaviorSelfToken>>,
    ),
    memberships: InteractionContainerMemberships,
    (paths, viewable_animals, controllers): (
        Query<(Entity, &GlobalTransform, &WorldMember, &PathTile)>,
        Query<(&GlobalTransform, &Visibility, &WorldMember), (With<Animal>, With<HabitatMember>)>,
        Query<
            &AnimationPresentationOwner,
            (
                With<AnimationPlaybackController>,
                With<AnimationGraphPlaybackState>,
            ),
        >,
    ),
    mut commands: Commands,
) {
    if documents.is_changed() || assets.is_changed() {
        subject_tasks.by_subject_types.clear();
    }
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    let Some(policy) = definitions.behavior_selection_policy() else {
        return;
    };
    if !policy.enabled {
        return;
    }
    if guests.is_empty() {
        return;
    }
    // The original offers viewing tasks the paths its `viewanimals` sensor
    // finds: path tiles with an animal in its habitat within viewing range.
    let view_sensor = AssetId::from_key("viewanimals");
    let path_sensor = AssetId::from_key("path");
    let viewing_paths = definitions
        .guest_viewing()
        .map(|viewing| {
            let range_squared = viewing.ground_range_m * viewing.ground_range_m;
            paths
                .iter()
                .filter(|(_, path, path_world, _)| {
                    viewable_animals
                        .iter()
                        .any(|(animal, visibility, animal_world)| {
                            animal_world.root == path_world.root
                                && *visibility != Visibility::Hidden
                                && animal
                                    .translation()
                                    .xz()
                                    .distance_squared(path.translation().xz())
                                    <= range_squared
                        })
                })
                .map(|(path, transform, path_world, tile)| {
                    let elevated = definitions
                        .find_path(tile.definition)
                        .is_some_and(|definition| definition.elevated);
                    (path, transform.translation(), path_world.root, elevated)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut target_index = None;
    let mut combined_targets = Vec::new();
    for (
        guest,
        archetype,
        phase,
        mut random,
        need_facts,
        triggers,
        transform,
        world,
        mut cooldowns,
    ) in &mut guests
    {
        if let Some(cooldowns) = cooldowns.as_deref_mut() {
            cooldowns.observe_idle(clock.tick);
        }
        // Gate refusals are reported by the per-guest task progress diagnostics.
        if !transform.translation().is_finite()
            || *phase != GuestPhase::Visiting
            || !controllers
                .iter()
                .any(|owner| owner.gameplay_entity == guest)
        {
            continue;
        }
        let Some(definition) = definitions.find_guest(archetype.0) else {
            debug!(?guest, archetype = ?archetype.0, "idle guest has no guest definition");
            continue;
        };
        // Items the guest holds, such as a bought burger, for tasks that use one.
        let held_items = memberships
            .members_of_container(guest)
            .filter_map(|item| {
                let (definition, waste) = held.get(item).ok()?;
                let object = definitions.find_object(definition.0)?;
                Some((item, object, waste.map(|waste| waste.contained_waste_units)))
            })
            .collect::<Vec<_>>();
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let diagnostics = bevy::log::tracing::enabled!(bevy::log::Level::DEBUG);
        let subject_types = definition.behavior_subject_type_identifiers.as_slice();
        if !subject_tasks.by_subject_types.contains_key(subject_types) {
            let declarations: Vec<_> = view
                .behavior_tasks()
                .filter(|(_, _, task)| {
                    (task.subjects.is_empty()
                        || task
                            .subjects
                            .iter()
                            .any(|subject| subject_types.contains(&AssetId::from_key(subject))))
                        && candidate_requirements_are_satisfied(
                            &task.candidate_eligibility_requirements,
                            |role, required| {
                                role != BehaviorCandidateRole::Subject
                                    || subject_types.contains(&required)
                            },
                            |_, _| true,
                        )
                })
                .map(|(document, declaration, _)| (document.clone(), declaration))
                .collect();
            subject_tasks
                .by_subject_types
                .insert(subject_types.to_vec(), declarations);
        }
        for (document, declaration) in &subject_tasks.by_subject_types[subject_types] {
            let declaration = *declaration;
            let Some(task) = assets
                .get(document)
                .and_then(|asset| asset.behavior_task_at_index(declaration))
            else {
                continue;
            };
            if cooldowns
                .as_deref()
                .is_some_and(|cooldowns| cooldowns.is_cooling(task.id))
            {
                continue;
            }
            // Subject types are indexed. Live needs and triggers still vary by guest.
            if !candidate_requirements_are_satisfied(
                &task.candidate_eligibility_requirements,
                |_, _| true,
                |role, fact| {
                    role != BehaviorCandidateRole::Subject
                        || need_facts.satisfies(
                            fact,
                            triggers,
                            &definition.behavior_subject_type_identifiers,
                        )
                },
            ) {
                continue;
            }
            let matches_type = |object: &WorldObjectDefinition, required: AssetId| {
                object.id == required || object.behavior_type_identifiers.contains(&required)
            };
            let mut consider_target = |target: Entity,
                                       target_definition: Option<&WorldObjectDefinition>,
                                       elevated_path: Option<bool>,
                                       target_trash_level: Option<u16>,
                                       target_position: Vec3,
                                       object: Option<(
                Entity,
                AssetId,
                Option<&WorldObjectDefinition>,
                Option<u16>,
            )>,
                                       from_token: bool| {
                if target_retry_delays.rejects(guest, target) {
                    return;
                }
                let is_path = elevated_path.is_some();
                let is_self = target == guest;
                if !candidate_requirements_are_satisfied(
                    &task.candidate_eligibility_requirements,
                    |role, required| match role {
                        BehaviorCandidateRole::Subject => true,
                        BehaviorCandidateRole::Target => {
                            if from_token && required == AssetId::from_key("fromtoken") {
                                true
                            } else if is_self {
                                required == AssetId::from_key("self")
                            } else if is_path {
                                required == AssetId::from_key("path")
                            } else {
                                target_definition
                                    .is_some_and(|object| matches_type(object, required))
                            }
                        }
                        BehaviorCandidateRole::Object => {
                            object.is_some_and(|(_, key, object, _)| {
                                key == required
                                    || object.is_some_and(|object| matches_type(object, required))
                            })
                        }
                    },
                    |role, fact| {
                        if role == BehaviorCandidateRole::Object {
                            return object.is_some_and(|(_, _, _, waste)| {
                                satisfies_guest_target_fact(
                                    fact,
                                    transform.translation(),
                                    transform.translation(),
                                    None,
                                    waste,
                                )
                            });
                        }
                        if role == BehaviorCandidateRole::Target && !is_self {
                            return satisfies_guest_target_fact(
                                fact,
                                transform.translation(),
                                target_position,
                                elevated_path,
                                target_trash_level,
                            );
                        }
                        need_facts.satisfies(
                            fact,
                            triggers,
                            &definition.behavior_subject_type_identifiers,
                        )
                    },
                ) {
                    // Report world objects of a named type whose qualifiers refused them.
                    if diagnostics
                        && target_definition.is_some()
                        && candidate_requirements_are_satisfied(
                            &task.candidate_eligibility_requirements,
                            |role, required| match role {
                                BehaviorCandidateRole::Target => target_definition
                                    .is_some_and(|object| matches_type(object, required)),
                                _ => true,
                            },
                            |_, _| true,
                        )
                    {
                        debug!(?guest, ?target, task = %task.name,
                                distance_m = transform.translation().xz().distance(target_position.xz()),
                                "guest task target refused by its authored qualifiers");
                    }
                    return;
                }
                let score = score_supported_behavior_task(
                    task,
                    policy.outside_range_need_value,
                    transform.translation().distance(target_position),
                    |attribute| need_facts.source_need(attribute, triggers),
                    |value| {
                        price_effect.sample(value, Some(target), |upper| {
                            random.0.range_u32(upper).unwrap_or_default()
                        })
                    },
                );
                if diagnostics && target_definition.is_some() {
                    let needs = task
                        .scores
                        .iter()
                        .filter_map(|entry| match entry {
                            openzt2_game_data::behavior::score::BehaviorScore::AttributeValue {
                                attribute,
                                ..
                            } => Some(need_facts.source_need(*attribute, triggers)),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    debug!(?guest, ?target, task = %task.name, ?score,
                            minimum_score = policy.minimum_score,
                            needs_q16_and_triggered = ?needs,
                            "guest task target scored");
                }
                let Some(score) = score else {
                    return;
                };
                ranking.consider(
                    (
                        document,
                        declaration,
                        task,
                        target,
                        object.map(|(item, ..)| item),
                        from_token,
                    ),
                    task.priority.unwrap_or_default(),
                    score,
                    |upper| random.0.range_u32(upper),
                );
            };
            let mut consider_with_held_items =
                |target, target_definition, elevated, waste, position| {
                    if task.objects.is_empty() {
                        consider_target(
                            target,
                            target_definition,
                            elevated,
                            waste,
                            position,
                            None,
                            false,
                        );
                    } else {
                        for &(entity, object, waste_level) in &held_items {
                            if task
                                .objects
                                .iter()
                                .any(|name| matches_type(object, AssetId::from_key(name)))
                            {
                                consider_target(
                                    target,
                                    target_definition,
                                    elevated,
                                    waste,
                                    position,
                                    Some((entity, object.id, Some(object), waste_level)),
                                    false,
                                );
                            }
                        }
                    }
                };
            if task.targets.iter().any(|target| target == "self") {
                consider_with_held_items(guest, None, None, None, transform.translation());
            }
            if task.targets.iter().any(|target| target == "path") {
                if task.sensor_tag == Some(view_sensor) {
                    for &(path, position, root, elevated) in &viewing_paths {
                        if root == world.root {
                            consider_with_held_items(path, None, Some(elevated), None, position);
                        }
                    }
                } else if task.sensor_tag == Some(path_sensor) {
                    // The `path` sensor, such as Trash.tsk's
                    // ThrowAwayTrashGround, offers the tile nearest the guest;
                    // scan a sensor radius if the native sensor offers more.
                    let here = transform.translation();
                    if let Some((path, path_transform, _, tile)) = paths
                        .iter()
                        .filter(|(_, _, path_world, _)| path_world.root == world.root)
                        .min_by(|(_, a, _, _), (_, b, _, _)| {
                            a.translation()
                                .distance_squared(here)
                                .total_cmp(&b.translation().distance_squared(here))
                        })
                    {
                        let elevated = definitions
                            .find_path(tile.definition)
                            .is_some_and(|definition| definition.elevated);
                        consider_with_held_items(
                            path,
                            None,
                            Some(elevated),
                            None,
                            path_transform.translation(),
                        );
                    }
                }
            }
            let regular_target = |target: &&str| !matches!(*target, "self" | "path" | "fromtoken");
            if task.targets.is_empty()
                || task
                    .targets
                    .iter()
                    .map(String::as_str)
                    .any(|target| regular_target(&target))
            {
                let index = target_index.get_or_insert_with(|| {
                    let mut index = GuestTargetIndex::default();
                    for (entity, definition, transform, world, waste) in &targets {
                        let position = transform.translation();
                        if !position.is_finite() {
                            continue;
                        }
                        let Some(object) = definitions.find_object(definition.0) else {
                            continue;
                        };
                        index.insert(
                            world.root,
                            GuestTarget {
                                entity,
                                object,
                                position,
                                waste: waste.map(|waste| waste.contained_waste_units),
                            },
                        );
                    }
                    index
                });
                for target in index.candidates(
                    world.root,
                    &task.candidate_eligibility_requirements,
                    &mut combined_targets,
                ) {
                    if target.entity != guest
                        && (task.targets.is_empty()
                            || task
                                .targets
                                .iter()
                                .map(String::as_str)
                                .filter(regular_target)
                                .any(|required| {
                                    matches_type(target.object, AssetId::from_key(required))
                                }))
                    {
                        consider_with_held_items(
                            target.entity,
                            Some(target.object),
                            None,
                            target.waste,
                            target.position,
                        );
                    }
                }
            }
            if task.targets.iter().any(|target| target == "fromtoken") {
                for (token_entity, token) in tokens
                    .iter_many(token_owners.get(guest).ok().into_iter().flatten().copied())
                    .flatten()
                {
                    if clock.tick < token.available_at
                        || clock.tick >= token.expires_at
                        || !task
                            .objects
                            .iter()
                            .any(|name| AssetId::from_key(name) == token.name)
                    {
                        continue;
                    }
                    let Some(payload) = token.payload else {
                        continue;
                    };
                    let Ok((_, definition, position, member, waste)) = targets.get(payload) else {
                        continue;
                    };
                    if member.root != world.root {
                        continue;
                    }
                    consider_target(
                        payload,
                        definitions.find_object(definition.0),
                        None,
                        waste.map(|waste| waste.contained_waste_units),
                        position.translation(),
                        Some((token_entity, token.name, None, None)),
                        true,
                    );
                }
            }
        }
        let Some(((document, declaration, task, target, object, from_token), score)) =
            ranking.finish(true)
        else {
            debug!(?guest, "idle guest found no task scoring above the minimum");
            continue;
        };
        // The authored delay starts once the task ends. An unsupported winner
        // ends at once, so its delay still lets lower-ranked tasks run meanwhile.
        if let Some([minimum, maximum]) =
            task.task_delay_seconds.filter(|delay| *delay != [0.0, 0.0])
        {
            let seconds = minimum + (maximum - minimum).max(0.0) * random.0.unit_f32();
            let delay_ticks =
                (f64::from(seconds) / fixed_time.timestep().as_secs_f64()).ceil() as u64;
            if let Some(cooldowns) = cooldowns.as_deref_mut() {
                cooldowns.start_running(task.id, delay_ticks);
            } else {
                let mut cooldowns = GuestTaskCooldowns::default();
                cooldowns.start_running(task.id, delay_ticks);
                commands.entity(guest).insert(cooldowns);
            }
        }
        // An unsupported winning action does not promote a lower-ranked choice.
        if task.execution.has_unsupported_suffix()
            || task.completion.has_unsupported_suffix()
            || task.failure.has_unsupported_suffix()
        {
            debug!(?guest, task = %task.name, "idle guest's best task has unsupported actions");
            continue;
        }
        debug!(?guest, task = %task.name, ?target, self_target = target == guest,
            "idle guest chose a task");
        let execution_id = executions.next();
        if from_token {
            if let Some(token) = object {
                commands
                    .entity(token)
                    .insert(ClaimedBehaviorSelfToken { execution_id });
            }
        }
        commands.entity(guest).insert(BehaviorTaskEffectiveScore {
            execution_id,
            template_score: Some(task.priority.unwrap_or_default()),
            linked_score: None,
            selected_candidate_score: Some(score),
        });
        commands.entity(guest).insert(BehaviorTaskExecutionState {
            execution_id,
            program: task.id,
            target: Some(target),
            document: document.clone(),
            declaration,
            phase: BehaviorTaskExecutionPhase::Execution,
            action: 0,
            repetitions: 0,
            next_action_tick: clock.tick,
            object,
            interaction_slot: None,
            depth: 0,
        });
    }
}
