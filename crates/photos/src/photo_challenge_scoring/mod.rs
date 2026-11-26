//! Score photographs against the selected world's authored requirements.

use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::lua_script::{
    lua_script_asset_loading::LuaScriptAsset, lua_script_sources::LuaScriptSources,
};
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::persistence::generated_photo_persistence_types::PersistChallengePhotoJpegCopy;
use openzt2_interface_state::photos::photo_capture_types::Photo;
use openzt2_interface_state::photos::photo_challenge_types::{
    PhotoChallenge, PhotoChallengeCatalogueIndex, PhotoChallengeCompleted, PhotoChallengeProgress,
    PhotoChallengeSet,
};
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;

use super::photo_capture_types::{CapturedPhotoEvidence, PendingPhotoScore, PhotoCaptured};
use super::photo_challenge_lua_scoring_types::{PhotoChallengeScorer, PhotoRuleScriptContext};

pub(super) fn score_pending_photos_against_incomplete_lua_challenges(
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    scripts: Res<Assets<LuaScriptAsset>>,
    script_sources: Res<LuaScriptSources>,
    worlds: Query<&PhotoChallengeCatalogueIndex>,
    sets: Query<&PhotoChallengeSet>,
    mut challenges: Query<(
        &PhotoChallenge,
        &mut PhotoChallengeProgress,
        &PhotoRuleScriptContext,
    )>,
    mut scoring_contexts: Query<&mut PhotoChallengeScorer>,
    mut photos: Query<
        (Entity, &WorldMember, &mut Photo, &CapturedPhotoEvidence),
        With<PendingPhotoScore>,
    >,
    mut captured: MessageWriter<PhotoCaptured>,
    mut completed: MessageWriter<PhotoChallengeCompleted>,
    mut persistence: MessageWriter<PersistChallengePhotoJpegCopy>,
    mut commands: Commands,
) {
    let mut exposures = photos
        .iter()
        .map(|(entity, _, photo, _)| (photo.captured_tick, entity))
        .collect::<Vec<_>>();
    exposures.sort_unstable();
    for (_, photo_entity) in exposures {
        let Ok((_, world, mut photo, evidence)) = photos.get_mut(photo_entity) else {
            continue;
        };
        let mut best = 0;
        if let Ok(entries) = worlds.get(world.root) {
            let selected = evidence
                .challenge_set
                .and_then(|id| entries.sets.get(&id))
                .and_then(|&entity| sets.get(entity).ok());
            if let Some(record) = selected.and_then(|set| {
                assets
                    .get(&set.source)?
                    .document
                    .find_photo_challenge_set(set.id)
            }) {
                let subject = evidence
                    .subjects
                    .iter()
                    .max_by(|left, right| {
                        left.facts
                            .screen_fraction
                            .total_cmp(&right.facts.screen_fraction)
                    })
                    .map_or(AssetId::default(), |item| item.definition);
                for member in &record.members {
                    let Some(&entity) = entries.rules.get(&member.challenge) else {
                        continue;
                    };
                    let Ok((challenge, mut progress, context)) = challenges.get_mut(entity) else {
                        continue;
                    };
                    if progress.completed {
                        continue;
                    }
                    let Some(asset) = assets.get(&challenge.source) else {
                        continue;
                    };
                    let Some(rule) = asset.document.find_photo_challenge_rule(challenge.id) else {
                        continue;
                    };
                    let Some(binding) = asset
                        .document
                        .photo_challenge_script_bindings
                        .iter()
                        .find(|binding| binding.challenge == rule.id)
                    else {
                        error!(challenge = ?rule.id, "photo challenge has no Lua scorer binding");
                        continue;
                    };
                    let Some((path, script)) = asset.photo_script(binding) else {
                        error!(script = %binding.script, "photo challenge Lua scorer is not loaded");
                        continue;
                    };
                    let Ok(mut scorer) = scoring_contexts.get_mut(context.0) else {
                        continue;
                    };
                    let score = match scorer.score(
                        path,
                        script,
                        &scripts,
                        &script_sources,
                        &binding.entry,
                        evidence,
                    ) {
                        Ok(score) => score,
                        Err(error) => {
                            error!(entry = %binding.entry, %error, "photo challenge Lua scorer failed");
                            continue;
                        }
                    };
                    best = best.max(score);
                    let rating_half_stars = (score / 500) as u8;
                    if rating_half_stars == 0
                        || rating_half_stars < rule.difficulty.saturating_mul(2)
                    {
                        continue;
                    }
                    *progress = PhotoChallengeProgress {
                        completed: true,
                        completed_by: Some(photo_entity),
                        rating_half_stars,
                    };
                    completed.write(PhotoChallengeCompleted {
                        photo: photo_entity,
                        challenge: rule.id,
                        subject,
                        rating_half_stars,
                    });
                    persistence.write(PersistChallengePhotoJpegCopy {
                        photo_entity,
                        challenge_definition_identifier: rule.id,
                    });
                }
            }
        }
        photo.score_milli = best;
        captured.write(PhotoCaptured {
            photo: photo_entity,
            score_milli: best,
        });
        commands.entity(photo_entity).remove::<PendingPhotoScore>();
    }
}
