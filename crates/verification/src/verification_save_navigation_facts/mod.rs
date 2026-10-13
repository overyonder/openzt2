use bevy::{ecs::system::SystemParam, prelude::*};

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;

use openzt2_session::persistence::{
    persistence_failure_types::WorldSnapshotPersistenceFailed,
    persistence_filesystem_paths::PersistenceFilesystemPaths,
    persistence_ui_types::OpenLoadSlotCatalogueAfterWorldSnapshotSave,
};

use openzt2_animals::animal_lifecycle::adoption_offer_inventory_types::AnimalAdoptionOfferInventory;
use openzt2_interface_state::persistence::profile_types::ProfileIndex;
use openzt2_interface_state::persistence::profile_types::ProfileOptions;
use openzt2_interface_state::persistence::save_slot_types::LoadWorldSnapshotFromSlot;
use openzt2_interface_state::persistence::save_slot_types::SaveSlotCatalogue;
use openzt2_interface_state::persistence::save_slot_types::SaveWorldSnapshotToSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotLoadedFromSlot;
use openzt2_interface_state::persistence::save_slot_types::WorldSnapshotSavedToSlot;
use openzt2_interface_state::photos::photo_album_types::DefaultPhotoAlbumStorageInitializedForWorld;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengesInitializedForScenarioRevision;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    verification_journey_report::record_verification_failure,
    verification_journey_state_types::{
        VerificationFactCollectionState, VerificationJourneyReport, VerificationJourneySession,
        VerificationSessionStage,
    },
};

#[derive(SystemParam)]
pub(super) struct VerificationSaveNavigationObservations<'w, 's> {
    pending_loads: Query<'w, 's, (), With<OpenLoadSlotCatalogueAfterWorldSnapshotSave>>,
    save_requests: MessageReader<'w, 's, SaveWorldSnapshotToSlot>,
    load_requests: MessageReader<'w, 's, LoadWorldSnapshotFromSlot>,
    save_completions: MessageReader<'w, 's, WorldSnapshotSavedToSlot>,
    load_completions: MessageReader<'w, 's, WorldSnapshotLoadedFromSlot>,
    failures: MessageReader<'w, 's, WorldSnapshotPersistenceFailed>,
    profiles: Res<'w, ProfileIndex>,
    profile: Res<'w, ProfileOptions>,
    catalogue: Res<'w, SaveSlotCatalogue>,
    paths: Option<Res<'w, PersistenceFilesystemPaths>>,
    photo_challenges: Query<'w, 's, (&'static PhotoChallenge, &'static PhotoChallengeProgress)>,
    persistent_identifiers: Query<'w, 's, &'static PersistentId>,
    scenarios: Res<'w, WorldScenarios>,
    photo_baselines: Query<
        'w,
        's,
        &'static PhotoChallengesInitializedForScenarioRevision,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            With<DefaultPhotoAlbumStorageInitializedForWorld>,
        ),
    >,
    world_names: Query<
        'w,
        's,
        (&'static Name, Option<&'static AnimalAdoptionOfferInventory>),
        With<WorldRoot>,
    >,
}

