//! Bounded entry into the authored task executor for idle guests.
mod target_index;

use openzt2_zoo_state::guests::guest_simulation_types::GuestNeedTriggers;
use openzt2_zoo_state::guests::guest_simulation_types::GuestTaskCooldowns;

use arrayvec::ArrayVec;
use bevy::{platform::collections::HashMap, prelude::*};
use openzt2_game_data::{
    behavior::eligibility::BehaviorCandidateRole,
    world_definitions::world_objects::WorldObjectDefinition, AssetId,
};

use openzt2_game_assets::behavior::behavior_asset_types::BehaviorDocumentAsset;
use openzt2_game_assets::behavior::behavior_asset_types::LoadedBehaviorDocumentCollection;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy;
use openzt2_zoo_state::animal_behavior::behavior_target_retry_delays::BehaviorTargetRetryDelays;
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
use openzt2_zoo_state::habitat::habitat_membership_and_containment::locate_habitat_at_world_position;
use openzt2_zoo_state::habitat::habitat_types::HabitatIndex;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::habitat::habitat_types::HabitatMember;
use openzt2_zoo_state::topology::topology_graph_types::FenceEdge;
use openzt2_zoo_state::topology::topology_graph_types::TopologyGrid;
use openzt2_zoo_state::topology::topology_graph_types::TopologyNode;
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
            Option<&BehaviorTargetRetryDelays>,
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
    held: Query<&DefinitionId>,
    occupancy: Res<InteractionContainerOccupancy>,
    paths: Query<(Entity, &GlobalTransform, &WorldMember, &PathTile)>,
    viewable_animals: Query<
        (&GlobalTransform, &Visibility, &WorldMember, &HabitatMember),
        With<Animal>,
    >,
    (habitat_lookup, busy_guests, (regions, fence_edges, topology_nodes)): (
        (Option<Res<HabitatIndex>>, Option<Res<TopologyGrid>>),
        Query<&BehaviorTaskExecutionState, With<Guest>>,
        (
            Query<&HabitatRegion>,
            Query<&FenceEdge>,
            Query<&TopologyNode>,
        ),
    ),
    controllers: Query<
        &AnimationPresentationOwner,
        (
            With<AnimationPlaybackController>,
            With<AnimationGraphPlaybackState>,
        ),
    >,
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
    // finds: path tiles beside a habitat, within the authored viewing
    // distance of it, with an animal of that habitat within viewing range.
    let view_sensor = AssetId::from_key("viewanimals");
    let path_sensor = AssetId::from_key("path");
    let (habitats, topology) = habitat_lookup;
    let beside_habitat = |path: Vec2, habitat: Entity, distance_m: f32| {
        let (Some(habitats), Some(topology)) = (habitats.as_deref(), topology.as_deref()) else {
            return true;
        };
        path_is_beside_habitat(habitats, *topology, path, habitat, distance_m)
    };
    // A wall guests cannot see through, such as concrete, hides the animals
    // behind it; the sight line enters the habitat across its boundary.
    let view_is_walled_off = |path: Vec2, animal: Vec2, habitat: Entity| {
        let (Some(topology), Ok(region)) = (topology.as_deref(), regions.get(habitat)) else {
            return false;
        };
        region.boundary_fence_entities.iter().any(|&fence| {
            let Ok(edge) = fence_edges.get(fence) else {
                return false;
            };
            let (Ok(a), Ok(b)) = (topology_nodes.get(edge.a), topology_nodes.get(edge.b)) else {
                return false;
            };
            definitions
                .find_fence(edge.definition)
                .is_some_and(|definition| definition.blocks_view)
                && segments_cross(
                    path,
                    animal,
                    topology.cell_translation(a.cell).xz(),
                    topology.cell_translation(b.cell).xz(),
                )
        })
    };
    let viewing_paths = definitions
        .guest_viewing()
        .map(|viewing| {
            let range_squared = viewing.ground_range_m * viewing.ground_range_m;
            paths
                .iter()
                .filter(|(_, path, path_world, _)| {
                    let path = path.translation().xz();
                    viewable_animals
                        .iter()
                        .any(|(animal, visibility, animal_world, member)| {
                            animal_world.root == path_world.root
                                && *visibility != Visibility::Hidden
                                && animal.translation().xz().distance_squared(path) <= range_squared
                                && beside_habitat(
                                    path,
                                    member.habitat_entity,
                                    viewing.ground_distance_m,
                                )
                                && !view_is_walled_off(
                                    path,
                                    animal.translation().xz(),
                                    member.habitat_entity,
                                )
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
    // Like the task's `Use_Path` reservation, one guest views from a path
    // tile at a time, so watchers spread along the fence instead of crowding
    // onto the best spot.
    let mut reserved_viewing_paths = busy_guests
        .iter()
        .filter_map(|task| task.target())
        .collect::<bevy::platform::collections::HashSet<_>>();
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
        target_retry_delays,
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
        let held_items = occupancy
            .members_of_container(guest)
            .filter_map(|item| {
                let object = definitions.find_object(held.get(item).ok()?.0)?;
                Some((item, object))
            })
            .collect::<Vec<_>>();
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let diagnostics = bevy::log::tracing::enabled!(bevy::log::Level::DEBUG);
        let subject_types = definition.behavior_subject_type_identifiers.as_slice();
        if !subject_tasks.by_subject_types.contains_key(subject_types) {
            let declarations = view
                .behavior_tasks()
                .filter(|(_, _, task)| {
                    // Token candidates still require their own producers.
                    task.targets.len() == 1
                        && task.targets[0] != "fromtoken"
                        && task
                            .candidate_eligibility_requirements
                            .iter()
                            .any(|requirement| {
                                requirement.candidate_role == BehaviorCandidateRole::Subject
                                    && !requirement.candidate_type_identifiers.is_empty()
                            })
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
            // A task that uses an item takes the first held item its `Objects` name.
            let object = if task.objects.is_empty() {
                None
            } else {
                let Some(&(item, object)) = held_items.iter().find(|(_, object)| {
                    candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| {
                            role != BehaviorCandidateRole::Object || matches_type(object, required)
                        },
                        |role, _| role != BehaviorCandidateRole::Object,
                    )
                }) else {
                    continue;
                };
                Some((item, object))
            };
            let mut consider_target =
                |target: Entity,
                 target_definition: Option<&WorldObjectDefinition>,
                 elevated_path: Option<bool>,
                 target_trash_level: Option<u16>,
                 target_position: Vec3| {
                    if target_retry_delays.is_some_and(|delays| delays.rejects(target)) {
                        return;
                    }
                    let is_path = elevated_path.is_some();
                    let is_self = target == guest;
                    if !candidate_requirements_are_satisfied(
                        &task.candidate_eligibility_requirements,
                        |role, required| match role {
                            BehaviorCandidateRole::Subject => true,
                            BehaviorCandidateRole::Target => {
                                if is_self {
                                    required == AssetId::from_key("self")
                                } else if is_path {
                                    required == AssetId::from_key("path")
                                } else {
                                    target_definition
                                        .is_some_and(|object| matches_type(object, required))
                                }
                            }
                            BehaviorCandidateRole::Object => {
                                object.is_some_and(|(_, object)| matches_type(object, required))
                            }
                        },
                        |role, fact| {
                            if role == BehaviorCandidateRole::Object {
                                return false;
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
                            .filter_map(|entry| {
                                match entry {
                                openzt2_game_data::behavior::score::BehaviorScore::AttributeValue {
                                    attribute,
                                    ..
                                } => Some(need_facts.source_need(*attribute, triggers)),
                                _ => None,
                            }
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
                            object.map(|(item, _)| item),
                        ),
                        task.priority.unwrap_or_default(),
                        score,
                        |upper| random.0.range_u32(upper),
                    );
                };
            if task.targets[0] == "self" {
                consider_target(guest, None, None, None, transform.translation());
            } else if task.targets[0] == "path" {
                if task.sensor_tag == Some(view_sensor) {
                    for &(path, position, root, elevated) in &viewing_paths {
                        if root == world.root && !reserved_viewing_paths.contains(&path) {
                            consider_target(path, None, Some(elevated), None, position);
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
                        consider_target(
                            path,
                            None,
                            Some(elevated),
                            None,
                            path_transform.translation(),
                        );
                    }
                }
            } else {
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
                    if target.entity != guest {
                        consider_target(
                            target.entity,
                            Some(target.object),
                            None,
                            target.waste,
                            target.position,
                        );
                    }
                }
            }
        }
        // The supported candidate subset cannot prove native singleton status.
        let Some(((document, declaration, task, target, object), score)) = ranking.finish(false)
        else {
            debug!(?guest, "idle guest found no task scoring above the minimum");
            continue;
        };
        reserved_viewing_paths.insert(target);
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
            stack: ArrayVec::new(),
        });
    }
}

/// Whether the segments `a0`-`a1` and `b0`-`b1` cross each other.
fn segments_cross(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> bool {
    let side = |p: Vec2, q: Vec2, r: Vec2| (q - p).perp_dot(r - p);
    side(a0, a1, b0) * side(a0, a1, b1) < 0.0 && side(b0, b1, a0) * side(b0, b1, a1) < 0.0
}

/// Whether any of `habitat`'s ground lies closer than `distance_m` to `path`.
fn path_is_beside_habitat(
    habitats: &HabitatIndex,
    topology: TopologyGrid,
    path: Vec2,
    habitat: Entity,
    distance_m: f32,
) -> bool {
    let step = topology.spacing_m.max(0.5);
    let steps = (distance_m / step).ceil() as i32;
    (-steps..=steps).any(|z| {
        (-steps..=steps).any(|x| {
            let offset = Vec2::new(x as f32, z as f32) * step;
            offset.length_squared() < distance_m * distance_m
                && locate_habitat_at_world_position(habitats, path + offset, topology)
                    == Some(habitat)
        })
    })
}

#[cfg(test)]
mod viewing_path_tests {
    use super::*;

    #[test]
    fn only_a_path_near_the_habitat_views_its_animals() {
        let habitat = Entity::from_raw_u32(7).unwrap();
        let mut habitats = HabitatIndex::default();
        habitats.record_habitat_for_topology_cell(IVec2::new(0, 0), habitat);
        let topology = TopologyGrid::default();
        assert!(path_is_beside_habitat(
            &habitats,
            topology,
            Vec2::new(5.0, 0.5),
            habitat,
            6.0
        ));
        // Habitat ground exactly at the viewing distance is too far.
        assert!(!path_is_beside_habitat(
            &habitats,
            topology,
            Vec2::new(6.5, 0.5),
            habitat,
            6.0
        ));
        assert!(segments_cross(
            Vec2::ZERO,
            Vec2::X * 4.0,
            Vec2::new(2.0, -1.0),
            Vec2::new(2.0, 1.0)
        ));
        assert!(!segments_cross(
            Vec2::ZERO,
            Vec2::X * 4.0,
            Vec2::new(5.0, -1.0),
            Vec2::new(5.0, 1.0)
        ));
        // A stand 20 m away sees the animal within range but not the habitat.
        assert!(!path_is_beside_habitat(
            &habitats,
            topology,
            Vec2::new(20.0, 0.5),
            habitat,
            6.0
        ));
    }
}
