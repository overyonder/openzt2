//! Hydration of live photo-challenge entities from the active scenario catalogue.

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::photo_challenge_types::{
    PhotoChallenge, PhotoChallengeProgress, PhotoChallengeSet, PhotoChallengeSetProgress,
    PhotoChallengesInitializedForScenarioRevision,
};

pub(super) fn hydrate_photo_challenges_and_sets_from_active_scenario_catalogue(
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    roots: Query<
        (
            Entity,
            Option<&PhotoChallengesInitializedForScenarioRevision>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    existing_challenges: Query<(Entity, &WorldMember, &PhotoChallenge)>,
    existing_sets: Query<(Entity, &WorldMember, &PhotoChallengeSet)>,
    mut commands: Commands,
) {
    let Some(revision) = scenarios.revision() else {
        return;
    };
    let Some(scenarios) = scenarios.get(&assets) else {
        return;
    };
    for (root, initialized) in &roots {
        if initialized.is_some_and(|initialized| initialized.0 == revision) {
            continue;
        }
        let mut retained_challenges = BTreeSet::new();
        for record in scenarios.photo_challenges() {
            let Some(source) = scenarios.handle_for_photo_challenge(record.id) else {
                continue;
            };
            let Some(row) = assets.get(source).and_then(|asset| {
                asset
                    .document
                    .photo_challenges
                    .iter()
                    .position(|row| row.id == record.id)
            }) else {
                continue;
            };
            let component = PhotoChallenge {
                id: record.id,
                source: source.clone(),
                row: row as u32,
            };
            if let Some((entity, ..)) = existing_challenges
                .iter()
                .find(|(_, member, challenge)| member.root == root && challenge.id == record.id)
            {
                retained_challenges.insert(entity);
                commands.entity(entity).insert(component);
            } else {
                retained_challenges.insert(
                    commands
                        .spawn((
                            WorldMember { root },
                            component,
                            PhotoChallengeProgress::default(),
                        ))
                        .id(),
                );
            }
        }
        existing_challenges
            .iter()
            .filter(|(entity, member, _)| {
                member.root == root && !retained_challenges.contains(entity)
            })
            .for_each(|(entity, ..)| commands.entity(entity).despawn());

        let mut retained_sets = BTreeSet::new();
        for record in scenarios.photo_sets() {
            let Some(handle) = scenarios.handle_for_photo_set(record.id) else {
                continue;
            };
            let Some(row) = assets.get(handle).and_then(|asset| {
                asset
                    .document
                    .photo_challenge_sets
                    .iter()
                    .position(|row| row.id == record.id)
            }) else {
                continue;
            };
            let component = PhotoChallengeSet {
                id: record.id,
                source: handle.clone(),
                row: row as u32,
            };
            if let Some((entity, ..)) = existing_sets
                .iter()
                .find(|(_, member, set)| member.root == root && set.id == record.id)
            {
                retained_sets.insert(entity);
                commands.entity(entity).insert(component);
            } else {
                retained_sets.insert(
                    commands
                        .spawn((
                            WorldMember { root },
                            component,
                            PhotoChallengeSetProgress::default(),
                        ))
                        .id(),
                );
            }
        }
        existing_sets
            .iter()
            .filter(|(entity, member, _)| member.root == root && !retained_sets.contains(entity))
            .for_each(|(entity, ..)| commands.entity(entity).despawn());
        commands
            .entity(root)
            .insert(PhotoChallengesInitializedForScenarioRevision(revision));
    }
}
