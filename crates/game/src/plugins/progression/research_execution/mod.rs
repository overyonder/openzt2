use super::research_duration_calculation::resolve_research_duration_to_simulation_ticks;
use crate::plugins::simulation_time::deterministic_random_stream::ZooSeed;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::economy::account_transaction_types::Account;
use crate::plugins::economy::account_transaction_types::TransactionCompleted;
use crate::plugins::economy::account_transaction_types::TransactionKind;
use crate::plugins::economy::account_transaction_types::TransactionRejected;
use crate::plugins::economy::account_transaction_types::TransactionRequest;
use crate::plugins::economy::money_types::Money;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use crate::plugins::world_spawn::persistent_id_types::PersistentIdAllocator;
use crate::plugins::world_spawn::world_membership_types::WorldMember;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_set_operations::{
        catalogue_definition_is_unlocked_at_index, mark_catalogue_definition_unlocked_at_index,
    },
    research_progress_calculations::advance_research_project_by_one_tick,
    research_types::{
        PendingResearchProjectPayment, ResearchAvailabilityUnlockCountdown, ResearchProject,
        ResearchProjectAvailability, ResearchProjectCompleted, ResearchProjectCompletionReady,
        ResearchProjectPaused, StartResearchProjectRequest,
    },
    unlock_types::UnlockedCatalogueDefinitionSet,
};

pub(super) fn validate_start_research_requests_and_request_payments(
    mut requests: MessageReader<StartResearchProjectRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    projects: Query<&ResearchProject>,
    availability: Query<&ResearchProjectAvailability>,
    pending: Query<&PendingResearchProjectPayment>,
    roots: Query<(Entity, &WorldRoot)>,
    mut commands: Commands,
    mut transactions: MessageWriter<TransactionRequest>,
    mut accepted: Local<Vec<AssetId>>,
) {
    if requests.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some((root, _)) = roots.iter().next() else {
        return;
    };
    accepted.clear();
    let definition_count = definitions.research().count();
    if accepted.capacity() < definition_count {
        let additional = definition_count - accepted.capacity();
        accepted.reserve_exact(additional);
    }
    for request in requests.read() {
        let Some(research) = definitions.find_research(request.definition) else {
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
        if accepted.contains(&request.definition)
            || availability
                .iter()
                .find(|available| available.item == request.definition)
                .is_none_or(|available| !available.available)
            || projects
                .iter()
                .any(|active| active.definition == request.definition)
            || pending
                .iter()
                .any(|active| active.definition == request.definition)
            || !research.prerequisites.iter().all(|id| {
                find_catalogue_definition_index(definitions, *id)
                    .is_some_and(|index| catalogue_definition_is_unlocked_at_index(&unlocks, index))
            })
            || invalid_target
            || already_completed
        {
            continue;
        }
        accepted.push(request.definition);
        let operation = commands
            .spawn((
                PendingResearchProjectPayment {
                    definition: request.definition,
                },
                WorldMember { root },
            ))
            .id();
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
    pending: Query<(Entity, &PendingResearchProjectPayment, &WorldMember)>,
    mut ids: ResMut<PersistentIdAllocator>,
    seed: Res<ZooSeed>,
    mut commands: Commands,
) {
    if completed.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for result in completed.read() {
        let Ok((operation, payment, member)) = pending.get(result.operation) else {
            continue;
        };
        let Some(definition) = definitions.find_research(payment.definition) else {
            // The transaction is already committed. Never retain a dead
            // payment operation that could be mistaken for an in-flight
            // purchase if the loaded catalogue was replaced unexpectedly.
            commands.entity(operation).despawn();
            continue;
        };
        let Ok(id) = ids.allocate(member.root) else {
            commands.entity(operation).despawn();
            continue;
        };
        commands.spawn((
            ResearchProject {
                definition: payment.definition,
                elapsed_ticks: 0,
                required_ticks: resolve_research_duration_to_simulation_ticks(
                    definition.duration,
                    definitions.timing().fixed_hz,
                    *seed,
                    id,
                ),
            },
            *member,
            id,
        ));
        commands.entity(operation).despawn();
    }
}

pub(super) fn discard_rejected_research_payment_operations(
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<(), With<PendingResearchProjectPayment>>,
    mut commands: Commands,
) {
    for result in rejected.read() {
        if pending.get(result.operation).is_ok() {
            commands.entity(result.operation).despawn();
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
    mut countdowns: Query<(
        Entity,
        &mut ResearchAvailabilityUnlockCountdown,
        &mut ResearchProjectAvailability,
    )>,
    mut commands: Commands,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    for (entity, mut countdown, mut availability) in &mut countdowns {
        countdown.remaining_ticks = countdown.remaining_ticks.saturating_sub(1);
        if countdown.remaining_ticks != 0 {
            continue;
        }
        availability.available = true;
        changed.write(ProgressionFactChanged::Research(countdown.item));
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
        commands.entity(completion.project).despawn();
    }
}

pub(super) fn refresh_authored_research_availability(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    fame: Res<super::fame_types::Fame>,
    roots: Query<
        &crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity,
        With<WorldRoot>,
    >,
    mut availability: Query<(
        &mut ResearchProjectAvailability,
        Option<&ResearchAvailabilityUnlockCountdown>,
    )>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let research_mode = roots.iter().next().is_some_and(|identity| {
        matches!(
            identity.mode,
            crate::game_session_types::WorldSessionMode::Campaign
                | crate::game_session_types::WorldSessionMode::Challenge
        )
    });
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
        }
    }
}
