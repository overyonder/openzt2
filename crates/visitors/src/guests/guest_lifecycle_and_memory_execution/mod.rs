use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestMemoryKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestSequence;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use super::guest_arrival_calculations::displaced_from_entrance_gate;
use super::guest_simulation_calculations::apply_permille_delta;
use openzt2_zoo_state::locomotion::locomotion_types::SpatialGrid;
use super::guest_simulation_calculations::memory_value;
use super::guest_simulation_calculations::push_memory;
use super::guest_simulation_calculations::should_leave;
use super::guest_simulation_calculations::survey_rate_permille;
use super::guest_simulation_calculations::survey_view_score_permille;
use super::guest_simulation_types::DeparturePending;
use super::guest_simulation_types::Guest;
use super::guest_simulation_types::GuestArchetype;
use super::guest_simulation_types::GuestDeparted;
use super::guest_simulation_types::GuestDeparturePoints;
use super::guest_simulation_types::GuestDestination;
use super::guest_simulation_types::GuestEducation;
use super::guest_simulation_types::GuestEnergy;
use super::guest_simulation_types::GuestHunger;
use super::guest_simulation_types::GuestMemories;
use super::guest_simulation_types::GuestMemoryEntry;
use super::guest_simulation_types::GuestNavigationRequest;
use super::guest_simulation_types::GuestPhase;
use super::guest_simulation_types::GuestReachedEntrance;
use super::guest_simulation_types::GuestReaction;
use super::guest_simulation_types::GuestReactionApplied;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestSatisfaction;
use super::guest_simulation_types::GuestSurvey;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingTarget;
use super::guest_simulation_types::Viewing;
use super::guest_simulation_types::VisitTime;
use openzt2_zoo_state::animal_behavior::behavior_task_failure_transition::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::find_reaction;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn apply_guest_reactions(
    mut emitted: MessageReader<super::view_event_execution::BehaviorViewEventEmitted>,
    mut commands: Commands,
    members: Query<(
        &WorldMember,
        Option<&openzt2_world_state::world_spawn::world_membership_types::DefinitionId>,
    )>,
    observations: Query<(&GuestDestination, Option<&GuestNavigationRequest>)>,
    interruptions: Query<
        (),
        Or<(
            With<BehaviorTaskExecutionState>,
            With<openzt2_zoo_state::animal_behavior::behavior_set_start_request_types::PendingBehaviorSet>,
            With<openzt2_zoo_state::economy::service_types::ServiceReservation>,
            With<openzt2_zoo_state::locomotion::locomotion_types::Destination>,
        )>,
    >,
    mut incoming: MessageReader<GuestReaction>,
    mut applied: MessageWriter<GuestReactionApplied>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    survey: Option<ResMut<GuestSurvey>>,
    mut guests: Query<
        (
            &GuestArchetype,
            &GuestPhase,
            &mut GuestSatisfaction,
            &mut GuestEducation,
            &mut GuestMemories,
            Option<&Viewing>,
            Option<&super::guest_simulation_types::GuestViewReactionReceipt>,
            Option<&BehaviorTaskExecutionState>,
        ),
        With<Guest>,
    >,
) {
    let events: Vec<_> = emitted.read().collect();
    let Some(definitions) = active_definitions.get(&definitions) else {
        incoming.clear();
        return;
    };
    // Deferred component writes must also suppress duplicates in this batch.
    let mut receipts = std::collections::HashMap::new();
    let mut survey = survey;
    let mut reactions: Vec<_> = incoming.read().copied().collect();
    reactions.sort_by_key(|reaction| reaction.view_event.map_or(0, |origin| origin.event_id));
    for reaction in &reactions {
        let Ok((
            archetype,
            phase,
            mut satisfaction,
            mut education,
            mut memories,
            viewing,
            receipt,
            task,
        )) = guests.get_mut(reaction.guest)
        else {
            continue;
        };
        let Some((asset, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let survey_policy = asset.guest_generation().map(|p| &p.need_survey);
        let mut reaction = *reaction;
        if reaction.kind == GuestMemoryKind::AnimalView {
            let Some(origin) = reaction.view_event else {
                continue;
            };
            // A guest watches either from a viewing destination it reached,
            // or through its authored view task (`ZTBehViewAnimals`) at the
            // path that task targets.
            let watching_from_destination = !interruptions.contains(reaction.guest)
                && observations
                    .get(reaction.guest)
                    .is_ok_and(|(destination, request)| {
                        destination.purpose == GuestVisitPurpose::View
                            && Some(destination.entity) == origin.viewing.area
                            && request.is_none_or(|request| request.0.is_none())
                    });
            let watching_through_task = task.is_some_and(|task| {
                origin.viewing.area.is_some() && task.target() == origin.viewing.area
            });
            if viewing != Some(&origin.viewing)
                || origin.viewing.subject != reaction.subject
                || *phase != GuestPhase::Visiting
                || !(watching_from_destination || watching_through_task)
            {
                continue;
            }
            let Some(event) = events.iter().find(|event| {
                event.event_id == origin.event_id && event.source == reaction.subject
            }) else {
                continue;
            };
            let Ok((guest_world, _)) = members.get(reaction.guest) else {
                continue;
            };
            let Ok((subject_world, subject_definition)) = members.get(reaction.subject) else {
                continue;
            };
            if guest_world.root != subject_world.root
                || origin.viewing.area.is_none_or(|area| {
                    !members
                        .get(area)
                        .is_ok_and(|(area_world, _)| area_world.root == guest_world.root)
                })
                || subject_definition
                    .is_none_or(|definition| definition.0 != event.source_definition)
                || definitions
                    .find_object(event.source_definition)
                    .is_none_or(|definition| {
                        !definition
                            .view_data
                            .iter()
                            .any(|row| row.name == event.view_key)
                    })
            {
                continue;
            }
            // Archetypes lowered from the original carry no reaction policy;
            // they react with the event's own authored view score instead.
            let deltas = match find_reaction(definition, GuestMemoryKind::AnimalView) {
                Some(policy) => (policy.satisfaction_delta, policy.education_delta),
                None => {
                    let Some(row) = definitions
                        .find_object(event.source_definition)
                        .and_then(|definition| {
                            definition
                                .view_data
                                .iter()
                                .find(|row| row.name == event.view_key)
                        })
                    else {
                        continue;
                    };
                    (row.view_score.round().clamp(-1_000.0, 1_000.0) as i16, 0)
                }
            };
            if survey_policy.is_none() {
                continue;
            }
            let last = receipts
                .entry(reaction.guest)
                .or_insert_with(|| receipt.map_or(0, |receipt| receipt.0));
            if origin.event_id <= *last {
                continue;
            }
            *last = origin.event_id;
            commands.entity(reaction.guest).insert(
                super::guest_simulation_types::GuestViewReactionReceipt(*last),
            );
            // Read current policy rather than accepting deltas queued before a reload.
            (
                reaction.satisfaction_delta_permille,
                reaction.education_delta_permille,
            ) = deltas;
        }
        let education_delta = survey_policy
            .map(|policy| {
                cap_positive(
                    reaction.education_delta_permille,
                    policy.maximum_education_points_per_view,
                )
            })
            .unwrap_or(reaction.education_delta_permille);
        let entertainment_delta =
            if matches!(reaction.kind, GuestMemoryKind::Show | GuestMemoryKind::Tour) {
                survey_policy
                    .map(|policy| {
                        cap_positive(
                            reaction.satisfaction_delta_permille,
                            policy.maximum_entertainment_points_per_view,
                        )
                    })
                    .unwrap_or(reaction.satisfaction_delta_permille)
            } else {
                reaction.satisfaction_delta_permille
            };
        satisfaction.add_wellness_q16(i32::from(entertainment_delta) * (1 << 16));
        education.0 = apply_permille_delta(education.0, education_delta);
        if let Some(survey) = survey.as_deref_mut() {
            survey.education_points = survey
                .education_points
                .saturating_add(education_delta.max(0) as u32);
            if matches!(reaction.kind, GuestMemoryKind::Show | GuestMemoryKind::Tour) {
                survey.entertainment_points = survey
                    .entertainment_points
                    .saturating_add(entertainment_delta.max(0) as u32);
            }
            if matches!(
                reaction.kind,
                GuestMemoryKind::AnimalView
                    | GuestMemoryKind::Education
                    | GuestMemoryKind::Show
                    | GuestMemoryKind::Tour
            ) {
                let maximum_education = survey_policy
                    .map(|policy| policy.maximum_education_points_per_view)
                    .unwrap_or_default();
                let maximum_entertainment = survey_policy
                    .map(|policy| policy.maximum_entertainment_points_per_view)
                    .unwrap_or_default();
                survey.education_view_rate_permille = survey_rate_permille(
                    education_delta.max(0) as u32,
                    u32::from(maximum_education),
                );
                survey.entertainment_view_rate_permille = survey_rate_permille(
                    entertainment_delta.max(0) as u32,
                    u32::from(maximum_entertainment),
                );
                survey.view_score_permille = survey_view_score_permille(
                    survey.critical_need_rate_permille,
                    survey.education_view_rate_permille,
                    survey.entertainment_view_rate_permille,
                    *phase != GuestPhase::Arriving,
                );
            }
        }
        let replacement = definition.memory.replacement;
        push_memory(
            &mut memories,
            GuestMemoryEntry {
                subject: reaction.subject,
                kind: reaction.kind,
                value_permille: memory_value(reaction.kind, entertainment_delta, education_delta),
                age_ticks: 0,
            },
            replacement,
        );
        applied.write(GuestReactionApplied {
            view_event: reaction.view_event,
            guest: reaction.guest,
            subject: reaction.subject,
            kind: reaction.kind,
            viewing: viewing.copied(),
        });
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn advance_guest_lifecycle(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut arrived: MessageReader<Arrived>,
    mut guests: Query<
        (
            Entity,
            &GuestArchetype,
            &mut GuestPhase,
            &mut VisitTime,
            &GuestSatisfaction,
            &GuestEducation,
            &GuestHunger,
            &GuestThirst,
            &GuestEnergy,
            &GuestRestroom,
            &GuestDeparturePoints,
            Option<&GuestDestination>,
        ),
        With<Guest>,
    >,
    entrances: Query<(
        Entity,
        &PersistentId,
        &WorldMember,
        &ZooEntrance,
        &GlobalTransform,
    )>,
    grid: Res<SpatialGrid>,
    members: Query<&WorldMember, With<Guest>>,
    mut requests: Query<&mut GuestNavigationRequest, With<Guest>>,
    tasks: Query<(), With<BehaviorTaskExecutionState>>,
    mut commands: Commands,
    mut navigation_request_sequence: ResMut<NavigationRequestSequence>,
    mut navigate: MessageWriter<NavigateTo>,
    mut reached_entrance: MessageWriter<GuestReachedEntrance>,
    mut departed: MessageWriter<GuestDeparted>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for arrival in arrived.read() {
        let Ok((guest, _, phase, _, satisfaction, education, _, _, _, _, _, destination)) =
            guests.get_mut(arrival.entity)
        else {
            continue;
        };
        let Ok(mut request) = requests.get_mut(guest) else {
            continue;
        };
        if request.0 != Some(arrival.request_id) {
            continue;
        }
        match *phase {
            GuestPhase::Arriving => {
                request.0 = None;
                commands.entity(guest).remove::<GuestDestination>();
                // Admission owns the Arriving -> Visiting/Leaving decision.
                // Publishing the gate arrival before changing phase prevents
                // a guest from entering the zoo before its checked transfer.
                reached_entrance.write(GuestReachedEntrance { guest });
            }
            GuestPhase::Leaving
                if destination
                    .is_some_and(|destination| destination.purpose == GuestVisitPurpose::Exit) =>
            {
                request.0 = None;
                departed.write(GuestDeparted {
                    guest,
                    satisfaction_permille: satisfaction.0,
                    education_permille: education.0,
                });
                commands.entity(guest).insert(DeparturePending);
            }
            GuestPhase::Visiting | GuestPhase::Leaving => {}
        }
    }

    for (
        guest,
        archetype,
        mut phase,
        mut visit,
        satisfaction,
        _,
        hunger,
        thirst,
        energy,
        restroom,
        departure_points,
        destination,
    ) in &mut guests
    {
        if *phase == GuestPhase::Arriving {
            continue;
        }
        if *phase == GuestPhase::Visiting {
            visit.elapsed_ticks = visit.elapsed_ticks.saturating_add(1);
            if should_leave(
                departure_points.0,
                satisfaction.0,
                hunger.value,
                thirst.value,
                energy.value,
                restroom.value,
            ) {
                info!(target: "openzt2_gameplay_journey", ?guest,
                    satisfaction = satisfaction.0, hunger = hunger.value,
                    thirst = thirst.value, energy = energy.value,
                    restroom = restroom.value, departure_points_q16 = departure_points.0,
                    visit_ticks = visit.elapsed_ticks,
                    "guest leaving after needs evaluation");
                *phase = GuestPhase::Leaving;
                commands
                    .entity(guest)
                    .remove::<(GuestDestination, Viewing, GuestViewingTarget)>();
                if tasks.contains(guest) {
                    mark_behavior_task_for_failure_and_stop_navigation(guest, &mut commands);
                }
            }
        }
        if *phase != GuestPhase::Leaving
            // Let the task owner finish its failure branch before installing
            // exit navigation that task cleanup could otherwise cancel.
            || tasks.contains(guest)
            || destination.is_some_and(|destination| destination.purpose == GuestVisitPurpose::Exit)
        {
            continue;
        }
        let Ok(member) = members.get(guest) else {
            continue;
        };
        let Some((entrance_entity, _, _, entrance, gate)) = entrances
            .iter()
            .filter(|(_, _, entrance_member, _, _)| entrance_member.root == member.root)
            .min_by_key(|(_, id, _, _, _)| id.0)
        else {
            continue;
        };
        let Some((asset, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        let exit = asset
            .guest_generation()
            .and_then(|policy| {
                displaced_from_entrance_gate(
                    entrance.exit,
                    gate.rotation(),
                    policy.departure_offset_m,
                    &grid,
                )
            })
            .unwrap_or(entrance.exit);
        let request_id = navigation_request_sequence.next();
        commands.entity(guest).insert((
            GuestDestination {
                entity: entrance_entity,
                purpose: GuestVisitPurpose::Exit,
            },
            GuestNavigationRequest(Some(request_id)),
        ));
        navigate.write(NavigateTo {
            entity: guest,
            request_id,
            destination: exit,
            arrival_radius_m: f32::from(definition.radius_cm) * 0.01,
        });
    }
}

pub(crate) fn despawn_departed_guests(
    departed: Query<Entity, (With<Guest>, With<DeparturePending>)>,
    mut commands: Commands,
) {
    for guest in &departed {
        commands.entity(guest).despawn();
    }
}

pub(crate) fn age_guest_memories(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    entities: Query<()>,
    mut guests: Query<(&GuestArchetype, &mut GuestMemories), With<Guest>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (archetype, mut memories) in &mut guests {
        let Some((_, definition)) = find_guest_definition(definitions, archetype.0) else {
            continue;
        };
        for entry in &mut memories.entries {
            entry.age_ticks = entry.age_ticks.saturating_add(1);
        }
        memories.entries.retain(|entry| {
            entities.get(entry.subject).is_ok()
                && entry.age_ticks
                    < find_reaction(definition, entry.kind)
                        .map(|reaction| reaction.retention_ticks)
                        .unwrap_or_else(|| definition.memory.retention_ticks)
        });
        let capacity = usize::from(memories.active_capacity).max(1);
        memories.next = (usize::from(memories.next) % capacity) as u16;
    }
}

fn cap_positive(value: i16, maximum: u16) -> i16 {
    if value <= 0 {
        value
    } else {
        value.min(maximum.min(i16::MAX as u16) as i16)
    }
}
