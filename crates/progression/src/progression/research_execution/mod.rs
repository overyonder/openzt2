use super::research_duration_calculation::resolve_research_duration_to_simulation_ticks;
use bevy::prelude::*;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::TransactionKind;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_world_state::simulation_time::simulation_clock_types::ZooClock;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::{
        catalogue_definition_is_unlocked_at_index, mark_catalogue_definition_unlocked_at_index,
    },
    research_progress_calculations::advance_research_project_by_one_tick,
    research_types::{
        PendingResearchProjectPayment, ResearchAvailabilityUnlockCountdown,
        ResearchDurationOverride, ResearchProject, ResearchProjectAvailability,
        ResearchProjectCompleted, ResearchProjectCompletionReady, ResearchProjectPaused,
        ResearchProjectPersistentId, ResearchPurchaseState, SetResearchDuration,
        StartResearchProjectRequest, WorldResearchItems,
    },
    unlock_types::UnlockedCatalogueDefinitionSet,
};

pub(super) fn validate_start_research_requests_and_request_payments(
    mut requests: MessageReader<StartResearchProjectRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    mut items: Query<(
        &WorldMember,
        &ResearchProjectAvailability,
        &mut ResearchPurchaseState,
        Has<ResearchProject>,
    )>,
    mut commands: Commands,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    if requests.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in requests.read() {
        let Ok((member, availability, mut purchase, has_project)) = items.get_mut(request.item)
        else {
            continue;
        };
        if !availability.available || has_project || *purchase != ResearchPurchaseState::Idle {
            continue;
        }
        let Some(research) = definitions.find_research(availability.item) else {
            continue;
        };
        let targets = &research.unlocks;
        let invalid_target = targets
            .iter()
            .any(|target| find_catalogue_definition_index(definitions, *target).is_none()
                || definitions.unlock_definitions_targeting_catalogue_definition(*target).any(|unlock| !matches!(unlock.requirement, openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::UnlockRequirement::Research(_))));
        let already_completed = !targets.is_empty()
            && targets.iter().all(|target| {
                find_catalogue_definition_index(definitions, *target)
                    .is_some_and(|index| catalogue_definition_is_unlocked_at_index(&unlocks, index))
            });
        if !research.prerequisites.iter().all(|id| {
            find_catalogue_definition_index(definitions, *id)
                .is_some_and(|index| catalogue_definition_is_unlocked_at_index(&unlocks, index))
        }) || invalid_target
            || already_completed
        {
            continue;
        }
        let operation = commands
            .spawn((
                PendingResearchProjectPayment {
                    item: request.item,
                },
                *member,
                openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction::Research,
            ))
            .id();
        *purchase = ResearchPurchaseState::AwaitingPayment(operation);
        transactions.write(TransactionRequest {
            operation,
            debit: Account::Zoo,
            credit: Account::External,
            amount: Money(research.cost_cents),
            kind: TransactionKind::Research,
            subject: None,
        });
    }
}
pub(super) fn start_paid_research_projects(
    mut completed: MessageReader<TransactionCompleted>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    pending: Query<&PendingResearchProjectPayment>,
    mut items: Query<(
        &WorldMember,
        &ResearchProjectAvailability,
        &ResearchDurationOverride,
        &mut ResearchPurchaseState,
    )>,
    mut ids: ResMut<PersistentIdAllocator>,
    seed: Res<ZooSeed>,
    mut commands: Commands,
) {
    let definitions = active_definitions.get(&definitions);
    for result in completed.read() {
        let Ok(payment) = pending.get(result.operation) else {
            continue;
        };
        commands.entity(result.operation).try_despawn();
        let Ok((member, availability, duration, mut purchase)) = items.get_mut(payment.item) else {
            continue;
        };
        if *purchase != ResearchPurchaseState::AwaitingPayment(result.operation) {
            continue;
        }
        *purchase = ResearchPurchaseState::Idle;
        let Some(definitions) = definitions else {
            continue;
        };
        let Some(definition) = definitions.find_research(availability.item) else {
            continue;
        };
        let Ok(id) = ids.allocate(member.root) else {
            continue;
        };
        let hz = definitions.timing().fixed_hz;
        commands.entity(payment.item).insert((
            ResearchProject {
                definition: availability.item,
                elapsed_ticks: 0,
                required_ticks: duration.0.map_or_else(
                    || {
                        resolve_research_duration_to_simulation_ticks(
                            definition.duration,
                            hz,
                            *seed,
                            id,
                        )
                    },
                    |seconds| (seconds * hz as f32).max(1.0) as u64,
                ),
            },
            ResearchProjectPersistentId(id),
        ));
    }
}

pub(super) fn discard_rejected_research_payment_operations(
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<&PendingResearchProjectPayment>,
    mut items: Query<&mut ResearchPurchaseState>,
    mut commands: Commands,
) {
    for result in rejected.read() {
        let Ok(payment) = pending.get(result.operation) else {
            continue;
        };
        if let Ok(mut purchase) = items.get_mut(payment.item) {
            if *purchase == ResearchPurchaseState::AwaitingPayment(result.operation) {
                *purchase = ResearchPurchaseState::Idle;
            }
        }
        commands.entity(result.operation).try_despawn();
    }
}

