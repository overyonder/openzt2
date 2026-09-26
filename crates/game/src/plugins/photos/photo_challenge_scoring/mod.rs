//! Lua scoring of captured photos and progress updates for challenge sets.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::lua_script::lua_script_archive_path_index::LuaScriptArchivePathIndex;
use crate::assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::persistence::generated_photo_persistence_types::PersistChallengePhotoJpegCopy;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    photo_capture_types::{
        CapturedPhotoEvidence, CapturedPhotoSemantics, PendingPhotoScore, Photo, PhotoCaptured,
        PhotoChallengeCompleted,
    },
    photo_challenge_lua_scoring_types::PhotoChallengeLuaScoringVirtualMachineContexts,
    photo_challenge_types::{
        PhotoChallenge, PhotoChallengeProgress, PhotoChallengeSet, PhotoChallengeSetProgress,
    },
};

pub(super) fn score_pending_photos_against_incomplete_lua_challenges(
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_index: Res<LuaScriptArchivePathIndex>,
    mut photo_challenge_lua_scoring_virtual_machine_contexts: NonSendMut<
        PhotoChallengeLuaScoringVirtualMachineContexts,
    >,
    mut photos: Query<
        (
            Entity,
            &mut Photo,
            &CapturedPhotoEvidence,
            &CapturedPhotoSemantics,
        ),
        With<PendingPhotoScore>,
    >,
    mut challenges: Query<(&PhotoChallenge, &WorldMember, &mut PhotoChallengeProgress)>,
    mut captured: MessageWriter<PhotoCaptured>,
    mut completed: MessageWriter<PhotoChallengeCompleted>,
    mut challenge_photo_persistence_requests: MessageWriter<PersistChallengePhotoJpegCopy>,
    mut commands: Commands,
) {
    for (photo_entity, mut photo, visual, semantic) in &mut photos {
        let mut best = 0;
        for (challenge, _member, mut progress) in &mut challenges {
            if progress.completed {
                continue;
            }
            let Some(asset) = assets.get(&challenge.source) else {
                continue;
            };
            let Some(record) = asset.document.photo_challenges.get(challenge.row as usize) else {
                continue;
            };
            let Some(binding) = asset
                .document
                .photo_challenge_script_bindings
                .iter()
                .find(|binding| binding.challenge == record.id)
            else {
                error!(challenge = ?record.id, "photo challenge has no Lua scorer binding");
                continue;
            };
            let Some((path, script)) = asset.photo_script(binding) else {
                error!(script = %binding.script, "photo challenge Lua scorer is not loaded");
                continue;
            };
            let score = match photo_challenge_lua_scoring_virtual_machine_contexts
                .execute_photo_challenge_lua_scoring_function(
                    record.id,
                    challenge.source.id(),
                    path,
                    script,
                    &scripts,
                    &script_index,
                    &binding.entry,
                    visual,
                    semantic,
                ) {
                Ok(score) => score,
                Err(error) => {
                    error!(entry = %binding.entry, %error, "photo challenge Lua scorer failed");
                    continue;
                }
            };
            best = best.max(score);
            if score == 0 {
                continue;
            }
            let subject = visual
                .0
                .iter()
                .max_by_key(|evidence| {
                    u32::from(evidence.facts.screen_permille)
                        + u32::from(evidence.facts.center_permille)
                })
                .map_or(AssetId::default(), |evidence| evidence.definition);
            let rating_stars = u8::try_from(score.saturating_add(999) / 1000)
                .unwrap_or(5)
                .min(5);
            progress.completed = true;
            progress.completed_by = Some(photo_entity);
            progress.rating_stars = rating_stars;
            completed.write(PhotoChallengeCompleted {
                photo: photo_entity,
                challenge: record.id,
                subject,
                rating_stars,
            });
            challenge_photo_persistence_requests.write(PersistChallengePhotoJpegCopy {
                photo_entity,
                challenge_definition_identifier: record.id,
            });
        }
        photo.score_milli = best;
        captured.write(PhotoCaptured {
            photo: photo_entity,
            score_milli: best,
        });
        commands.entity(photo_entity).remove::<PendingPhotoScore>();
    }
}

pub(super) fn update_photo_challenge_set_progress_from_completed_challenges(
    mut completed: MessageReader<PhotoChallengeCompleted>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    mut sets: Query<(&PhotoChallengeSet, &mut PhotoChallengeSetProgress)>,
) {
    for event in completed.read() {
        for (set, mut progress) in &mut sets {
            let Some(record) = assets
                .get(&set.source)
                .and_then(|asset| asset.document.photo_challenge_sets.get(set.row as usize))
            else {
                continue;
            };
            if !record
                .members
                .iter()
                .any(|member| member.challenge == event.challenge)
            {
                continue;
            }
            progress.completed_members = progress.completed_members.saturating_add(1);
            progress.rating_stars = progress
                .rating_stars
                .saturating_add(u16::from(event.rating_stars));
            if event.subject != AssetId::default()
                && !progress.selected_subjects[..usize::from(progress.selected_subject_count)]
                    .contains(&event.subject)
                && usize::from(progress.selected_subject_count) < progress.selected_subjects.len()
            {
                let index = usize::from(progress.selected_subject_count);
                progress.selected_subjects[index] = event.subject;
                progress.selected_subject_count += 1;
            }
        }
    }
}
