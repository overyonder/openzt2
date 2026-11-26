use std::collections::HashSet;

use bevy::{
    ecs::{query::QueryData, system::SystemParam},
    prelude::*,
};
use openzt2_animals::{
    animal_behavior::BreedingCenterEndangeredBirths,
    animal_lifecycle::egg_incubation_types::EggIncubationState,
};
use openzt2_game_assets::audio::audio_asset_types::AudioAsset;
use openzt2_world_state::world_spawn::persistent_id_types::{PersistentId, PersistentIdAllocator};
use openzt2_world_state::world_spawn::world_membership_types::{DefinitionId, WorldMember};
use openzt2_world_state::{
    simulation_time::deterministic_random_stream::DeterministicRng,
    world_spawn::persistent_id_index::PersistentIdIndex,
};
use openzt2_zoo_state::animal_behavior::disease_exposure_state::{
    ActiveZooDisease, AssignedDiseaseTarget, DiseaseExposureDeadline, DiseaseResearchFunding,
};
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::shows::running_show_types::WorldTrickPopularity;
use openzt2_zoo_state::{
    audio::audio_environment_types::WorldAudioPreset,
    economy::general_economy_types::GeneralEconomyLedger,
    entity_naming_types::{EntityNameRegistered, EntityNameTagCounters},
};
use openzt2_zoo_state::{
    economy::service_types::ServiceRequestSequence,
    guests::{
        guest_simulation_types::GuestArrivalState,
        guest_survey_types::{GuestSurvey, GuestSurveyNeedCadence},
    },
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

#[derive(QueryData)]
#[query_data(mutable)]
struct RuntimeRootComponents {
    allocator: Option<&'static mut PersistentIdAllocator>,
    service: Option<&'static mut ServiceRequestSequence>,
    general_economy: Option<&'static mut GeneralEconomyLedger>,
    guest_survey: Option<&'static mut GuestSurvey>,
    arrivals: Option<&'static mut GuestArrivalState>,
    survey: Option<&'static mut GuestSurveyNeedCadence>,
    counters: Option<&'static mut EntityNameTagCounters>,
    trend: Option<&'static mut GuestFavouriteAnimalTrend>,
    funding: Option<&'static mut DiseaseResearchFunding>,
    exposure_deadline: Option<&'static mut DiseaseExposureDeadline>,
    disease: Option<&'static mut ActiveZooDisease>,
    trick_popularity: Option<&'static mut WorldTrickPopularity>,
}

#[derive(SystemParam)]
pub(super) struct RuntimeContinuationApplication<'w, 's> {
    roots: Query<'w, 's, RuntimeRootComponents>,
    identities: Option<Res<'w, PersistentIdIndex>>,
    eggs: Query<'w, 's, Option<&'static mut EggIncubationState>, With<WorldMember>>,
    animal_targets: Query<'w, 's, (), With<openzt2_zoo_state::animal_lifecycle::types::Animal>>,
    assigned: Query<'w, 's, (Entity, &'static AssignedDiseaseTarget)>,
    filters: Query<
        'w,
        's,
        (
            &'static openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter,
            Option<
                &'static mut openzt2_aquatic::aquatic::water_filter_timing::FilterCleanupInterval,
            >,
        ),
    >,
    guests: super::guest_continuation_snapshot::GuestContinuationApplication<'w, 's>,
    breeding_centers:
        Query<'w, 's, Option<&'static mut BreedingCenterEndangeredBirths>, With<WorldMember>>,
}

impl RuntimeContinuationApplication<'_, '_> {
    pub(super) fn apply(
        &mut self,
        commands: &mut Commands,
        world_root: Entity,
        record: Option<&RuntimeContinuationSnapshot>,
    ) -> Result<(), Failure> {
        use super::restored_component_values::restore_optional_component;

        let Some(record) = record else {
            if self
                .roots
                .get(world_root)
                .is_ok_and(|root| root.general_economy.is_some())
            {
                commands.entity(world_root).remove::<GeneralEconomyLedger>();
            }
            return Ok(());
        };
        let domain = record.domain_continuation.as_ref();
        if let Some(domain) = domain.filter(|domain| !domain.guests.is_empty()) {
            let identities = self
                .identities
                .as_deref()
                .ok_or(Failure::BrokenPersistentEntityReference)?;
            self.guests
                .apply(commands, world_root, &domain.guests, identities)?;
        }
        let eggs = domain.map_or(&[][..], |domain| domain.eggs.as_slice());
        let live_eggs = eggs
            .iter()
            .map(|egg| {
                self.identities
                    .as_ref()
                    .and_then(|identities| identities.get(world_root, PersistentId(egg.id)))
                    .filter(|entity| self.eggs.contains(*entity))
                    .ok_or(Failure::BrokenPersistentEntityReference)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let resolve = |id| {
            self.identities
                .as_ref()
                .and_then(|identities| identities.get(world_root, PersistentId(id)))
                .ok_or(Failure::BrokenPersistentEntityReference)
        };
        let mut assignments = Vec::new();
        let mut breeding_centers = Vec::new();
        let mut filters = Vec::new();
        if let Some(domain) = domain {
            for &(id, disease) in &domain.disease.assignments {
                let animal = resolve(id)?;
                if !self.animal_targets.contains(animal) {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                assignments.push((animal, disease));
            }
            for &(id, remaining_ns) in &domain.water_filter_cleanup {
                let filter = resolve(id)?;
                if !self.filters.contains(filter) {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                filters.push((filter, remaining_ns));
            }
            for &(id, value) in &domain.breeding_center_births {
                let center = resolve(id)?;
                if !self.breeding_centers.contains(center) {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                breeding_centers.push((center, value));
            }
        }
        let allocator = PersistentIdAllocator::from_snapshot(record.next_persistent_identifier)
            .map_err(|_| Failure::CorruptSnapshotSection)?;
        let live = self
            .roots
            .get_mut(world_root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let mut root = commands.entity(world_root);
        restore_optional_component(&mut root, live.allocator, Some(allocator), false);
        restore_optional_component(&mut root, live.arrivals, record.guest_arrivals, true);
        restore_optional_component(
            &mut root,
            live.survey,
            record.guest_survey.map(|(cadence, _)| cadence),
            true,
        );
        restore_optional_component(
            &mut root,
            live.counters,
            record.entity_name_counters.clone(),
            true,
        );
        restore_optional_component(
            &mut root,
            live.trend,
            record.guest_favourite_animal_trend,
            true,
        );
        restore_optional_component(
            &mut root,
            live.service,
            record
                .service_request_sequence
                .map(ServiceRequestSequence::from_snapshot),
            false,
        );
        restore_optional_component(
            &mut root,
            live.general_economy,
            record.general_economy,
            true,
        );
        restore_optional_component(
            &mut root,
            live.guest_survey,
            record.guest_survey.map(|(_, survey)| survey),
            true,
        );
        let disease = domain.map(|domain| &domain.disease);
        restore_optional_component(
            &mut root,
            live.funding,
            disease
                .and_then(|disease| disease.funding)
                .map(DiseaseResearchFunding),
            true,
        );
        restore_optional_component(
            &mut root,
            live.exposure_deadline,
            disease
                .and_then(|disease| disease.exposure_deadline)
                .map(DiseaseExposureDeadline),
            true,
        );
        restore_optional_component(
            &mut root,
            live.disease,
            disease
                .and_then(|disease| disease.active)
                .map(|(definition, cure_due_tick)| ActiveZooDisease {
                    definition,
                    cure_due_tick,
                }),
            true,
        );
        if let Some(tricks) = domain.and_then(|domain| domain.trick_popularity.as_ref()) {
            let restored = WorldTrickPopularity(
                tricks
                    .iter()
                    .map(|(&trick, &score)| (trick, score))
                    .collect(),
            );
            restore_optional_component(&mut root, live.trick_popularity, Some(restored), false);
        }
        for (entity, egg) in live_eggs.into_iter().zip(eggs) {
            restore_optional_component(
                &mut commands.entity(entity),
                self.eggs.get_mut(entity).ok().flatten(),
                Some(EggIncubationState::from_saved_remaining(
                    egg.amount,
                    egg.remaining_ns,
                    DeterministicRng::from_raw(egg.random),
                )),
                false,
            );
        }
        for (entity, assignment) in &self.assigned {
            if assignment.root == world_root {
                commands.entity(entity).despawn();
            }
        }
        for (animal, disease) in assignments {
            commands.spawn((
                AssignedDiseaseTarget {
                    root: world_root,
                    disease,
                },
                ChildOf(animal),
            ));
        }
        for (entity, remaining_ns) in filters {
            let (filter, timer) = self
                .filters
                .get_mut(entity)
                .map_err(|_| Failure::BrokenPersistentEntityReference)?;
            let restored = openzt2_aquatic::aquatic::water_filter_timing::FilterCleanupInterval::from_saved_remaining(
                filter.policy.cleanup_interval_seconds,
                std::time::Duration::from_nanos(remaining_ns),
            );
            restore_optional_component(&mut commands.entity(entity), timer, Some(restored), false);
        }
        for (entity, value) in breeding_centers {
            restore_optional_component(
                &mut commands.entity(entity),
                self.breeding_centers.get_mut(entity).ok().flatten(),
                Some(BreedingCenterEndangeredBirths(value)),
                false,
            );
        }
        Ok(())
    }
}

pub(super) type SavedEntityNames<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PersistentId,
        &'static WorldMember,
        Option<&'static mut Name>,
    ),
>;

pub(super) fn restore_saved_entity_names(
    commands: &mut Commands,
    members: &mut SavedEntityNames,
    root: Entity,
    names: &[(u64, String)],
) {
    use super::restored_component_values::restore_optional_component;
    for (entity, id, member, live) in members {
        if member.root != root {
            continue;
        }
        let saved = names
            .binary_search_by_key(&id.0, |(id, _)| *id)
            .ok()
            .map(|index| Name::new(names[index].1.clone()));
        restore_optional_component(&mut commands.entity(entity), live, saved, true);
    }
}

pub(super) type SavedNameRegistration<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PersistentId,
        &'static WorldMember,
        Has<DefinitionId>,
        Has<EntityNameRegistered>,
    ),
>;

pub(super) fn restore_entity_name_registration(
    commands: &mut Commands,
    members: &SavedNameRegistration,
    root: Entity,
    registered: Option<&[u64]>,
) {
    for (entity, id, member, has_definition, is_registered) in members {
        if member.root != root || (registered.is_none() && !has_definition) {
            continue;
        }
        let saved = registered.is_none_or(|registered| registered.binary_search(&id.0).is_ok());
        let mut entity = commands.entity(entity);
        match (is_registered, saved) {
            (false, true) => {
                entity.insert(EntityNameRegistered);
            }
            (true, false) => {
                entity.remove::<EntityNameRegistered>();
            }
            _ => {}
        }
    }
}

pub(super) fn restore_world_audio_preset(
    commands: &mut Commands,
    root: Entity,
    mut audio: Option<openzt2_game_data::audio::AudioDocument>,
    current: &mut Query<Option<&mut WorldAudioPreset>>,
    assets: Option<&mut Assets<AudioAsset>>,
) -> Result<(), Failure> {
    use super::restored_component_values::restore_optional_component;
    let live = current
        .get_mut(root)
        .map_err(|_| Failure::BrokenPersistentEntityReference)?;
    let restored = if let Some(document) = &mut audio {
        let assets = assets.ok_or(Failure::UnknownAssetIdentifier)?;
        let soundscape = document
            .soundscape_contents
            .pop()
            .ok_or(Failure::CorruptSnapshotSection)?;
        Some(WorldAudioPreset(assets.add(AudioAsset::from_world_preset(
            soundscape,
            document.crowd,
        ))))
    } else {
        None
    };
    restore_optional_component(&mut commands.entity(root), live, restored, true);
    Ok(())
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
    use bevy::ecs::system::SystemState;
    use openzt2_world_state::{
        simulation_time::deterministic_random_stream::DeterministicRng,
        world_spawn::persistent_id_types::PersistentIdError,
    };

    fn apply_test_continuation(
        world: &mut World,
        root: Entity,
        snapshot: &RuntimeContinuationSnapshot,
        limits: RuntimeContinuationIdentityHighWaterMarks<'_>,
    ) -> Result<(), Failure> {
        validate_runtime_continuation_identity_high_water_marks(snapshot, limits)?;
        let mut state = SystemState::<(
            Commands,
            RuntimeContinuationApplication,
            SavedEntityNames,
            SavedNameRegistration,
            Query<Option<&mut WorldAudioPreset>>,
            Option<ResMut<Assets<AudioAsset>>>,
        )>::new(world);
        let (mut commands, mut runtime, mut names, registration, mut audio, mut assets) =
            state.get_mut(world).unwrap();
        runtime.apply(&mut commands, root, Some(snapshot))?;
        if let Some(saved) = &snapshot.entity_names {
            restore_saved_entity_names(&mut commands, &mut names, root, saved);
        }
        restore_entity_name_registration(
            &mut commands,
            &registration,
            root,
            snapshot.registered_entity_name_identifiers.as_deref(),
        );
        restore_world_audio_preset(
            &mut commands,
            root,
            snapshot.world_audio_preset.clone(),
            &mut audio,
            assets.as_deref_mut(),
        )?;
        state.apply(world);
        Ok(())
    }

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
            guest_survey: Some((
                GuestSurveyNeedCadence(5.0),
                GuestSurvey {
                    previous_day_of_month: 9,
                    current_day_of_month: 1,
                    ..Default::default()
                },
            )),
            guest_favourite_animal_trend: Some(
                GuestFavouriteAnimalTrend::from_saved_state(
                    [Some(openzt2_game_data::AssetId::from_key("first")), None],
                    37,
                    0,
                )
                .unwrap(),
            ),
            domain_continuation: None,
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
        let saved = record();
        apply_test_continuation(&mut world, root, &saved, high_water_marks(&[12, 31])).unwrap();
        let recaptured = {
            let mut state = SystemState::<RuntimeContinuationSnapshotCapture>::new(&mut world);
            let capture = state.get(&world).unwrap();
            assert!(capture.capture(other_root).is_err());
            capture.capture(root).unwrap()
        };
        let mut saved_bytes = Vec::new();
        let mut expected = saved.clone();
        expected.navigation_sequence = 0;
        expected.behavior_execution_sequence = 0;
        append_runtime_continuation_snapshot(&mut saved_bytes, &expected).unwrap();
        let mut recaptured_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut recaptured_bytes, &recaptured).unwrap();
        assert_eq!(recaptured_bytes, saved_bytes);
        assert_eq!(
            world
                .get_mut::<PersistentIdAllocator>(root)
                .unwrap()
                .allocate()
                .unwrap(),
            PersistentId(800)
        );
        assert_eq!(
            world
                .get_mut::<ServiceRequestSequence>(root)
                .unwrap()
                .allocate(),
            Some(700)
        );
        assert_eq!(
            world
                .get_mut::<GuestArrivalState>(root)
                .unwrap()
                .rng
                .next_u32(),
            saved.guest_arrivals.unwrap().rng.next_u32()
        );
        let mut absent = saved;
        absent.service_request_sequence = None;
        absent.guest_arrivals = None;
        absent.guest_survey = None;
        absent.guest_favourite_animal_trend = None;
        apply_test_continuation(&mut world, root, &absent, high_water_marks(&[])).unwrap();
        assert_eq!(
            world
                .get::<ServiceRequestSequence>(root)
                .unwrap()
                .next_sequence(),
            701
        );
        assert!(world.get::<GuestArrivalState>(root).is_none());
        assert!(world.get::<GuestSurvey>(root).is_none());
        assert!(world.get::<GuestFavouriteAnimalTrend>(root).is_none());
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
            openzt2_zoo_state::economy::money_types::Money::from_cents(3_706_800),
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
        apply_test_continuation(&mut world, root, &decoded, high_water_marks(&[])).unwrap();
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
        let mut expected = saved;
        expected.navigation_sequence = 0;
        expected.behavior_execution_sequence = 0;
        let mut expected_bytes = Vec::new();
        append_runtime_continuation_snapshot(&mut expected_bytes, &expected).unwrap();
        assert_eq!(recaptured_bytes, expected_bytes);
        decoded.general_economy = None;
        decoded.world_audio_preset = None;
        decoded.entity_name_counters = None;
        decoded.registered_entity_name_identifiers = None;
        apply_test_continuation(&mut world, root, &decoded, high_water_marks(&[])).unwrap();
        assert!(world
            .get::<openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset>(root)
            .is_none());
        assert!(world
            .get::<openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger>(root)
            .is_none());
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
            PersistentIdAllocator::from_snapshot(0),
            Err(PersistentIdError::Zero)
        );
        apply_test_continuation(
            &mut world,
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
        assert_eq!(
            world
                .get_mut::<PersistentIdAllocator>(root)
                .unwrap()
                .allocate(),
            Err(PersistentIdError::Exhausted)
        );
        assert_eq!(
            world
                .get_mut::<ServiceRequestSequence>(root)
                .unwrap()
                .allocate(),
            None
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
            world
                .entity_mut(root)
                .insert(ServiceRequestSequence::from_snapshot(55));
            assert!(apply_test_continuation(&mut world, root, &saved, invalid).is_err());
            assert_eq!(
                world
                    .get::<ServiceRequestSequence>(root)
                    .unwrap()
                    .next_sequence(),
                55
            );
            assert!(world.get::<PersistentIdAllocator>(root).is_none());
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
