use bevy::ecs::relationship::RelationshipTarget;
use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_animals::{
    animal_behavior::BreedingCenterEndangeredBirths,
    animal_lifecycle::egg_incubation_types::EggIncubationState,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
use openzt2_world_state::world_spawn::{
    persistent_id_types::{PersistentId, PersistentIdAllocator},
    world_membership_types::{WorldMember, WorldMembers},
};
use openzt2_zoo_state::animal_behavior::disease_exposure_state::{
    ActiveZooDisease, AssignedDiseaseTarget, AssignedDiseaseTargets, DiseaseExposureDeadline,
    DiseaseResearchFunding,
};
use openzt2_zoo_state::guests::guest_favourite_animal_trend_types::GuestFavouriteAnimalTrend;
use openzt2_zoo_state::shows::running_show_types::WorldTrickPopularity;
use openzt2_zoo_state::{
    behavior_task_execution_types::BehaviorTaskGeneration,
    economy::service_types::ServiceRequestSequence,
    guests::{
        guest_simulation_types::GuestArrivalState,
        guest_survey_types::{GuestSurvey, GuestSurveyNeedCadence},
    },
    locomotion::locomotion_types::NavigationRequestGeneration,
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
    generations: Query<
        'w,
        's,
        (
            Option<&'static NavigationRequestGeneration>,
            Option<&'static BehaviorTaskGeneration>,
        ),
    >,
    service: Query<'w, 's, &'static ServiceRequestSequence, With<WorldRoot>>,
    arrivals: Query<'w, 's, &'static GuestArrivalState, With<WorldRoot>>,
    survey: Query<'w, 's, &'static GuestSurvey, With<WorldRoot>>,
    survey_cadence: Query<'w, 's, &'static GuestSurveyNeedCadence, With<WorldRoot>>,
    general_economy: Query<
        'w,
        's,
        &'static openzt2_zoo_state::economy::general_economy_types::GeneralEconomyLedger,
        With<WorldRoot>,
    >,
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
        Without<openzt2_zoo_state::guests::guest_simulation_types::Guest>,
    >,
    named_entities: Query<
        'w,
        's,
        (
            &'static openzt2_world_state::world_spawn::persistent_id_types::PersistentId,
            &'static openzt2_world_state::world_spawn::world_membership_types::WorldMember,
        ),
        With<openzt2_zoo_state::entity_naming_types::EntityNameRegistered>,
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
            Option<&'static AssignedDiseaseTargets>,
        ),
        With<WorldRoot>,
    >,
    eggs: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static WorldMember,
            &'static EggIncubationState,
        ),
    >,
    disease_assignments: Query<'w, 's, (&'static AssignedDiseaseTarget, &'static ChildOf)>,
    animal_ids: Query<
        'w,
        's,
        (&'static PersistentId, &'static WorldMember),
        With<openzt2_zoo_state::animal_lifecycle::types::Animal>,
    >,
    breeding_centers: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static BreedingCenterEndangeredBirths,
        ),
    >,
    filters: Query<
        'w,
        's,
        (
            &'static PersistentId,
            &'static openzt2_aquatic::aquatic::water_filter_timing::FilterCleanupInterval,
        ),
    >,
    fossils: super::fossil_continuation_snapshot::CaptureFossilContinuation<'w, 's>,
    guests: super::guest_continuation_snapshot::GuestContinuationCapture<'w, 's>,
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
        let (members, funding, exposure_deadline, active, popularity, assigned) = self
            .domain_roots
            .get(world_root)
            .map_err(|_| Failure::BrokenPersistentEntityReference)?;
        let mut eggs = self
            .eggs
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .map(|(id, _, egg)| {
                let (amount, remaining_ns, random) = egg.saved_remaining();
                EggContinuation {
                    id: id.0,
                    amount,
                    remaining_ns,
                    random,
                }
            })
            .collect::<Vec<_>>();
        eggs.sort_unstable_by_key(|egg| egg.id);
        let mut assignments = Vec::new();
        if let Some(assigned) = assigned {
            for assignment in self.disease_assignments.iter_many(assigned.iter()) {
                let (assignment, parent) =
                    assignment.map_err(|_| Failure::BrokenPersistentEntityReference)?;
                let (id, member) = self
                    .animal_ids
                    .get(parent.parent())
                    .map_err(|_| Failure::BrokenPersistentEntityReference)?;
                if member.root != world_root {
                    return Err(Failure::BrokenPersistentEntityReference);
                }
                assignments.push((id.0, assignment.disease));
            }
        }
        assignments.sort_unstable();
        let mut breeding_center_births = self
            .breeding_centers
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .map(|(id, value)| (id.0, value.0))
            .collect::<Vec<_>>();
        breeding_center_births.sort_unstable_by_key(|(id, _)| *id);
        let mut water_filter_cleanup = self
            .filters
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .map(|(id, timer)| {
                let ns = u64::try_from(timer.remaining().as_nanos())
                    .map_err(|_| Failure::CapacityExceeded)?;
                Ok((id.0, ns))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        water_filter_cleanup.sort_unstable_by_key(|(id, _)| *id);
        let fossils = self.fossils.capture(world_root, members)?;
        let guests = self.guests.capture(members)?;
        let domain_continuation = (funding.is_some()
            || exposure_deadline.is_some()
            || active.is_some()
            || !eggs.is_empty()
            || !assignments.is_empty()
            || !breeding_center_births.is_empty()
            || !water_filter_cleanup.is_empty()
            || !guests.is_empty()
            || fossils.is_some()
            || popularity.is_some_and(|popularity| !popularity.0.is_empty()))
        .then(|| RuntimeDomainContinuation {
            disease: DiseaseContinuation {
                funding: funding.map(|funding| funding.0),
                exposure_deadline: exposure_deadline.map(|deadline| deadline.0),
                active: active.map(|active| (active.definition, active.cure_due_tick)),
                assignments,
            },
            eggs,
            breeding_center_births,
            water_filter_cleanup,
            guests,
            fossils,
            trick_popularity: popularity.map(|popularity| {
                popularity
                    .0
                    .iter()
                    .map(|(&trick, &score)| (trick, score))
                    .collect()
            }),
        });
        let (navigation_sequence, behavior_execution_sequence) = self
            .generations
            .iter_many(members.into_iter().flat_map(RelationshipTarget::iter))
            .flatten()
            .fold(
                (0, 0),
                |(navigation_max, behavior_max), (navigation, behavior)| {
                    (
                        navigation_max
                            .max(navigation.map_or(0, NavigationRequestGeneration::last_issued)),
                        behavior_max.max(behavior.map_or(0, BehaviorTaskGeneration::last_issued)),
                    )
                },
            );
        let record = RuntimeContinuationSnapshot {
            domain_continuation,
            general_economy: self.general_economy.get(world_root).ok().copied(),
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
            navigation_sequence,
            behavior_execution_sequence,
            service_request_sequence: self
                .service
                .get(world_root)
                .ok()
                .map(|sequence| sequence.next_sequence()),
            guest_arrivals: self.arrivals.get(world_root).ok().copied(),
            guest_survey: self
                .survey_cadence
                .get(world_root)
                .ok()
                .copied()
                .zip(self.survey.get(world_root).ok().copied()),
            guest_favourite_animal_trend: self.favourite_trends.get(world_root).ok().copied(),
        };
        validate_runtime_continuation_snapshot(&record)?;
        Ok(record)
    }
}
