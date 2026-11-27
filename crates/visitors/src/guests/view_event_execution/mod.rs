//! Emits authored view events and delivers them to the guests watching.

use openzt2_zoo_state::behavior_task_execution_types::running_actions::WhenBehaviorAction;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::{
    AuthoredViewThought, RunBehaviorViewAnimals, RunBehaviorViewEvent,
    RunningBehaviorInstruction,
};
use openzt2_shows::shows::ShowAudienceFeedback;
use openzt2_game_assets::localization::{
    loaded_localization_queries::LoadedLocalizationView,
    localization_asset_types::LocalizationAsset, localization_precedence_index::LocalizationPrecedenceIndex,
};
use openzt2_game_data::{behavior::action::feedback::{BehaviorFeedbackAction, BehaviorFeedbackEntry}, localization::LocalizationFormatArgument};
use openzt2_interface_state::information::zoo_message_types::{ZooMessage, ZooMessages};
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::mark_behavior_task_for_failure_and_stop_navigation;
use openzt2_zoo_state::behavior_task_execution_types::{BehaviorFeedbackDispatched, BehaviorTaskExecutionState};
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
    locomotion::locomotion_types::Destination,
};

use openzt2_world_state::world_spawn::world_entity_crating::UncratedWorldEntity;
use openzt2_zoo_state::{
    animal_behavior::behavior_spatial_candidates::BehaviorSpatialCandidates,
    authored_float_attributes::AuthoredFloatAttributes, staff::staff_employment_types::StaffRole,
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
    pub(crate) feedback: Option<BehaviorFeedbackAction>,
    pub(crate) source_target: Option<Entity>,
    pub(crate) source_object: Option<Entity>,
}

#[derive(Component)]
pub(super) struct ViewThoughtTimeout(Timer);

pub(super) fn expire_view_thoughts(
    time: Res<Time<Fixed>>,
    mut thoughts: Query<(Entity, &mut ViewThoughtTimeout)>,
    mut commands: Commands,
) {
    for (thought, mut timeout) in &mut thoughts {
        if timeout.0.tick(time.delta()).is_finished() {
            commands.entity(thought).despawn();
        }
    }
}

pub(super) fn deliver_view_thoughts<'a>(
    commands: &mut Commands,
    guest: Entity,
    source: Entity,
    target: Option<Entity>,
    object: Option<Entity>,
    entries: impl IntoIterator<Item = &'a BehaviorFeedbackEntry>,
) {
    for entry in entries {
        let BehaviorFeedbackEntry::Thought {
            localization_key,
            priority,
            timeout_seconds,
            globally_visible,
            use_entity_name,
            use_target_name,
            use_target_target,
            use_target_object,
            use_target_species,
        } = entry
        else {
            continue;
        };
        let mut thought = commands.spawn((
            ChildOf(guest),
            AuthoredViewThought {
                localization_key: AssetId::from_key(&localization_key.to_ascii_lowercase()),
                priority: priority.unwrap_or_default(),
                globally_visible: globally_visible.unwrap_or_default(),
                source,
                arguments: [
                    use_entity_name.then_some(guest),
                    use_target_name.then_some(source),
                    target.filter(|_| *use_target_target),
                    object.filter(|_| *use_target_object),
                    use_target_species.then_some(source),
                ],
            },
        ));
        if let Some(seconds) = timeout_seconds {
            thought.insert(ViewThoughtTimeout(Timer::from_seconds(
                f32::from(*seconds),
                TimerMode::Once,
            )));
        }
    }
}

