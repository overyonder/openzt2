use bevy::prelude::*;
use openzt2_game_data::world_definitions::guest_simulation_definitions::GuestVisitPurpose;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::locomotion::locomotion_types::Arrived;
use openzt2_zoo_state::locomotion::locomotion_types::NavigateTo;
use openzt2_zoo_state::locomotion::locomotion_types::NavigationRequestGeneration;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::zoo_entrance_anchor_synchronization::ZooEntrance;
use super::guest_arrival_calculations::displaced_from_entrance_gate;
use openzt2_zoo_state::locomotion::locomotion_types::{EntranceNavigationConnection, SpatialGrid};
use super::guest_simulation_calculations::should_leave;
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
use super::guest_simulation_types::GuestNavigationRequest;
use super::guest_simulation_types::GuestPhase;
use super::guest_simulation_types::GuestReachedEntrance;
use super::guest_simulation_types::GuestRestroom;
use super::guest_simulation_types::GuestSatisfaction;
use super::guest_simulation_types::GuestThirst;
use super::guest_simulation_types::GuestViewingTarget;
use super::guest_simulation_types::Viewing;
use super::guest_simulation_types::VisitTime;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::BehaviorTaskExecutionState;
use super::guest_definition_queries::find_guest_definition;
use super::guest_definition_queries::find_reaction;

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
    grid: Single<&SpatialGrid>,
    members: Query<&WorldMember, With<Guest>>,
    mut requests: Query<&mut GuestNavigationRequest, With<Guest>>,
    tasks: Query<(), With<BehaviorTaskExecutionState>>,
    mut commands: Commands,
    mut navigation_generations: Query<&mut NavigationRequestGeneration>,
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
                let Ok(member) = members.get(guest) else {
                    continue;
                };
                departed.write(GuestDeparted {
                    root: member.root,
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
        let exit = asset.guest_generation().map_or(entrance.exit, |policy| {
            displaced_from_entrance_gate(entrance.exit, gate.rotation(), policy.departure_offset_m)
        });
        let Ok(mut generation) = navigation_generations.get_mut(guest) else {
            continue;
        };
        let request_id = generation.advance();
        commands.entity(guest).insert((
            GuestDestination {
                entity: entrance_entity,
                purpose: GuestVisitPurpose::Exit,
            },
            GuestNavigationRequest(Some(request_id)),
        ));
        if grid.width != 0 && grid.height != 0 && grid.cell_of(exit.xz()).is_none() {
            commands.entity(guest).insert(EntranceNavigationConnection {
                request_id,
                gate: entrance.exit,
                inbound: false,
            });
        }
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
