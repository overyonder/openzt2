use bevy::{ecs::system::SystemParam, prelude::*};
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;
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
    runtime_continuation_snapshot_types::RuntimeContinuationSnapshot,
};

#[derive(SystemParam)]
pub(in crate::persistence) struct RuntimeContinuationSnapshotCapture<'w, 's> {
    allocator: Res<'w, PersistentIdAllocator>,
    navigation: Res<'w, NavigationRequestSequence>,
    behavior: Res<'w, BehaviorTaskExecutionSequence>,
    service: Option<Res<'w, ServiceRequestSequence>>,
    arrivals: Option<Res<'w, GuestArrivalState>>,
    survey: Option<Res<'w, GuestSurvey>>,
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
}

impl RuntimeContinuationSnapshotCapture<'_, '_> {
    pub(super) fn capture(
        &self,
        world_root: Entity,
    ) -> Result<RuntimeContinuationSnapshot, Failure> {
        if !self.allocator.owns_world(world_root) {
            return Err(Failure::BrokenPersistentEntityReference);
        }
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
        let record = RuntimeContinuationSnapshot {
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
            next_persistent_identifier: self.allocator.next(),
            navigation_sequence: self.navigation.last_issued(),
            behavior_execution_sequence: self.behavior.last_issued(),
            service_request_sequence: self
                .service
                .as_ref()
                .map(|sequence| sequence.next_sequence()),
            guest_arrivals: self.arrivals.as_deref().copied(),
            guest_survey: self.survey.as_deref().copied(),
            guest_favourite_animal_trend: self.favourite_trends.get(world_root).ok().copied(),
        };
        validate_runtime_continuation_snapshot(&record)?;
        Ok(record)
    }
}