pub(super) fn collect_save_navigation_verification_facts(
    session: Res<VerificationJourneySession>,
    mut report: ResMut<VerificationJourneyReport>,
    mut observed: VerificationSaveNavigationObservations,
) {
    // Read every frame, including negative waits: messages expire between snapshots.
    for request in observed.save_requests.read() {
        report.world_snapshot_save_requests = report.world_snapshot_save_requests.saturating_add(1);
        let path = observed.paths.as_ref().map(|paths| {
            paths.save_slot_file_path(
                &observed.profile.profile_identifier.0,
                request.save_slot_identifier,
            )
        });
        info!(target: "openzt2_verification", slot = request.save_slot_identifier.0,
            profile = %observed.profile.profile_identifier.to_lowercase_hexadecimal_string(), ?path, "world snapshot save requested");
    }
    for request in observed.load_requests.read() {
        report.world_snapshot_load_requests = report.world_snapshot_load_requests.saturating_add(1);
        report.last_world_snapshot_load_request = Some(*request);
        info!(target: "openzt2_verification", slot = request.save_slot_identifier.0,
            profile = %observed.profile.profile_identifier.to_lowercase_hexadecimal_string(), "world snapshot load requested");
    }
    for completion in observed.save_completions.read() {
        report.world_snapshot_save_completions =
            report.world_snapshot_save_completions.saturating_add(1);
        report.last_world_snapshot_save_completion = Some(*completion);
        info!(target: "openzt2_verification", slot = completion.save_slot_identifier.0,
            bytes = completion.saved_snapshot_byte_count, "world snapshot save completed");
    }
    for completion in observed.load_completions.read() {
        report.world_snapshot_load_completions =
            report.world_snapshot_load_completions.saturating_add(1);
        report.last_world_snapshot_load_completion = Some(*completion);
        info!(target: "openzt2_verification", slot = completion.save_slot_identifier.0,
            "world snapshot load completed");
    }
    for failure in observed.failures.read() {
        report.world_snapshot_persistence_failures =
            report.world_snapshot_persistence_failures.saturating_add(1);
        let message = format!(
            "world snapshot {:?} slot {} failed: {:?}",
            failure.persistence_operation, failure.save_slot_identifier.0, failure.failure_reason
        );
        if session.stage == VerificationSessionStage::Journey {
            // Preserve the typed reason and stop this failed operation's blind wait.
            let location = session
                .steps
                .get(session.step_index)
                .map_or("persistence", |step| step.source_location.as_str());
            record_verification_failure(&mut report, location, message);
        } else {
            error!(target: "openzt2_verification", %message);
        }
    }
    if session.fact_collection != VerificationFactCollectionState::Collected {
        return;
    }
    report.current_facts.insert(
        "world_photo_baseline_initialized".to_owned(),
        f64::from(u8::from(observed.scenarios.revision().is_some_and(
            |revision| {
                observed
                    .photo_baselines
                    .single()
                    .is_ok_and(|initialized| initialized.matches_revision(revision))
            },
        ))),
    );
    for (name, value) in [
        (
            "world_snapshot_save_requests",
            report.world_snapshot_save_requests,
        ),
        (
            "world_snapshot_load_requests",
            report.world_snapshot_load_requests,
        ),
        (
            "world_snapshot_save_completions",
            report.world_snapshot_save_completions,
        ),
        (
            "world_snapshot_load_completions",
            report.world_snapshot_load_completions,
        ),
        (
            "pending_load_after_save",
            u32::try_from(observed.pending_loads.iter().count()).unwrap_or(u32::MAX),
        ),
        (
            "world_snapshot_persistence_failures",
            report.world_snapshot_persistence_failures,
        ),
    ] {
        report
            .current_facts
            .insert(name.to_owned(), f64::from(value));
    }
    let saved_slot = report
        .last_world_snapshot_save_completion
        .map(|completion| completion.save_slot_identifier);
    for (name, slot) in [
        ("world_snapshot_saved_slot", saved_slot),
        (
            "world_snapshot_requested_load_slot",
            report
                .last_world_snapshot_load_request
                .map(|request| request.save_slot_identifier),
        ),
        (
            "world_snapshot_loaded_slot",
            report
                .last_world_snapshot_load_completion
                .map(|completion| completion.save_slot_identifier),
        ),
    ] {
        report
            .current_facts
            .insert(name.to_owned(), slot.map_or(-1.0, |slot| f64::from(slot.0)));
    }
    let matches_profile_and_world_name =
        observed.catalogue.save_slot_records.iter().any(|record| {
            Some(record.save_slot_identifier) == saved_slot
                && record.profile_identifier == observed.profile.profile_identifier
                && observed.profiles.selected_profile_identifier == Some(record.profile_identifier)
                && observed
                    .world_names
                    .single()
                    .is_ok_and(|(name, _)| name.as_str() == record.world_display_name)
        });
    report.current_facts.insert(
        "saved_slot_matches_active_profile_and_world_name".to_owned(),
        f64::from(u8::from(matches_profile_and_world_name)),
    );
    report.current_facts.insert(
        "world_limited_adoption_offer_inventory_present".to_owned(),
        observed
            .world_names
            .single()
            .map_or(-1.0, |(_, inventory)| {
                f64::from(u8::from(inventory.is_some()))
            }),
    );
    let mut challenge_progress = observed.photo_challenges.iter().collect::<Vec<_>>();
    challenge_progress.sort_unstable_by_key(|(challenge, _)| challenge.id);
    report.current_facts.insert(
        "photo_challenge_progress_count".to_owned(),
        challenge_progress.len() as f64,
    );
    // Compare identity-associated progress across world reconstruction without
    // retaining entity IDs or a second challenge registry.
    let mut progress_hash = blake3::Hasher::new();
    let mut unresolved_photo_references = 0_u32;
    for (challenge, progress) in challenge_progress {
        progress_hash.update(&challenge.id.0);
        progress_hash.update(&[
            u8::from(progress.completed),
            u8::from(progress.completed_by.is_some()),
            progress.rating_stars,
        ]);
        let photo_identifier = progress
            .completed_by
            .and_then(|photo| observed.persistent_identifiers.get(photo).ok().copied());
        if progress.completed_by.is_some() && photo_identifier.is_none() {
            unresolved_photo_references = unresolved_photo_references.saturating_add(1);
        }
        progress_hash.update(&photo_identifier.map_or(0, |id| id.0).to_le_bytes());
    }
    report.current_facts.insert(
        "photo_challenge_unresolved_photo_references".to_owned(),
        f64::from(unresolved_photo_references),
    );
    for (index, bytes) in progress_hash.finalize().as_bytes()[..16]
        .chunks_exact(4)
        .enumerate()
    {
        report.current_facts.insert(
            format!("photo_challenge_progress_state_{index}"),
            f64::from(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])),
        );
    }
}
