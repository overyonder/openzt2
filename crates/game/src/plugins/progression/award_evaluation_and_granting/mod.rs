use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::economy::money_types::Money;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    award_and_progression_fact_types::{
        EarnedProgressionAward, ProgressionAwardConditionDurationProgress,
        ProgressionAwardConditionStorageInitialized, ProgressionAwardGrantCandidate,
        ProgressionAwardGranted, ProgressionFactChanged,
    },
    award_condition_calculations::advance_award_condition_satisfied_tick_count,
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::mark_catalogue_definition_unlocked_at_index,
    rating_types::ZooRating,
    unlock_types::UnlockedCatalogueDefinitionSet,
    zoo_rating_calculations::{select_zoo_rating_input, zoo_rating_satisfies_authored_comparison},
};

pub(super) fn initialize_award_condition_duration_storage(
    roots: Query<
        Entity,
        (
            With<WorldRoot>,
            With<WorldLoadCompleted>,
            Without<ProgressionAwardConditionStorageInitialized>,
        ),
    >,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    existing: Query<&ProgressionAwardConditionDurationProgress>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut commands: Commands,
) {
    let Some(root) = roots.iter().next() else {
        return;
    };
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for award in definitions.awards() {
        let award_id = award.id;
        for (offset, condition) in award.conditions.iter().enumerate() {
            if condition.duration_ticks == 0
                || existing.iter().any(|progress| {
                    progress.award == award_id && progress.condition_index == offset as u32
                })
            {
                continue;
            }
            let Ok(id) = ids.allocate(root) else {
                return;
            };
            commands.spawn((
                ProgressionAwardConditionDurationProgress {
                    award: award_id,
                    condition_index: offset as u32,
                    satisfied_ticks: 0,
                },
                WorldMember { root },
                id,
            ));
        }
    }
    commands
        .entity(root)
        .insert(ProgressionAwardConditionStorageInitialized);
}

pub(super) fn evaluate_all_progression_award_conditions_and_queue_grants(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    _clock: Res<ZooClock>,
    earned_awards: Query<&EarnedProgressionAward>,
    mut condition_progress: Query<&mut ProgressionAwardConditionDurationProgress>,
    mut grant_candidates: MessageWriter<ProgressionAwardGrantCandidate>,
    mut changed_progression_facts: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for award in definitions.awards() {
        let award_id = award.id;
        if earned_awards
            .iter()
            .any(|earned_award| earned_award.definition == award_id)
        {
            continue;
        }
        for (condition_index, condition) in award.conditions.iter().enumerate() {
            if condition.duration_ticks == 0 {
                continue;
            }
            let rating_value = select_zoo_rating_input(&rating, &condition.kind);
            let condition_is_satisfied = rating_value.is_some_and(|value| {
                zoo_rating_satisfies_authored_comparison(
                    value,
                    condition.value,
                    &condition.comparison,
                )
            });
            if let Some(mut progress) = condition_progress.iter_mut().find(|progress| {
                progress.award == award_id && progress.condition_index == condition_index as u32
            }) {
                let next_satisfied_ticks = advance_award_condition_satisfied_tick_count(
                    progress.satisfied_ticks,
                    condition_is_satisfied,
                    condition.duration_ticks,
                );
                if progress.satisfied_ticks != next_satisfied_ticks {
                    progress.satisfied_ticks = next_satisfied_ticks;
                    changed_progression_facts.write(ProgressionFactChanged::Award(award_id));
                }
            }
        }
        let all_conditions_are_satisfied = !award.conditions.is_empty()
            && award
                .conditions
                .iter()
                .enumerate()
                .all(|(condition_index, condition)| {
                    let rating_value = select_zoo_rating_input(&rating, &condition.kind);
                    rating_value.is_some_and(|value| {
                        zoo_rating_satisfies_authored_comparison(
                            value,
                            condition.value,
                            &condition.comparison,
                        )
                    }) && (condition.duration_ticks == 0
                        || condition_progress.iter().any(|progress| {
                            progress.award == award_id
                                && progress.condition_index == condition_index as u32
                                && progress.satisfied_ticks >= condition.duration_ticks
                        }))
                });
        if all_conditions_are_satisfied {
            grant_candidates.write(ProgressionAwardGrantCandidate {
                definition: award_id,
            });
        }
    }
}

pub(super) fn grant_earned_progression_awards(
    mut candidates: MessageReader<ProgressionAwardGrantCandidate>,
    clock: Res<ZooClock>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    earned: Query<&EarnedProgressionAward>,
    roots: Query<(Entity, &WorldRoot)>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut commands: Commands,
    mut granted: MessageWriter<ProgressionAwardGranted>,
    mut changed: MessageWriter<ProgressionFactChanged>,
    mut transactions: MessageWriter<TransactionRequest>,
    mut materialized: Local<Vec<AssetId>>,
) {
    if candidates.is_empty() {
        return;
    }
    let (Some(definitions), Some((root, _))) =
        (active_definitions.get(&definitions), roots.iter().next())
    else {
        return;
    };
    materialized.clear();
    let definition_count = definitions.awards().count();
    if materialized.capacity() < definition_count {
        let additional = definition_count - materialized.capacity();
        materialized.reserve_exact(additional);
    }
    for candidate in candidates.read() {
        if materialized.contains(&candidate.definition)
            || earned
                .iter()
                .any(|award| award.definition == candidate.definition)
        {
            continue;
        }
        materialized.push(candidate.definition);
        let Some(definition) = definitions.find_award(candidate.definition) else {
            continue;
        };
        let targets = &definition.unlocks;
        if !targets
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        let Ok(id) = ids.allocate(root) else {
            continue;
        };
        let award = commands
            .spawn((
                EarnedProgressionAward {
                    definition: candidate.definition,
                    earned_tick: clock.tick,
                },
                WorldMember { root },
                id,
            ))
            .id();
        granted.write(ProgressionAwardGranted {
            award,
            definition: candidate.definition,
        });
        for target in targets {
            let id = *target;
            if let Some(index) = find_catalogue_definition_index(definitions, id) {
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(id));
                }
            }
        }
        if definition.reward_cents > 0 {
            transactions.write(TransactionRequest {
                operation: award,
                debit: Account::External,
                credit: Account::Zoo,
                amount: Money(definition.reward_cents),
                kind: TransactionKind::Reward,
                subject: Some(award),
            });
        }
        changed.write(ProgressionFactChanged::Award(candidate.definition));
    }
}
