use bevy::ecs::relationship::RelationshipTarget;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{WorldMember, WorldMembers},
};
use openzt2_zoo_state::animal_behavior::{
    disease_exposure_state::{ActiveZooDisease, DiseaseExposureDeadline, DiseaseResearchFunding},
    egg_incubation::EggIncubation,
};
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::shows::running_show_types::WorldTrickPopularity;
use openzt2_zoo_state::{
    behavior_task_execution_types::BehaviorTaskExecutionSequence,
    economy::service_types::ServiceRequestSequence,
    guests::{
        guest_simulation_types::GuestArrivalState,
        guest_survey_types::{GuestSurvey, GuestSurveyNeedCadence},
    },
    locomotion::locomotion_types::NavigationRequestSequence,
};

use super::{
    super::persistence_failure_types::WorldSnapshotPersistenceFailure as Failure,
    runtime_continuation_snapshot_encoding::validate_runtime_continuation_snapshot,
    runtime_continuation_snapshot_types::{
        DiseaseContinuation, EggContinuation, RuntimeContinuationSnapshot,
        RuntimeDomainContinuation,
    },
};

#[derive(SystemParam)]
pub(in crate::persistence) struct RuntimeContinuationSnapshotCapture<'w, 's> {
    allocator: Query<'w, 's, &'static PersistentIdAllocator, With<WorldRoot>>,
    navigation: Res<'w, NavigationRequestSequence>,
    behavior: Res<'w, BehaviorTaskExecutionSequence>,
    service: Option<Res<'w, ServiceRequestSequence>>,
    arrivals: Query<'w, 's, &'static GuestArrivalState, With<WorldRoot>>,
    survey: Option<Res<'w, GuestSurvey>>,
    survey_cadence: Query<'w, 's, &'static GuestSurveyNeedCadence, With<WorldRoot>>,
    general_economy:
        Option<Res<'w, openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger>>,
    audio_assets:
        Option<Res<'w, Assets<openzt2_game_assets::audio::audio_asset_types::AudioAsset>>>,
    audio_presets: Query<
        'w,
        's,
        &'static openzt2_zoo_state::audio::audio_environment_types::WorldAudioPreset,
        With<WorldRoot>,
    >,
    name_counters: Query<
        'w,
        's,
        &'static openzt2_zoo_state::entity_naming_types::EntityNameTagCounters,
        With<WorldRoot>,
    >,
    names: Query<
        'w,
        's,
        (
            &'static openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
            &'static openzt2_world_state::world_spawn::world_membership_types::WorldMember,
            &'static Name,
        ),
        // Guests have no saved records yet, so their names have no owner on load.
        Without<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
    >,
    named_entities: Query<
        'w,
        's,
        (
            &'static openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
            &'static openzt2_world_state::world_spawn::world_membership_types::WorldMember,
        ),
        (
            With<openzt2_zoo_state::entity_naming_types::EntityNameRegistered>,
            Without<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
        ),
    >,
    favourite_trends: Query<'w, 's, &'static GuestFavouriteAnimalTrend, With<WorldRoot>>,
    domain_roots: Query<
        'w,
        's,
        (
            Option<&'static WorldMembers>,
            Option<&'static DiseaseResearchFunding>,
            Option<&'static DiseaseExposureDeadline>,
            Option<&'static ActiveZooDisease>,
            Option<&'static WorldTrickPopularity>,
        ),
        With<WorldRoot>,
    >,
    eggs: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static WorldMember,
            &'static EggIncubation,
        ),
    >,
    fossils: super::fossil_continuation_snapshot::CaptureFossilContinuation<'w, 's>,
}

impl RuntimeContinuationSnapshotCapture<'_, '_> {
    pub(super) fn capture(
        &self,
        world_root: Entity,
    ) -> Result<RuntimeContinuationSnapshot, Failure> {
        let allocator = self
            .allocator
            .get(world_root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let mut registered_names = self
            .named_entities
            .iter()
            .filter_map(|(id, member)| (member.root == world_root).then_some(id.0))
            .collect::<Vec<_>>();
        registered_names.sort_unstable();
        let mut names = self
            .names
            .iter()
            .filter_map(|(id, member, name)| {
                (member.root == world_root).then(|| (id.0, name.as_str().to_owned()))
            })
            .collect::<Vec<_>>();
        names.sort_unstable_by_key(|(id, _)| *id);
        let (members, funding, exposure_deadline, active, popularity) = self
            .domain_roots
            .get(world_root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let mut eggs = self
            .eggs
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .map(|(id, _, egg)| EggContinuation {
                id: id.0,
                amount: egg.amount,
                remaining_ns: egg.remaining_ns,
                random: egg.random.to_raw(),
            })
            .collect::<Vec<_>>();
        eggs.sort_unstable_by_key(|egg| egg.id);
        let fossils = self.fossils.capture(world_root, members)?;
        let domain_continuation = (funding.is_some()
            || exposure_deadline.is_some()
            || active.is_some()
            || !eggs.is_empty()
            || fossils.is_some()
            || popularity.is_some_and(|popularity| !popularity.0.is_empty()))
        .then(|| RuntimeDomainContinuation {
            disease: DiseaseContinuation {
                funding: funding.map(|funding| funding.0),
                exposure_deadline: exposure_deadline.map(|deadline| deadline.0),
                active: active.map(|active| (active.definition, active.cure_due_tick)),
            },
            eggs,
            fossils,
            trick_popularity: popularity.map(|popularity| {
                popularity
                    .0
                    .iter()
                    .map(|(&trick, &score)| (trick, score))
                    .collect()
            }),
        });
        let record = RuntimeContinuationSnapshot {
            domain_continuation,
            general_economy: self.general_economy.as_deref().copied(),
            entity_names: Some(names),
            world_audio_preset: self
                .audio_presets
                .get(world_root)
                .ok()
                .map(|preset| {
                    self.audio_assets
                        .as_ref()
                        .and_then(|assets| assets.get(&preset.0))
                        .map(|asset| asset.document().clone())
                        .ok_or(Failure::SnapshotTransactionInFlight)
                })
                .transpose()?,
            entity_name_counters: self.name_counters.get(world_root).ok().cloned(),
            registered_entity_name_identifiers: Some(registered_names),
            next_persistent_identifier: allocator.next(),
            navigation_sequence: self.navigation.last_issued(),
            behavior_execution_sequence: self.behavior.last_issued(),
            service_request_sequence: self
                .service
                .as_ref()
                .map(|sequence| sequence.next_sequence()),
            guest_arrivals: self.arrivals.get(world_root).ok().copied(),
            guest_survey: self
                .survey_cadence
                .get(world_root)
                .ok()
                .copied()
                .zip(self.survey.as_deref().copied()),
            guest_favourite_animal_trend: self.favourite_trends.get(world_root).ok().copied(),
        };
        validate_runtime_continuation_snapshot(&record)?;
        Ok(record)
    }
}