pub(super) fn emit_authored_view_events(
    activation: On<WhenBehaviorAction<RunBehaviorViewEvent>>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    world_definitions: Res<WorldDefinitions>,
    clocks: Query<&ZooClock>,
    tasks: Query<
        (
            Entity,
            Option<&DefinitionId>,
            Option<&StaffRole>,
            Option<&AuthoredFloatAttributes>,
            &WorldMember,
            Option<&GlobalTransform>,
            (
                &BehaviorTaskExecutionState,
                &RunningBehaviorInstruction,
                &RunBehaviorViewEvent,
            ),
        ),
        Without<PendingBehaviorTaskFailure>,
    >,
    spatial: BehaviorSpatialCandidates,
    recipients: Query<
        (
            &WorldMember,
            &GlobalTransform,
            Option<&DefinitionId>,
            Option<&GuestArchetype>,
        ),
        UncratedWorldEntity,
    >,
    mut guests: Query<
        (
            &mut GuestEducation,
            &mut GuestSurveyExperience,
            Option<&mut GuestDonationMeters>,
        ),
        With<Guest>,
    >,
    mut events: MessageWriter<BehaviorViewEventEmitted>,
    mut feedback: MessageWriter<BehaviorFeedbackDispatched>,
    (feedback_assets, localization, names, mut zoo_messages, real_time): (
        Option<Res<Assets<LocalizationAsset>>>,
        Option<Res<LocalizationPrecedenceIndex>>,
        Query<&Name>,
        Query<&mut ZooMessages>,
        Res<Time<Real>>,
    ),
    mut commands: Commands,
) {
    let Some(definitions) = world_definitions.get(&world_assets) else {
        return;
    };
    for (
        source,
        definition,
        staff,
        attributes,
        member,
        pose,
        (task, instruction, running_action),
    ) in std::iter::once(tasks.get(activation.entity)).flatten()
    {
        let Ok(clock) = clocks.get(member.root) else {
            continue;
        };
        if task.next_action_tick > clock.tick {
            continue;
        }
        if !instruction.is_current(task) {
            continue;
        }
        let event = &running_action.0;
        let source_definitions = [
            definition.and_then(|id| definitions.find_object(id.0)),
            staff.and_then(|role| definitions.find_object(role.0)),
        ];
        let definition = source_definitions.into_iter().flatten().find(|object| {
            object
                .view_data
                .iter()
                .any(|row| row.name == event.view_key)
        });
        let Some(definition) = definition else {
            mark_behavior_task_for_failure_and_stop_navigation(source, &mut commands);
            continue;
        };
        let property = |key: Option<AssetId>| {
            key.and_then(|key| {
                attributes.and_then(|values| values.get(key)).or_else(|| {
                    source_definitions.into_iter().flatten().find_map(|object| {
                        let properties = &object.authored_runtime.numeric_behavior_properties;
                        let index = properties
                            .binary_search_by_key(&key, |&(property, _)| property)
                            .ok()?;
                        Some(properties[index].1)
                    })
                })
            })
            .unwrap_or_default()
        };
        let education = property(event.education_value_property);
        let education_donation = property(event.education_donation_property);
        if let Some(radius) = event.radius {
            if let Some(pose) = pose {
                let centre = pose.translation();
                let extent = Vec2::splat(radius.metres);
                let mut nearby = Vec::new();
                spatial.in_bounds(
                    member.root,
                    centre.xz() - extent,
                    centre.xz() + extent,
                    &mut nearby,
                );
                for recipient in nearby {
                    let Ok((recipient_world, pose, definition, archetype)) =
                        recipients.get(recipient)
                    else {
                        continue;
                    };
                    if recipient_world.root != member.root
                        || (pose.translation() - centre).abs().max_element() > radius.metres
                        || radius.target_type.is_some_and(|target| {
                            let guest = archetype
                                .and_then(|archetype| definitions.find_guest(archetype.0))
                                .is_some_and(|guest| {
                                    guest.behavior_subject_type_identifiers.contains(&target)
                                });
                            let object = definition
                                .and_then(|definition| definitions.find_object(definition.0))
                                .is_some_and(|object| {
                                    object.id == target
                                        || object.behavior_type_identifiers.contains(&target)
                                });
                            !(guest || object)
                        })
                    {
                        continue;
                    }
                    if let Ok((mut value, mut experience, mut donations)) =
                        guests.get_mut(recipient)
                    {
                        apply_authored_education(
                            education,
                            education_donation,
                            &mut value,
                            &mut experience,
                            donations.as_deref_mut(),
                        );
                        deliver_view_thoughts(
                            &mut commands,
                            recipient,
                            source,
                            task.target,
                            task.object,
                            event.feedback.iter().flat_map(|feedback| &feedback.entries),
                        );
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
                feedback: event.feedback.clone(),
                source_target: task.target,
                source_object: task.object,
            });
        }
        if let Some(data) = &event.feedback {
            if let Some((mut messages, localization)) = zoo_messages.get_mut(member.root).ok().zip(
                localization
                    .as_ref()
                    .zip(feedback_assets.as_ref())
                    .and_then(|(index, assets)| index.borrow_loaded_localization_view(assets)),
            ) {
                deliver_view_messages(
                    &data.entries,
                    source,
                    task.target,
                    &names,
                    localization,
                    &mut messages,
                    real_time.elapsed_secs_f64(),
                );
            }
            feedback.write(BehaviorFeedbackDispatched {
                actor: source,
                document: task.document.clone(),
                declaration: task.declaration,
                phase: task.phase,
                action: task.action,
            });
        }
        instruction.complete(source, clock.tick, &mut commands);
    }
}

fn deliver_view_messages(
    entries: &[BehaviorFeedbackEntry],
    source: Entity,
    target: Option<Entity>,
    names: &Query<&Name>,
    localization: LoadedLocalizationView<'_>,
    messages: &mut ZooMessages,
    now: f64,
) {
    for entry in entries {
        let BehaviorFeedbackEntry::Message {
            localization_key,
            timeout_seconds,
            globally_visible: Some(true),
            use_entity_name,
            use_target_name,
            ..
        } = entry
        else {
            continue;
        };
        let arguments = [
            use_entity_name.then_some(source),
            target.filter(|_| *use_target_name),
        ]
        .into_iter()
        .flatten()
        .map(|entity| {
            LocalizationFormatArgument::Text(names.get(entity).map_or("", |name| name.as_str()))
        })
        .collect::<Vec<_>>();
        let mut text = String::new();
        if localization
            .write_localized_text_with_format_arguments(
                AssetId::from_key(&localization_key.to_ascii_lowercase()),
                &arguments,
                &mut text,
            )
            .is_ok()
        {
            messages.messages.push(ZooMessage {
                text,
                expires_at_seconds: now + f64::from(timeout_seconds.unwrap_or(10)),
            });
        }
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
                .entry(AssetId::from_key("donate_education"))
                .or_default() += donation;
        }
    }
}

