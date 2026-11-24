//! Emits authored view events and delivers them to the guests watching.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{RunBehaviorViewAnimals, RunBehaviorViewEvent, RunningBehaviorInstruction};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
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
    donations::donation_guest_decision_types::GuestDonationMeters,
    economy::service_types::ServiceReservation,
    guests::{
        guest_simulation_types::{
            Guest, GuestArchetype, GuestDestination, GuestEducation, GuestMemories,
            GuestMemoryEntry, GuestNavigationRequest, GuestPhase, GuestReaction, GuestSatisfaction,
            GuestViewers, Viewing,
        },
        guest_survey_types::GuestSurveyExperience,
    },
    locomotion::locomotion_types::{Destination, SpatialGrid, SpatialGridMembership},
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
    pub(crate) education: f32,
    pub(crate) education_donation: f32,
}

pub(super) fn emit_authored_view_events(
    activation: On<WhenBehaviorAction<RunBehaviorViewEvent>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clock: Single<&ZooClock>,
    tasks: Query<
        (
            Entity,
            Option<&DefinitionId>,
            Option<(&WorldMember, &GlobalTransform)>,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorViewEvent,
            ),
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    grids: Query<&SpatialGrid>,
    spatial: SpatialGridMembership,
    mut guests: Query<
        (
            &GuestArchetype,
            &WorldMember,
            &GlobalTransform,
            &mut GuestEducation,
            &mut GuestSurveyExperience,
            Option<&mut GuestDonationMeters>,
        ),
        With<Guest>,
    >,
    mut events: MessageWriter<BehaviorViewEventEmitted>,
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (source, definition, position, (task, instruction, running_action)) in
        std::iter::once(tasks.get(activation.entity)).flatten()
    {
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
        let property = |key: Option<AssetId>| {
            key.and_then(|key| {
                let properties = &definition.authored_runtime.numeric_behavior_properties;
                let index = properties
                    .binary_search_by_key(&key, |&(property, _)| property)
                    .ok()?;
                Some(properties[index].1)
            })
            .unwrap_or_default()
        };
        let education = property(event.education_value_property);
        let education_donation = property(event.education_donation_property);
        if let Some(radius) = event.radius {
            if let Some((member, pose)) = position {
                if let Ok(grid) = grids.get(member.root) {
                    let centre = pose.translation();
                    let minimum = (((centre.xz() - grid.origin) - Vec2::splat(radius.metres))
                        / grid.cell_size_m)
                        .floor()
                        .as_ivec2()
                        .max(IVec2::ZERO);
                    let maximum = (((centre.xz() - grid.origin) + Vec2::splat(radius.metres))
                        / grid.cell_size_m)
                        .floor()
                        .as_ivec2()
                        .min(IVec2::new(grid.width as i32 - 1, grid.height as i32 - 1));
                    for row in minimum.y..=maximum.y {
                        for column in minimum.x..=maximum.x {
                            let cell = row as u32 * grid.width + column as u32;
                            for recipient in spatial.members(member.root, cell) {
                                let Ok((
                                    archetype,
                                    guest_world,
                                    pose,
                                    mut value,
                                    mut experience,
                                    mut donations,
                                )) = guests.get_mut(recipient)
                                else {
                                    continue;
                                };
                                if guest_world.root != member.root
                                    || (pose.translation() - centre).abs().max_element()
                                        > radius.metres
                                    || radius.target_type.is_some_and(|target| {
                                        !definitions.find_guest(archetype.0).is_some_and(|guest| {
                                            guest
                                                .behavior_subject_type_identifiers
                                                .contains(&target)
                                        })
                                    })
                                {
                                    continue;
                                }
                                apply_authored_education(
                                    education,
                                    education_donation,
                                    &mut value,
                                    &mut experience,
                                    donations.as_deref_mut(),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            events.write(BehaviorViewEventEmitted {
                source,
                source_definition: definition.id,
                view_key: event.view_key,
                education,
                education_donation,
            });
        }
        instruction.complete(source, clock.tick, &mut commands);
    }
}

fn apply_authored_education(
    education: f32,
    donation: f32,
    value: &mut GuestEducation,
    experience: &mut GuestSurveyExperience,
    donations: Option<&mut GuestDonationMeters>,
) {
    if education != 0.0 {
        value.0 = apply_permille_delta(value.0, (education * 10.0).round() as i16);
        experience.education_points += education;
    }
    if donation != 0.0 {
        if let Some(donations) = donations {
            *donations
                .0
                .entry(AssetId::from_key("Donate_Education"))
                .or_default() += donation;
        }
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
            Option<&mut GuestDonationMeters>,
        ),
        (With<Guest>, With<GuestPhase>),
    >,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        reactions.clear();
        events.clear();
        return;
    };
    let mut apply = |reaction: GuestReaction, authored_education: f32, authored_donation: f32| {
        let Ok((
            archetype,
            mut satisfaction,
            mut education,
            mut memories,
            mut experience,
            mut donations,
        )) = guests.get_mut(reaction.guest)
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
        apply_authored_education(
            authored_education,
            authored_donation,
            &mut education,
            &mut experience,
            donations.as_deref_mut(),
        );
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
        apply(*reaction, 0.0, 0.0);
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
            if apply(
                GuestReaction {
                    guest,
                    subject: event.source,
                    kind: GuestMemoryKind::AnimalView,
                    satisfaction_delta_permille: 0,
                    education_delta_permille: 0,
                },
                event.education,
                event.education_donation,
            ) {
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
