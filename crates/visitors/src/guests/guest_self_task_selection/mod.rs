//! Bounded entry into the authored task executor for idle guests.

use openzt2_zoo_state::behavior_task_execution_types::allocate_behavior_execution_id;
mod target_index;
use openzt2_zoo_state::animal_behavior::behavior_token_types::{
    BehaviorSelfToken, BehaviorTokenContext, ClaimedBehaviorSelfToken,
};

use openzt2_zoo_state::guests::guest_simulation_types::GuestEntranceApproach;
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
use openzt2_zoo_state::animal_behavior::behavior_candidate_selection::{BehaviorCandidateRanking, BehaviorTargetBiome};
use openzt2_zoo_state::animal_behavior::behavior_task_effective_score_types::BehaviorTaskEffectiveScore;
use openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet;
use openzt2_zoo_state::behavior_task_execution_types::SynchronizedBehaviorParticipant;
use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;
use openzt2_world_state::animation_graph::animation_presentation_relationship_types::AnimationPresentations;
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionPhase;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskGeneration;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use openzt2_zoo_state::economy::price_effect::BehaviorPriceEffectContext;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::GuestVisitedTourStations;
use openzt2_zoo_state::economy::service_types::ServiceReservation;
use openzt2_zoo_state::locomotion::locomotion_types::Destination;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_zoo_state::topology::topology_graph_types::PathTile;
use openzt2_zoo_state::shows::running_show_types::{GuestShowNeed, ShowViewingLocations};
use bevy::ecs::relationship::RelationshipTarget;
use openzt2_zoo_state::transport_tours::transport_rider_types::TransportRider;
use openzt2_zoo_state::transport_tours::transport_rider_types::WaitingForTransport;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::guest_viewing_area_sensor::{GuestViewingSensor, ViewingAreaSensors};
use super::{
    guest_target_eligibility::satisfies_guest_target_fact,
    guest_task_need_facts::GuestTaskNeedFacts,
};
use openzt2_zoo_state::{
    donations::donation_guest_decision_types::GuestDonationMeters,
    guests::guest_simulation_types::GuestFavouriteAnimal,
};

use super::guest_simulation_types::{
    Guest, GuestArchetype, GuestDeparturePoints, GuestDestination, GuestPhase, GuestRng, Viewing,
};
use openzt2_world_state::world_spawn::world_membership_types::target_type_index::WorldTargetTypeQueries;

#[derive(Resource, Default, PartialEq)]
pub(super) struct GuestSubjectTaskIndex {
    by_archetype: HashMap<AssetId, Vec<(Handle<BehaviorDocumentAsset>, usize)>>,
}

pub(super) fn refresh_guest_subject_tasks(
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    mut index: ResMut<GuestSubjectTaskIndex>,
) {
    if !(documents.is_changed()
        || assets.is_changed()
        || world_assets.is_changed()
        || world_definitions.is_changed())
    {
        return;
    }
    let mut next = GuestSubjectTaskIndex::default();
    if let (Some(view), Some(definitions)) = (
        documents.create_declaration_index_view(&assets),
        world_definitions.get(&world_assets),
    ) {
        for definition in definitions.guest_definitions() {
            let subject_types = definition.behavior_subject_type_identifiers.as_slice();
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
            next.by_archetype.insert(definition.id, declarations);
        }
    }
    index.set_if_neq(next);
}