#[derive(Message)]
pub(crate) struct AppliedAnimalView {
    pub(crate) guest: Entity,
    pub(crate) subject: Entity,
    pub(crate) view_score: f32,
    pub(crate) entertainment_score: f32,
    pub(crate) donation_score: f32,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(crate) fn apply_guest_reactions(
    mut events: MessageReader<BehaviorViewEventEmitted>,
    mut reactions: MessageReader<GuestReaction>,
    mut applied_views: MessageWriter<AppliedAnimalView>,
    clocks: Query<&ZooClock>,
    mut commands: Commands,
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
        let Ok((member, _)) = members.get(reaction.guest) else {
            return false;
        };
        let Ok(clock) = clocks.get(member.root) else {
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
                created_tick: clock.tick,
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
                deliver_view_thoughts(
                    &mut commands,
                    guest,
                    event.source,
                    event.source_target,
                    event.source_object,
                    event.feedback.iter().flat_map(|feedback| &feedback.entries),
                );
                applied_views.write(AppliedAnimalView {
                    guest,
                    subject: event.source,
                    view_score: row.view_score,
                    entertainment_score: row.entertainment_score,
                    donation_score: row.donation_score,
                });
            }
        }
    }
}

pub(super) fn present_show_audience_feedback(
    mut feedback: MessageReader<ShowAudienceFeedback>,
    guests: Query<&WorldMember, With<Guest>>,
    members: Query<&WorldMember>,
    assets: Option<Res<Assets<LocalizationAsset>>>,
    localization: Option<Res<LocalizationPrecedenceIndex>>,
    names: Query<&Name>,
    mut messages: Query<&mut ZooMessages>,
    time: Res<Time<Real>>,
    mut commands: Commands,
) {
    let localization = localization
        .as_deref()
        .zip(assets.as_deref())
        .and_then(|(index, assets)| index.borrow_loaded_localization_view(assets));
    for feedback in feedback.read() {
        let Ok(guest) = guests.get(feedback.guest) else {
            continue;
        };
        if !members
            .get(feedback.stage)
            .is_ok_and(|stage| stage.root == guest.root)
        {
            continue;
        }
        deliver_view_thoughts(
            &mut commands,
            feedback.guest,
            feedback.stage,
            None,
            None,
            feedback.entries.iter(),
        );
        if let Some((localization, mut messages)) =
            localization.zip(messages.get_mut(guest.root).ok())
        {
            deliver_view_messages(
                &feedback.entries,
                feedback.guest,
                Some(feedback.stage),
                &names,
                localization,
                &mut messages,
                time.elapsed_secs_f64(),
            );
        }
    }
}
