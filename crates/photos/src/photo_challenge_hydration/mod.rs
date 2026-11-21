//! Hydration of live photo-challenge entities from the active scenario catalogue.

use std::collections::BTreeMap;

use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_game_assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use openzt2_interface_state::photos::photo_challenge_types::{
    PhotoChallenge, PhotoChallengeCatalogueBound, PhotoChallengeCatalogueIndex,
    PhotoChallengeEntryOf, PhotoChallengeProgress, PhotoChallengeSet, WorldPhotoChallengeEntries,
};
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::photo_challenge_lua_scoring_types::{
    PhotoChallengeScorer, PhotoChallengeScript, PhotoRuleScriptContext,
};

pub(super) fn hydrate_photo_challenges_and_sets_from_active_scenario_catalogue(
    scenarios: Res<WorldScenarios>,
    assets: Res<Assets<WorldScenarioDocumentAsset>>,
    roots: Query<
        (
            Entity,
            Has<PhotoChallengeCatalogueBound>,
            Option<&WorldPhotoChallengeEntries>,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    mut challenges: Query<&mut PhotoChallenge>,
    scripts: Query<&PhotoChallengeScript>,
    rule_scripts: Query<&PhotoRuleScriptContext>,
    mut sets: Query<&mut PhotoChallengeSet>,
    mut indexes: Query<&mut PhotoChallengeCatalogueIndex>,
    mut commands: Commands,
) {
    let catalogue_changed = scenarios.is_changed();
    let ready_catalogue = scenarios
        .catalogue_documents_are_loaded(&assets)
        .then(|| scenarios.get(&assets))
        .flatten();
    let Some(catalogue) = ready_catalogue else {
        for (root, bound, _) in &roots {
            if bound {
                commands
                    .entity(root)
                    .remove::<(PhotoChallengeCatalogueBound, PhotoChallengeCatalogueIndex)>();
            }
        }
        return;
    };
    for (root, bound, entries) in &roots {
        if bound && !catalogue_changed {
            continue;
        }
        let mut existing_challenges = BTreeMap::new();
        let mut existing_sets = BTreeMap::new();
        let mut existing_scripts = BTreeMap::new();
        if let Some(entries) = entries {
            for entity in entries.iter() {
                if let Ok(challenge) = challenges.get(entity) {
                    existing_challenges.insert(challenge.id, entity);
                } else if let Ok(set) = sets.get(entity) {
                    existing_sets.insert(set.id, entity);
                } else if let Ok(script) = scripts.get(entity) {
                    existing_scripts.insert(script.0.id(), entity);
                }
            }
        }
        let mut index = PhotoChallengeCatalogueIndex::default();
        let mut current_scripts = BTreeMap::new();
        for record in catalogue.photo_challenges() {
            let Some(source) = catalogue.handle_for_photo_challenge(record.id) else {
                continue;
            };
            let challenge = PhotoChallenge {
                id: record.id,
                source: source.clone(),
            };
            let entity = if let Some(entity) = existing_challenges.remove(&record.id) {
                if let Ok(mut existing) = challenges.get_mut(entity) {
                    existing.set_if_neq(challenge);
                }
                entity
            } else {
                commands
                    .spawn((
                        WorldMember { root },
                        PhotoChallengeEntryOf(root),
                        challenge,
                        PhotoChallengeProgress::default(),
                    ))
                    .id()
            };
            if let Some(script) = assets.get(source).and_then(|asset| {
                let binding = asset
                    .document
                    .photo_challenge_script_bindings
                    .iter()
                    .find(|binding| binding.challenge == record.id)?;
                asset.photo_script(binding).map(|(_, script)| script)
            }) {
                let context = *current_scripts.entry(script.id()).or_insert_with(|| {
                    existing_scripts.remove(&script.id()).unwrap_or_else(|| {
                        commands
                            .spawn((
                                WorldMember { root },
                                PhotoChallengeEntryOf(root),
                                PhotoChallengeScript(script.clone()),
                                PhotoChallengeScorer::default(),
                            ))
                            .id()
                    })
                });
                if !rule_scripts
                    .get(entity)
                    .is_ok_and(|current| current.0 == context)
                {
                    commands
                        .entity(entity)
                        .insert(PhotoRuleScriptContext(context));
                }
            } else if rule_scripts.contains(entity) {
                commands.entity(entity).remove::<PhotoRuleScriptContext>();
            }
            index.rules.insert(record.id, entity);
        }
        for record in catalogue.photo_sets() {
            let Some(source) = catalogue.handle_for_photo_set(record.id) else {
                continue;
            };
            let set = PhotoChallengeSet {
                id: record.id,
                source: source.clone(),
            };
            let entity = if let Some(entity) = existing_sets.remove(&record.id) {
                if let Ok(mut existing) = sets.get_mut(entity) {
                    existing.set_if_neq(set);
                }
                entity
            } else {
                commands
                    .spawn((WorldMember { root }, PhotoChallengeEntryOf(root), set))
                    .id()
            };
            index.sets.insert(record.id, entity);
        }
        for entity in existing_challenges
            .into_values()
            .chain(existing_sets.into_values())
            .chain(existing_scripts.into_values())
        {
            commands.entity(entity).despawn();
        }
        if !bound {
            commands.entity(root).insert(PhotoChallengeCatalogueBound);
        }
        if let Ok(mut current) = indexes.get_mut(root) {
            current.set_if_neq(index);
        } else {
            commands.entity(root).insert(index);
        }
    }
}
