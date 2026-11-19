//! Hydration of live photo-challenge entities from the active scenario catalogue.

use openzt2_interface_state::photos::photo_challenge_types::PhotoChallenge;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeCatalogueBound;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeProgress;
use openzt2_interface_state::photos::photo_challenge_types::PhotoChallengeSet;

use std::collections::BTreeSet;

use bevy::prelude::*;

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::photo_challenge_types::PhotoChallengeSetProgress;

pub(super) fn hydrate_photo_challenges_and_sets_from_active_scenario_catalogue(
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    roots: Query<
        (Entity, Option<&PhotoChallengeCatalogueBound>),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    mut existing_challenges: Query<(Entity, &WorldMember, &mut PhotoChallenge)>,
    mut existing_sets: Query<(Entity, &WorldMember, &mut PhotoChallengeSet)>,
    mut commands: Commands,
) {
    let catalogue_changed = scenarios.is_changed();
    if !scenarios.catalogue_documents_are_loaded(&assets) {
        return;
    }
    let Some(scenarios) = scenarios.get(&assets) else {
        return;
    };
    for (root, initialized) in &roots {
        if initialized.is_some() && !catalogue_changed {
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
            if let Some((entity, _, mut current)) = existing_challenges
                .iter_mut()
                .find(|(_, member, challenge)| member.root == root && challenge.id == record.id)
            {
                retained_challenges.insert(entity);
                current.set_if_neq(component);
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
            if let Some((entity, _, mut current)) = existing_sets
                .iter_mut()
                .find(|(_, member, set)| member.root == root && set.id == record.id)
            {
                retained_sets.insert(entity);
                current.set_if_neq(component);
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
        if initialized.is_none() {
            commands.entity(root).insert(PhotoChallengeCatalogueBound);
        }
    }
}