pub(super) fn select_idle_guest_tasks(
    target_retry_delays: BehaviorTargetRejections,
    (documents, assets, subject_tasks): (
        Res<LoadedBehaviorDocumentCollection>,
        Res<Assets<BehaviorDocumentAsset>>,
        Res<GuestSubjectTaskIndex>,
    ),
    (world_assets, world_definitions): (Res<Assets<WorldDefinitionAsset>>, Res<WorldDefinitions>),
    (clock, fixed_time, mut shared_random): (
        Single<&ZooClock>,
        Res<Time<Fixed>>,
        ResMut<openzt2_world_state::simulation_time::native_visual_random_stream::NativeVisualRandomStream>,
    ),
    (viewing_sensors, admissions): (
        ViewingAreaSensors,
        Query<&openzt2_zoo_state::economy::guest_admission_types::ZooAdmissionsOpen>,
    ),
    (price_effect, biomes): (BehaviorPriceEffectContext, BehaviorTargetBiome),
    (mut behavior_generations, mut effective_scores): (
        Query<&mut BehaviorTaskGeneration>,
        Query<&mut BehaviorTaskEffectiveScore>,
    ),
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            &GuestPhase,
            &mut GuestRng,
            GuestTaskNeedFacts,
            &GuestNeedTriggers,
            Option<&GuestDeparturePoints>,
            Option<&GuestShowNeed>,
            (Option<&openzt2_zoo_state::guests::guest_simulation_types::GuestSpeciesViewCounts>,
                Option<&openzt2_zoo_state::behavior_task_execution_types::running_actions::GuestTourDepartureGate>,
                Option<&GuestVisitedTourStations>),
            &GlobalTransform,
            &WorldMember,
            Option<&mut GuestTaskCooldowns>,
            (&mut GuestViewingSensor, Option<&GuestFavouriteAnimal>, Option<&GuestDonationMeters>),
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
            Without<GuestEntranceApproach>,
        ),
    >,
    (targets, held, indexed_targets): (
        Query<(
            Entity,
            &DefinitionId,
            &GlobalTransform,
            &WorldMember,
            Option<&FacilityContainedWaste>,
        )>,
        Query<(&DefinitionId, Option<&FacilityContainedWaste>)>,
        WorldTargetTypeQueries,
    ),
    (token_owners, tokens, token_contexts): (
        Query<&Children>,
        Query<(Entity, &BehaviorSelfToken), Without<ClaimedBehaviorSelfToken>>,
        Query<&BehaviorTokenContext>,
    ),
    memberships: InteractionContainerMemberships,
    (show_locations, location_targets): (
        Query<(&WorldMember, &ShowViewingLocations)>,
        Query<&ChildOf>,
    ),
    (paths, presentations, controllers, stations, circuits): (
        Query<(Entity, &GlobalTransform, &WorldMember, &PathTile)>,
        Query<&AnimationPresentations>,
        Query<
            (),
            (
                With<AnimationPlaybackController>,
                With<AnimationGraphPlaybackState>,
            ),
        >,
        Query<
            (
                Entity,
                &GlobalTransform,
                &WorldMember,
                &openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitMember,
            ),
            With<openzt2_zoo_state::transport_tours::transport_topology_types::TransportStation>,
        >,
        Query<(
            &openzt2_zoo_state::transport_tours::transport_circuit_types::TransportCircuit,
            Option<&openzt2_zoo_state::transport_tours::transport_circuit_types::CircuitRunning>,
        )>,
    ),
    mut commands: Commands,
) {
    if documents.create_declaration_index_view(&assets).is_none() {
        return;
    }
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
    // The animal-view sensor offers path tiles with a view from the path.
    let view_sensor = AssetId::from_key("viewanimals");
    let path_sensor = AssetId::from_key("path");
    let watch_sensor = AssetId::from_key("watchshow");
    let watch_token = AssetId::from_key("t_watchshow");
    let show_attribute = AssetId::from_key("show");
    let tour_attribute = AssetId::from_key("tour");
    let station_type = AssetId::from_key("station");
    let mut target_types = Vec::new();
    let mut combined_targets = Vec::new();
    for (
        guest,
        archetype,
        phase,
        mut random,
        need_facts,
        triggers,
        departure_points,
        show_need,
        (species_seen, tour_gate, visited_stations),
        transform,
        world,
        mut cooldowns,
        (mut viewing_sensor, favourite, donation_history),
    ) in &mut guests
    {
        if let Some(cooldowns) = cooldowns.as_deref_mut() {
            cooldowns.observe_idle(clock.tick);
        }
        // Gate refusals are reported by the per-guest task progress diagnostics.
        if !transform.translation().is_finite()
            || *phase != GuestPhase::Visiting
            || !AnimationPresentations::for_subject(guest, presentations.get(guest).ok())
                .any(|controller| controllers.contains(controller))
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
        let active_tokens: Vec<_> = tokens
            .iter_many(token_owners.get(guest).ok().into_iter().flatten().copied())
            .flatten()
            .filter(|(_, token)| token.is_available_at(clock.tick))
            .collect();
        let source_need = |attribute| {
            if attribute == show_attribute {
                show_need.map(GuestShowNeed::source_need)
            } else if attribute == tour_attribute {
                tour_gate.map(|tour| tour.source_need())
            } else {
                need_facts.source_need(attribute, triggers)
            }
        };
        let mut show_targets = Vec::new();
        let watching = active_tokens
            .iter()
            .find(|(_, token)| token.name == watch_token);
        if let Some((_, token)) = watching {
            if let Some((_, locations)) = token
                .payload
                .and_then(|stage| show_locations.get(stage).ok())
                .filter(|(member, _)| member.root == world.root)
            {
                show_targets.extend(locations.iter().filter_map(|location| {
                    let entity = location_targets.get(location).ok()?.parent();
                    if let Ok((_, pose, member, tile)) = paths.get(entity) {
                        if member.root != world.root {
                            return None;
                        }
                        let elevated = definitions
                            .find_path(tile.definition)
                            .is_some_and(|path| path.elevated);
                        Some((entity, None, Some(elevated), None, pose.translation()))
                    } else {
                        let (_, id, pose, member, waste) = targets.get(entity).ok()?;
                        if member.root != world.root {
                            return None;
                        }
                        Some((
                            entity,
                            Some(definitions.find_object(id.0)?),
                            None,
                            waste.map(|waste| waste.contained_waste_units),
                            pose.translation(),
                        ))
                    }
                }));
                let here = transform.translation();
                show_targets.sort_unstable_by(|left, right| {
                    here.distance_squared(left.4)
                        .total_cmp(&here.distance_squared(right.4))
                        .then_with(|| left.0.cmp(&right.0))
                });
            }
        }
        let mut ranking = BehaviorCandidateRanking::new(policy.minimum_score);
        let diagnostics = bevy::log::tracing::enabled!(bevy::log::Level::DEBUG);
        let Some(declarations) = subject_tasks.by_archetype.get(&archetype.0) else {
            continue;
        };
        if clock.tick >= viewing_sensor.next_capacity_refresh_tick {
            if let Some(visible) = viewing_sensors
                .worklists
                .get(world.root)
                .ok()
                .and_then(|worklist| worklist.mode_one_visible_species_count)
            {
                viewing_sensor.num_to_see = visible;
                if let Some(timestep) = definitions.simulation_timestep() {
                    let interval = 5_000_000_000_u128.div_ceil(timestep.as_nanos());
                    viewing_sensor.next_capacity_refresh_tick = clock
                        .tick
                        .saturating_add(u64::try_from(interval).unwrap_or(u64::MAX));
                }
            }
        }
        let mut viewing_candidates = None;
        for (document, declaration) in declarations {
            if ranking.has_forced_candidate() {
                break;
            }
            let declaration = *declaration;
            let Some(task) = assets
                .get(document)
                .and_then(|asset| asset.behavior_task_at_index(declaration))
            else {
                continue;
            };
            let leaves_zoo = task.scores.iter().any(|score| {
                matches!(
                    score,
                    openzt2_game_data::behavior::score::BehaviorScore::LeaveZooPressure
                )
            });
            let ordinary_score_task = leaves_zoo.then(|| {
                let mut ordinary = task.clone();
                ordinary.scores.retain(|score| {
                    !matches!(
                        score,
                        openzt2_game_data::behavior::score::BehaviorScore::LeaveZooPressure
                    )
                });
                ordinary
            });
            let can_board_tour = |station, root, circuit| {
                root == world.root
                    && visited_stations.is_none_or(|visited| !visited.0.contains(&station))
                    && circuits.get(circuit).is_ok_and(|(route, running)| {
                        !route.closed && running.is_none_or(|running| running.0)
                    })
            };
            let departure_capacity = if leaves_zoo {
                let available_tour = tour_gate
                    .filter(|_| {
                        stations.iter().any(|(station, _, member, circuit)| {
                            can_board_tour(station, member.root, circuit.0)
                        })
                    })
                    .map(|tour| (tour.need, tour.threshold));
                super::guest_behavior_departure_execution::departure_capacity_allows_score(
                    admissions.get(world.root).is_ok_and(|status| status.0),
                    departure_points.map_or(0, |points| points.0),
                    species_seen.map_or(0, |views| views.0.len()),
                    viewing_sensor.num_to_see,
                    available_tour,
                )
            } else {
                false
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
                            } else if required == station_type && stations.contains(target) {
                                true
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
                if ranking.has_forced_candidate() {
                    return;
                }
                let force = object
                    .and_then(|(token, ..)| token_contexts.get(token).ok())
                    .is_some_and(|context| context.force);
                let score = if force {
                    Some(openzt2_zoo_state::animal_behavior::behavior_candidate_selection::FORCED_BEHAVIOR_TOKEN_SCORE)
                } else if departure_capacity {
                    Some(1_000_000_000.0)
                } else {
                    score_supported_behavior_task(
                        ordinary_score_task.as_ref().unwrap_or(task),
                        policy.outside_range_need_value,
                        transform.translation().distance(target_position),
                        || biomes.at(world.root, target_position),
                        source_need,
                        |value| {
                            price_effect.sample(value, Some(target), |upper| {
                                random.0.range_u32(upper).unwrap_or_default()
                            })
                        },
                    )
                };
                if diagnostics && target_definition.is_some() {
                    let needs = task
                        .scores
                        .iter()
                        .filter_map(|entry| match entry {
                            openzt2_game_data::behavior::score::BehaviorScore::AttributeValue {
                                attribute,
                                ..
                            } => Some(source_need(*attribute)),
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
                ranking.consider_token_candidate(
                    (
                        document,
                        declaration,
                        task,
                        target,
                        object.map(|(item, ..)| item),
                    ),
                    task.priority.unwrap_or_default(),
                    score,
                    force,
                );
            };
            if task.sensor_tag == Some(watch_sensor) {
                if let Some(&(token_entity, token)) = watching {
                    for &(target, object, elevated, waste, position) in &show_targets {
                        if !task.targets.iter().any(|target_type| {
                            let required = AssetId::from_key(target_type);
                            if elevated.is_some() {
                                required == path_sensor
                            } else {
                                object.is_some_and(|object| matches_type(object, required))
                            }
                        }) {
                            continue;
                        }
                        if let (Some(object), Some(tag)) = (object, task.reservation_tag) {
                            let mut slots = object
                                .interaction_slots
                                .iter()
                                .enumerate()
                                .filter(|(_, slot)| slot.reservation_tag == tag && !slot.is_queue)
                                .peekable();
                            if slots.peek().is_some()
                                && !slots.any(|(index, slot)| {
                                    memberships.actor_slot(target, guest) == Some(index)
                                        || memberships.occupied_places(target, index)
                                            < usize::from(slot.capacity)
                                })
                            {
                                continue;
                            }
                        }
                        consider_target(
                            target,
                            object,
                            elevated,
                            waste,
                            position,
                            Some((token_entity, token.name, None, None)),
                            false,
                        );
                    }
                }
                continue;
            }
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
                        for &(entity, token) in &active_tokens {
                            if task
                                .objects
                                .iter()
                                .any(|name| AssetId::from_key(name) == token.name)
                                && (token.name != AssetId::from_key("t_donationtour")
                                    || token.payload == Some(target))
                            {
                                consider_target(
                                    target,
                                    target_definition,
                                    elevated,
                                    waste,
                                    position,
                                    Some((entity, token.name, None, None)),
                                    false,
                                );
                            }
                        }
                    }
                };
            if task.sensor_tag == Some(tour_attribute) {
                for (station, pose, member, circuit) in &stations {
                    if can_board_tour(station, member.root, circuit.0) {
                        let object = targets
                            .get(station)
                            .ok()
                            .and_then(|(_, name, ..)| definitions.find_object(name.0));
                        consider_with_held_items(station, object, None, None, pose.translation());
                    }
                }
                continue;
            }
            if task.targets.iter().any(|target| target == "self") {
                consider_with_held_items(guest, None, None, None, transform.translation());
            }
            if task.sensor_tag == Some(view_sensor) {
                let candidates = viewing_candidates.get_or_insert_with(|| {
                    viewing_sensors.candidates(
                        world.root,
                        transform.translation(),
                        favourite.and_then(|favourite| favourite.0),
                        donation_history,
                        &mut viewing_sensor,
                        || shared_random.next_u15(),
                    )
                });
                for &candidate in candidates.iter() {
                    if let Ok((_, pose, _, tile)) = paths.get(candidate) {
                        if task.targets.iter().any(|target| target == "path") {
                            let elevated = definitions
                                .find_path(tile.definition)
                                .is_some_and(|definition| definition.elevated);
                            consider_with_held_items(
                                candidate,
                                None,
                                Some(elevated),
                                None,
                                pose.translation(),
                            );
                        }
                    } else if let Ok((_, definition, pose, _, waste)) = targets.get(candidate) {
                        if let Some(object) = definitions.find_object(definition.0) {
                            if task
                                .targets
                                .iter()
                                .any(|target| matches_type(object, AssetId::from_key(target)))
                            {
                                consider_with_held_items(
                                    candidate,
                                    Some(object),
                                    None,
                                    waste.map(|waste| waste.contained_waste_units),
                                    pose.translation(),
                                );
                            }
                        }
                    }
                }
                continue;
            }
            if task.targets.iter().any(|target| target == "path") {
                if task.sensor_tag == Some(path_sensor) {
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
                target_index::candidate_types(
                    &task.targets,
                    &task.candidate_eligibility_requirements,
                    &mut target_types,
                );
                if target_types.is_empty() {
                    combined_targets.clear();
                    combined_targets.extend(
                        targets
                            .iter()
                            .filter(|(_, _, _, member, _)| member.root == world.root)
                            .map(|(entity, ..)| entity),
                    );
                } else {
                    indexed_targets.collect(
                        world.root,
                        target_types.iter().copied(),
                        &mut combined_targets,
                    );
                }
                for (entity, id, pose, _, waste) in targets
                    .iter_many(combined_targets.iter().copied())
                    .matched()
                {
                    let Some(object) = definitions.find_object(id.0) else {
                        continue;
                    };
                    let position = pose.translation();
                    if entity != guest
                        && position.is_finite()
                        && (task.targets.is_empty()
                            || task
                                .targets
                                .iter()
                                .map(String::as_str)
                                .filter(regular_target)
                                .any(|required| matches_type(object, AssetId::from_key(required))))
                    {
                        consider_with_held_items(
                            entity,
                            Some(object),
                            None,
                            waste.map(|waste| waste.contained_waste_units),
                            position,
                        );
                    }
                }
            }
            if task.targets.iter().any(|target| target == "fromtoken") {
                for &(token_entity, token) in &active_tokens {
                    if !task
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
        let Some(((document, declaration, task, target, object), score)) =
            ranking.finish(|| random.0.next_u32())
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
        debug!(?guest, task = %task.name, ?target, self_target = target == guest,
            "idle guest chose a task");
        let execution_id = allocate_behavior_execution_id(
            guest,
            behavior_generations.get_mut(guest).ok(),
            &mut commands,
        );
        if let Some(token) = object.filter(|&entity| tokens.contains(entity)) {
            commands
                .entity(token)
                .insert(ClaimedBehaviorSelfToken { execution_id });
        }
        let score = BehaviorTaskEffectiveScore {
            execution_id,
            template_score: Some(task.priority.unwrap_or_default()),
            linked_score: None,
            selected_candidate_score: Some(score),
        };
        if let Ok(mut current) = effective_scores.get_mut(guest) {
            current.set_if_neq(score);
        } else {
            commands.entity(guest).insert(score);
        }
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
