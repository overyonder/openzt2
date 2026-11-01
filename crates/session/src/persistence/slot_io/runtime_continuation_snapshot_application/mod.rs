use std::collections::HashSet;

use bevy::prelude::*;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::{
    behavior_task_execution_types::BehaviorTaskExecutionSequence,
    economy::service_types::ServiceRequestSequence,
    guests::{guest_simulation_types::GuestArrivalState, guest_survey_types::GuestSurvey},
    locomotion::locomotion_types::NavigationRequestSequence,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    runtime_continuation_snapshot_encoding::validate_runtime_continuation_snapshot,
    runtime_continuation_snapshot_types::{
        RuntimeContinuationIdentityHighWaterMarks, RuntimeContinuationSnapshot,
    },
};

pub(super) fn validate_runtime_continuation_identity_high_water_marks(
    record: &RuntimeContinuationSnapshot,
    high_water_marks: RuntimeContinuationIdentityHighWaterMarks<'_>,
) -> Result<(), Failure> {
    validate_runtime_continuation_snapshot(record)?;
    if record.next_persistent_identifier <= high_water_marks.maximum_persistent_identifier
        || record.navigation_sequence < high_water_marks.maximum_navigation_request
        || record.behavior_execution_sequence < high_water_marks.maximum_behavior_execution
    {
        warn!(
            next_persistent_identifier = record.next_persistent_identifier,
            maximum_persistent_identifier = high_water_marks.maximum_persistent_identifier,
            navigation_sequence = record.navigation_sequence,
            maximum_navigation_request = high_water_marks.maximum_navigation_request,
            behavior_execution_sequence = record.behavior_execution_sequence,
            maximum_behavior_execution = high_water_marks.maximum_behavior_execution,
            "saved continuation counters do not cover restored identities"
        );
        return Err(Failure::BrokenPersistentEntityReference);
    }
    let mut service_requests = HashSet::new();
    for sequence in high_water_marks.pending_service_request_sequences {
        if record
            .service_request_sequence
            .is_none_or(|next| *sequence >= next)
            || !service_requests.insert(*sequence)
        {
            return Err(Failure::BrokenPersistentEntityReference);
        }
    }
    Ok(())
}

/// Queue this last on the same Commands as restored entities and buffered results.
pub(super) fn apply_runtime_continuation_snapshot(
    commands: &mut Commands,
    world_root: Entity,
    record: &RuntimeContinuationSnapshot,
    high_water_marks: RuntimeContinuationIdentityHighWaterMarks<'_>,
) -> Result<(), Failure> {
    validate_runtime_continuation_identity_high_water_marks(record, high_water_marks)?;
    let allocator =
        PersistentIdAllocator::from_snapshot(world_root, record.next_persistent_identifier)
            .map_err(|_| Failure::CorruptSnapshotSection)?;
    if commands.get_entity(world_root).is_err() {
        return Err(Failure::BrokenPersistentEntityReference);
    }
    commands.insert_resource(allocator);
    commands.insert_resource(NavigationRequestSequence::from_snapshot(
        record.navigation_sequence,
    ));
    commands.insert_resource(BehaviorTaskExecutionSequence::from_snapshot(
        record.behavior_execution_sequence,
    ));
    apply_optional_resource(
        commands,
        record
            .service_request_sequence
            .map(ServiceRequestSequence::from_snapshot),
    );
    apply_optional_resource(commands, record.general_economy);
    apply_optional_resource::<GuestArrivalState>(commands, record.guest_arrivals);
    apply_optional_resource::<GuestSurvey>(commands, record.guest_survey);
    if let Some(counters) = &record.entity_name_counters {
        commands.entity(world_root).insert(counters.clone());
    } else {
        commands
            .entity(world_root)
            .remove::<openzt2_zoo_state::entity_naming_types::EntityNameTagCounters>();
    }
    apply_entity_name_registration(
        commands,
        world_root,
        record.registered_entity_name_identifiers.as_deref(),
    );
    if let Some(names) = record.entity_names.clone() {
        commands.queue(move |world: &mut World| {
            use openzt2_world_state::world_spawn::{
                persistent_id_types::PersistentId, world_membership_types::WorldMember,
            };
            let entities = world
                .query::<(Entity, &PersistentId, &WorldMember)>()
                .iter(world)
                .filter(|(_, _, member)| member.root == world_root)
                .map(|(entity, id, _)| (entity, id.0))
                .collect::<Vec<_>>();
            for (entity, id) in entities {
                if let Ok(index) = names.binary_search_by_key(&id, |(id, _)| *id) {
                    world
                        .entity_mut(entity)
                        .insert(Name::new(names[index].1.clone()));
                } else {
                    world.entity_mut(entity).remove::<Name>();
                }
            }
        });
    }
    let audio = record.world_audio_preset.clone();
    commands.queue(move |world: &mut World| {
        use openzt2_game_assets::audio::audio_asset_types::AudioAsset;
        use openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset;
        if let Some(mut document) = audio {
            let asset = AudioAsset::from_world_preset(
                document.soundscape_contents.remove(0),
                document.crowd,
            );
            let handle = world.resource_mut::<Assets<AudioAsset>>().add(asset);
            world
                .entity_mut(world_root)
                .insert(WorldAudioPreset(handle));
        } else {
            world.entity_mut(world_root).remove::<WorldAudioPreset>();
        }
    });
    if let Some(trend) = record.guest_favourite_animal_trend {
        commands.entity(world_root).insert(trend);
    } else {
        commands
            .entity(world_root)
            .remove::<GuestFavouriteAnimalTrend>();
    }
    Ok(())
}

