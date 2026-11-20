use bevy::{ecs::relationship::RelationshipTarget, prelude::*};
use std::collections::BTreeMap;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};
use openzt2_zoo_state::economy::{
    account_transaction_types::{Account, TransactionKind, TransactionRequest},
    money_types::Money,
};

use super::{
    award_and_progression_fact_types::{
        AuthoredAwardConditionIndex, EarnedProgressionAward, ProgressionAward,
        ProgressionAwardConditionDurationProgress, ProgressionAwardConditionOf,
        ProgressionAwardConditionStorageInitialized, ProgressionAwardConditions,
        ProgressionAwardGrantCandidate, ProgressionAwardGranted, ProgressionAwardState,
        ProgressionFactChanged,
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
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    earned: Query<(Entity, &WorldMember, &EarnedProgressionAward)>,
    saved_progress: Query<(
        Entity,
        &WorldMember,
        &ProgressionAwardConditionDurationProgress,
    )>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut commands: Commands,
) {
    let Some(root) = roots.iter().find(|&entity| ids.owns_world(entity)) else {
        return;
    };
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    let earned = earned
        .iter()
        .filter(|(_, member, _)| member.root == root)
        .map(|(entity, _, award)| (award.definition, entity))
        .collect::<BTreeMap<_, _>>();
    let saved_progress = saved_progress
        .iter()
        .filter(|(_, member, _)| member.root == root)
        .map(|(entity, _, progress)| ((progress.award, progress.condition_index), entity))
        .collect::<BTreeMap<_, _>>();
    let timed_condition_count = definitions
        .awards()
        .flat_map(|award| {
            award
                .conditions
                .iter()
                .enumerate()
                .map(move |(index, condition)| (award.id, index, condition))
        })
        .filter(|&(award, index, condition)| {
            condition.duration_ticks != 0 && !saved_progress.contains_key(&(award, index as u32))
        })
        .count();
    if u64::try_from(timed_condition_count)
        .ok()
        .and_then(|count| ids.next().checked_add(count))
        .is_none()
    {
        return;
    }
    let world = WorldMember { root };
    for definition in definitions.awards() {
        let award = if let Some(&entity) = earned.get(&definition.id) {
            commands.entity(entity).insert(ProgressionAward {
                definition: definition.id,
                state: ProgressionAwardState::Earned,
            });
            entity
        } else {
            commands
                .spawn((
                    ProgressionAward {
                        definition: definition.id,
                        state: ProgressionAwardState::Evaluating,
                    },
                    world,
                ))
                .id()
        };
        for (index, condition) in definition.conditions.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                return;
            };
            if let Some(&entity) = saved_progress.get(&(definition.id, index)) {
                commands.entity(entity).insert((
                    ProgressionAwardConditionOf(award),
                    AuthoredAwardConditionIndex(index),
                ));
                continue;
            }
            let mut row = commands.spawn((
                ProgressionAwardConditionOf(award),
                AuthoredAwardConditionIndex(index),
                world,
            ));
            if condition.duration_ticks != 0 {
                let Ok(id) = ids.allocate(root) else {
                    return;
                };
                row.insert((
                    id,
                    ProgressionAwardConditionDurationProgress {
                        award: definition.id,
                        condition_index: index,
                        satisfied_ticks: 0,
                    },
                ));
            }
        }
    }
    commands
        .entity(root)
        .insert(ProgressionAwardConditionStorageInitialized);
}

pub(super) fn evaluate_all_progression_award_conditions_and_queue_grants(
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    rating: Res<ZooRating>,
    mut awards: Query<
        (Entity, &mut ProgressionAward, &ProgressionAwardConditions),
        Without<EarnedProgressionAward>,
    >,
    mut conditions: Query<(
        &AuthoredAwardConditionIndex,
        Option<&mut ProgressionAwardConditionDurationProgress>,
    )>,
    mut candidates: MessageWriter<ProgressionAwardGrantCandidate>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for (award, mut state, members) in &mut awards {
        let Some(definition) = definitions.find_award(state.definition) else {
            continue;
        };
        let mut eligible =
            !definition.conditions.is_empty() && members.len() == definition.conditions.len();
        for entity in members.iter() {
            let Ok((index, progress)) = conditions.get_mut(entity) else {
                eligible = false;
                continue;
            };
            let Some(condition) = definition.conditions.get(index.0 as usize) else {
                eligible = false;
                continue;
            };
            let satisfied =
                select_zoo_rating_input(&rating, &condition.kind).is_some_and(|value| {
                    zoo_rating_satisfies_authored_comparison(
                        value,
                        condition.value,
                        &condition.comparison,
                    )
                });
            if condition.duration_ticks == 0 {
                eligible &= satisfied;
            } else if let Some(mut progress) = progress {
                let next = advance_award_condition_satisfied_tick_count(
                    progress.satisfied_ticks,
                    satisfied,
                    condition.duration_ticks,
                );
                if progress.satisfied_ticks != next {
                    progress.satisfied_ticks = next;
                    changed.write(ProgressionFactChanged::Award(state.definition));
                }
                eligible &= satisfied && next >= condition.duration_ticks;
            } else {
                eligible = false;
            }
        }
        let next = if eligible {
            ProgressionAwardState::Ready
        } else {
            ProgressionAwardState::Evaluating
        };
        if state.state != next {
            state.state = next;
        }
        if eligible {
            candidates.write(ProgressionAwardGrantCandidate { award });
        }
    }
}

pub(super) fn grant_earned_progression_awards(
    mut candidates: MessageReader<ProgressionAwardGrantCandidate>,
    clock: Res<ZooClock>,
    assets: Res<Assets<WorldDefinitionAsset>>,
    definitions: Res<WorldDefinitions>,
    mut awards: Query<(&WorldMember, &mut ProgressionAward)>,
    mut ids: ResMut<PersistentIdAllocator>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    mut commands: Commands,
    mut granted: MessageWriter<ProgressionAwardGranted>,
    mut changed: MessageWriter<ProgressionFactChanged>,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    let Some(definitions) = definitions.get(&assets) else {
        return;
    };
    for candidate in candidates.read() {
        let Ok((world, mut state)) = awards.get_mut(candidate.award) else {
            continue;
        };
        if state.state != ProgressionAwardState::Ready {
            continue;
        }
        let Some(definition) = definitions.find_award(state.definition) else {
            continue;
        };
        if !definition
            .unlocks
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        let Ok(id) = ids.allocate(world.root) else {
            continue;
        };
        state.state = ProgressionAwardState::Earned;
        commands.entity(candidate.award).insert((
            id,
            EarnedProgressionAward {
                definition: definition.id,
                earned_tick: clock.tick,
            },
        ));
        granted.write(ProgressionAwardGranted {
            award: candidate.award,
            definition: definition.id,
        });
        for &target in &definition.unlocks {
            if let Some(index) = find_catalogue_definition_index(definitions, target) {
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(target));
                }
            }
        }
        if definition.reward_cents > 0 {
            transactions.write(TransactionRequest {
                operation: candidate.award,
                debit: Account::External,
                credit: Account::Zoo,
                amount: Money(definition.reward_cents),
                kind: TransactionKind::Reward,
                subject: Some(candidate.award),
            });
        }
        changed.write(ProgressionFactChanged::Award(definition.id));
    }
}