pub(super) fn release_retired_research_payment(
    retired: On<Despawn<PendingResearchProjectPayment>>,
    payments: Query<&PendingResearchProjectPayment>,
    mut items: Query<&mut ResearchPurchaseState>,
) {
    let Ok(payment) = payments.get(retired.entity) else {
        return;
    };
    if let Ok(mut purchase) = items.get_mut(payment.item) {
        if *purchase == ResearchPurchaseState::AwaitingPayment(retired.entity) {
            *purchase = ResearchPurchaseState::Idle;
        }
    }
}

pub(super) fn apply_research_duration_overrides(
    mut requests: MessageReader<SetResearchDuration>,
    worlds: Query<&WorldResearchItems, With<WorldRoot>>,
    mut durations: Query<&mut ResearchDurationOverride>,
) {
    let Some(items) = worlds.iter().next() else {
        return;
    };
    for request in requests.read() {
        if let Some(&item) = items.definitions.get(&request.definition) {
            if let Ok(mut duration) = durations.get_mut(item) {
                duration.set_if_neq(ResearchDurationOverride(Some(request.seconds)));
            }
        }
    }
}

pub(super) fn advance_active_research_projects_by_one_tick(
    _clock: Res<ZooClock>,
    mut projects: Query<(Entity, &mut ResearchProject), Without<ResearchProjectPaused>>,
    mut ready: MessageWriter<ResearchProjectCompletionReady>,
) {
    for (entity, mut project) in &mut projects {
        if advance_research_project_by_one_tick(&mut project) {
            ready.write(ResearchProjectCompletionReady {
                project: entity,
                definition: project.definition,
            });
        }
    }
}

pub(super) fn advance_research_availability_unlock_countdowns(
    mut countdowns: Query<(Entity, &mut ResearchAvailabilityUnlockCountdown)>,
    mut commands: Commands,
) {
    for (entity, mut countdown) in &mut countdowns {
        countdown.remaining_ticks = countdown.remaining_ticks.saturating_sub(1);
        if countdown.remaining_ticks != 0 {
            continue;
        }
        // Expiry removes only the delay; the availability refresh still owns
        // the authored fame and game-mode requirements.
        commands
            .entity(entity)
            .remove::<ResearchAvailabilityUnlockCountdown>();
    }
}

pub(super) fn complete_research_projects_and_apply_authored_unlocks(
    mut ready: MessageReader<ResearchProjectCompletionReady>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut unlocks: ResMut<UnlockedCatalogueDefinitionSet>,
    projects: Query<&ResearchProject>,
    mut commands: Commands,
    mut completed: MessageWriter<ResearchProjectCompleted>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if ready.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for completion in ready.read() {
        let Ok(project) = projects.get(completion.project) else {
            continue;
        };
        if project.definition != completion.definition {
            continue;
        }
        let Some(research) = definitions.find_research(completion.definition) else {
            continue;
        };
        let targets = &research.unlocks;
        if !targets
            .iter()
            .all(|target| find_catalogue_definition_index(definitions, *target).is_some())
        {
            continue;
        }
        for target in targets {
            let id = *target;
            if let Some(index) = find_catalogue_definition_index(definitions, id) {
                if mark_catalogue_definition_unlocked_at_index(&mut unlocks, index) == Some(true) {
                    changed.write(ProgressionFactChanged::Unlock(id));
                }
            }
        }
        completed.write(ResearchProjectCompleted {
            project: completion.project,
            definition: completion.definition,
        });
        changed.write(ProgressionFactChanged::Research(completion.definition));
        commands.entity(completion.project).remove::<(
            ResearchProject,
            ResearchProjectPaused,
            ResearchProjectPersistentId,
        )>();
    }
}

pub(super) fn refresh_authored_research_availability(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    fame: Res<super::fame_types::Fame>,
    roots: Query<
        Ref<openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity>,
        With<WorldRoot>,
    >,
    mut availability: Query<(
        &mut ResearchProjectAvailability,
        Option<&ResearchAvailabilityUnlockCountdown>,
    )>,
    added: Query<
        (),
        Or<(
            Added<ResearchProjectAvailability>,
            Added<ResearchAvailabilityUnlockCountdown>,
        )>,
    >,
    mut expired: RemovedComponents<ResearchAvailabilityUnlockCountdown>,
    mut last_maximum: Local<Option<f32>>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let asset_changed = active_definitions.is_changed() || definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(identity) = roots.iter().next() else {
        return;
    };
    if !asset_changed
        && !identity.is_changed()
        && *last_maximum == fame.maximum_percent_reached
        && added.is_empty()
        && expired.is_empty()
    {
        return;
    }
    *last_maximum = fame.maximum_percent_reached;
    expired.clear();
    let research_mode = matches!(
        identity.mode,
        openzt2_world_state::game_session_types::WorldSessionMode::Campaign
            | openzt2_world_state::game_session_types::WorldSessionMode::Challenge
    );
    for (mut availability, countdown) in &mut availability {
        let available = research_mode
            && countdown.is_none()
            && definitions
                .find_research(availability.item)
                .is_some_and(|research| {
                    research.minimum_fame_percent == 0.0
                        || fame
                            .maximum_percent_reached
                            .is_some_and(|maximum| maximum >= research.minimum_fame_percent)
                });
        if availability.available != available {
            availability.available = available;
            changed.write(ProgressionFactChanged::Research(availability.item));
        }
    }
}