pub(super) fn apply_entity_name_registration(
    commands: &mut Commands,
    world_root: Entity,
    registered: Option<&[u64]>,
) {
    let registered = registered.map(<[u64]>::to_vec);
    commands.queue(move |world: &mut World| {
        use openzt2_world_state::world_spawn::{
            persistent_id_types::PersistentId,
            world_membership_types::{DefinitionId, WorldMember},
        };
        use openzt2_zoo_state::entity_naming_types::EntityNameRegistered;
        let entities = world
            .query::<(Entity, &PersistentId, &WorldMember, Option<&DefinitionId>)>()
            .iter(world)
            .filter(|(_, _, member, definition)| {
                member.root == world_root && (registered.is_some() || definition.is_some())
            })
            .map(|(entity, id, _, _)| {
                (
                    entity,
                    registered
                        .as_ref()
                        .is_none_or(|ids| ids.binary_search(&id.0).is_ok()),
                )
            })
            .collect::<Vec<_>>();
        for (entity, present) in entities {
            if present {
                world.entity_mut(entity).insert(EntityNameRegistered);
            } else {
                world.entity_mut(entity).remove::<EntityNameRegistered>();
            }
        }
    });
}

fn apply_optional_resource<T: Resource>(commands: &mut Commands, resource: Option<T>) {
    if let Some(resource) = resource {
        commands.insert_resource(resource);
    } else {
        commands.remove_resource::<T>();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        runtime_continuation_snapshot_capture::RuntimeContinuationSnapshotCapture,
        runtime_continuation_snapshot_encoding::{
            append_runtime_continuation_snapshot, decode_runtime_continuation_snapshot,
        },
    };
    use super::*;
    use bevy::ecs::{system::SystemState, world::CommandQueue};
    use openzt2_world_state::{
        simulation_time::deterministic_random_stream::DeterministicRng,
        world_spawn::persistent_id_types::{PersistentId, PersistentIdError},
    };

    fn high_water_marks<'a>(
        pending_service_request_sequences: &'a [u64],
    ) -> RuntimeContinuationIdentityHighWaterMarks<'a> {
        RuntimeContinuationIdentityHighWaterMarks {
            maximum_persistent_identifier: 42,
            maximum_navigation_request: 14,
            maximum_behavior_execution: 28,
            pending_service_request_sequences,
        }
    }

    fn record() -> RuntimeContinuationSnapshot {
        RuntimeContinuationSnapshot {
            general_economy: None,
            entity_names: Some(Vec::new()),
            world_audio_preset: None,
            entity_name_counters: None,
            registered_entity_name_identifiers: Some(Vec::new()),
            next_persistent_identifier: 800,
            navigation_sequence: 900,
            behavior_execution_sequence: 1000,
            service_request_sequence: Some(700),
            guest_arrivals: Some(GuestArrivalState {
                next_tick: 444,
                rng: DeterministicRng::from_raw([1234, 789]),
            }),
            guest_survey: Some(GuestSurvey {
                critical_need_countdown_seconds: 5.0,
                previous_day_of_month: 9,
                current_day_of_month: 1,
                ..Default::default()
            }),
            guest_favourite_animal_trend: Some(
                GuestFavouriteAnimalTrend::from_saved_state(
                    [Some(openzt2_game_data::AssetId::from_key("first")), None],
                    37,
                    0,
                )
                .unwrap(),
            ),
        }
    }

    #[test]
    fn exact_resource_continuation_preserves_gaps_random_draw_and_absence() {
        let mut world = World::new();
        let root = world
            .spawn(
                openzt2_world_state::world_spawn::world_membership_types::WorldRoot {
                    scenario: openzt2_game_data::AssetId::from_key("scenario"),
                },
            )
            .id();
        let other_root = world.spawn_empty().id();
        let mut queue = CommandQueue::default();
        let saved = record();
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &saved,
            high_water_marks(&[12, 31]),
        )
        .unwrap();
        queue.apply(&mut world);
        let recaptured = {
            let mut state = SystemState::<RuntimeContinuationSnapshotCapture>::new(&mut world);
            let capture = state.get(&world).unwrap();
            assert!(capture.capture(other_root).is_err());
            capture.capture(root).unwrap()
        };
        let mut saved_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut saved_bytes, &saved).unwrap();
        let mut recaptured_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut recaptured_bytes, &recaptured).unwrap();
        assert_eq!(recaptured_bytes, saved_bytes);
        assert_eq!(
            world
                .resource_mut::<PersistentIdAllocator>()
                .allocate(root)
                .unwrap(),
            PersistentId(800)
        );
        assert_eq!(
            world.resource_mut::<NavigationRequestSequence>().next(),
            901
        );
        assert_eq!(
            world.resource_mut::<BehaviorTaskExecutionSequence>().next(),
            1001
        );
        assert_eq!(
            world.resource_mut::<ServiceRequestSequence>().allocate(),
            Some(700)
        );
        assert_eq!(
            world.resource_mut::<GuestArrivalState>().rng.next_u32(),
            saved.guest_arrivals.unwrap().rng.next_u32()
        );
        let mut absent = saved;
        absent.service_request_sequence = None;
        absent.guest_arrivals = None;
        absent.guest_survey = None;
        absent.guest_favourite_animal_trend = None;
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &absent,
            high_water_marks(&[]),
        )
        .unwrap();
        queue.apply(&mut world);
        assert!(!world.contains_resource::<ServiceRequestSequence>());
        assert!(!world.contains_resource::<GuestArrivalState>());
        assert!(!world.contains_resource::<GuestSurvey>());
        assert!(world.get::<GuestFavouriteAnimalTrend>(root).is_none());
        assert_eq!(
            world.resource::<NavigationRequestSequence>().last_issued(),
            900
        );
    }

    #[test]
    fn historical_name_counts_and_registration_presence_survive_serialized_application() {
        use openzt2_game_data::AssetId;
        use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
        use openzt2_zoo_state::entity_naming_types::{EntityNameRegistered, EntityNameTagCounters};
        let tag = AssetId::from_key("tree");
        let mut saved = record();
        let mut general_history =
            openzt2_zoo_state::economy::monthly_finance_types::MonthlyFinanceHistory::default();
        general_history.enter_calendar_month(
            12,
            openzt2_zoo_state::economy::money_types::Money(3_706_800),
        );
        saved.general_economy = Some(
            openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger::from_saved(
                general_history,
            )
            .unwrap(),
        );
        saved.entity_names = Some(vec![(7, "Front Gate".into())]);
        saved.world_audio_preset = Some(openzt2_game_data::audio::AudioDocument {
            soundscape_contents: vec![openzt2_game_data::audio::AudioSoundscapeContent {
                biome: tag,
                name: "grassland".into(),
                loop_cue: Some(AssetId::from_key("loop")),
                cues: vec![openzt2_game_data::audio::AudioSoundscapeCue {
                    cue: AssetId::from_key("animal"),
                    weight: 1.0,
                    day_mask: 3,
                }],
            }],
            crowd: Some(openzt2_game_data::audio::AudioCrowdPolicy {
                thresholds: [-1, 15, 50, 60, 100, 160, 170, 170],
                cues: [Some(AssetId::from_key("quiet")), None, None, None, None],
            }),
            ..Default::default()
        });
        saved.entity_name_counters =
            Some(EntityNameTagCounters::from_saved_entries([(tag, 85)]).unwrap());
        saved.registered_entity_name_identifiers = Some(vec![7]);
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &saved).unwrap();
        let mut decoded = decode_runtime_continuation_snapshot(&bytes).unwrap();
        let mut world = World::new();
        world.init_resource::<Assets<openzt2_game_assets::audio::audio_asset_types::AudioAsset>>();
        let root = world
            .spawn(
                openzt2_world_state::world_spawn::world_membership_types::WorldRoot {
                    scenario: tag,
                },
            )
            .id();
        let counted = world
            .spawn((PersistentId(7), WorldMember { root }, DefinitionId(tag)))
            .id();
        let fresh = world
            .spawn((
                PersistentId(8),
                WorldMember { root },
                DefinitionId(tag),
                EntityNameRegistered,
            ))
            .id();
        let other_root = world.spawn_empty().id();
        let other = world
            .spawn((
                PersistentId(9),
                WorldMember { root: other_root },
                DefinitionId(tag),
            ))
            .id();
        let mut queue = CommandQueue::default();
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &decoded,
            high_water_marks(&[]),
        )
        .unwrap();
        queue.apply(&mut world);
        assert_eq!(
            world.get::<EntityNameTagCounters>(root).unwrap().value(tag),
            Some(85)
        );
        assert_eq!(world.get::<Name>(counted).unwrap().as_str(), "Front Gate");
        assert!(world.get::<EntityNameRegistered>(counted).is_some());
        assert!(world.get::<EntityNameRegistered>(fresh).is_none());
        assert!(world.get::<EntityNameRegistered>(other).is_none());
        let recaptured = {
            let mut state = SystemState::<RuntimeContinuationSnapshotCapture>::new(&mut world);
            state.get(&world).unwrap().capture(root).unwrap()
        };
        let mut recaptured_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut recaptured_bytes, &recaptured).unwrap();
        assert_eq!(recaptured_bytes, bytes);
        decoded.general_economy = None;
        decoded.world_audio_preset = None;
        decoded.entity_name_counters = None;
        decoded.registered_entity_name_identifiers = None;
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &decoded,
            high_water_marks(&[]),
        )
        .unwrap();
        queue.apply(&mut world);
        assert!(world
            .get::<openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset>(root)
            .is_none());
        assert!(!world.contains_resource::<openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger>());
        assert!(world.get::<EntityNameTagCounters>(root).is_none());
        assert!(world.get::<EntityNameRegistered>(fresh).is_some());
        assert!(world.get::<EntityNameRegistered>(other).is_none());
    }

    #[test]
    fn exhausted_counters_restore_without_reissuing_old_identities() {
        let mut saved = record();
        saved.next_persistent_identifier = u64::MAX;
        saved.navigation_sequence = u64::MAX;
        saved.behavior_execution_sequence = u64::MAX;
        saved.service_request_sequence = Some(u64::MAX);
        let mut bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut bytes, &saved).unwrap();
        let decoded = decode_runtime_continuation_snapshot(&bytes).unwrap();
        let mut world = World::new();
        let root = world.spawn_empty().id();
        assert_eq!(
            PersistentIdAllocator::from_snapshot(root, 0),
            Err(PersistentIdError::Zero)
        );
        let mut queue = CommandQueue::default();
        apply_runtime_continuation_snapshot(
            &mut Commands::new(&mut queue, &world),
            root,
            &decoded,
            RuntimeContinuationIdentityHighWaterMarks {
                maximum_persistent_identifier: u64::MAX - 1,
                maximum_navigation_request: u64::MAX,
                maximum_behavior_execution: u64::MAX,
                pending_service_request_sequences: &[u64::MAX - 1],
            },
        )
        .unwrap();
        queue.apply(&mut world);
        assert_eq!(
            world.resource_mut::<PersistentIdAllocator>().allocate(root),
            Err(PersistentIdError::Exhausted)
        );
        assert_eq!(
            world.resource_mut::<ServiceRequestSequence>().allocate(),
            None
        );
        assert_eq!(
            world.resource::<NavigationRequestSequence>().last_issued(),
            u64::MAX
        );
        assert_eq!(
            world
                .resource::<BehaviorTaskExecutionSequence>()
                .last_issued(),
            u64::MAX
        );
        let mut navigation = NavigationRequestSequence::from_snapshot(u64::MAX - 1);
        let mut behavior = BehaviorTaskExecutionSequence::from_snapshot(u64::MAX - 1);
        assert_eq!(navigation.next(), u64::MAX);
        assert_eq!(behavior.next(), u64::MAX);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| navigation.next())).is_err()
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| behavior.next())).is_err()
        );
    }

    #[test]
    fn inconsistent_highwaters_reject_before_queuing_resource_replacements() {
        let saved = record();
        let mut invalid = high_water_marks(&[12]);
        for field in 0..5 {
            invalid.maximum_persistent_identifier = if field == 0 { 800 } else { 42 };
            invalid.maximum_navigation_request = if field == 1 { 901 } else { 14 };
            invalid.maximum_behavior_execution = if field == 2 { 1001 } else { 28 };
            invalid.pending_service_request_sequences = if field == 3 {
                &[700]
            } else if field == 4 {
                &[12, 12]
            } else {
                &[12]
            };
            let mut world = World::new();
            let root = world.spawn_empty().id();
            world.insert_resource(NavigationRequestSequence::from_snapshot(55));
            let mut queue = CommandQueue::default();
            assert!(apply_runtime_continuation_snapshot(
                &mut Commands::new(&mut queue, &world),
                root,
                &saved,
                invalid
            )
            .is_err());
            queue.apply(&mut world);
            assert_eq!(
                world.resource::<NavigationRequestSequence>().last_issued(),
                55
            );
            assert!(!world.contains_resource::<PersistentIdAllocator>());
        }
        let mut absent = saved;
        absent.service_request_sequence = None;
        assert!(validate_runtime_continuation_identity_high_water_marks(
            &absent,
            high_water_marks(&[0])
        )
        .is_err());
    }
}
