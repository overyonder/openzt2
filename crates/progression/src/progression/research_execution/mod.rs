use std::time::Duration;

use super::research_duration_calculation::resolve_research_duration;
use bevy::ecs::{entity::EntityHashSet, relationship::RelationshipTarget};
use bevy::prelude::*;
use openzt2_world_state::game_session_types::WorldSessionMode;
use openzt2_world_state::simulation_time::deterministic_random_stream::ZooSeed;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::economy::account_transaction_types::Account;
use openzt2_zoo_state::economy::account_transaction_types::TransactionCompleted;
use openzt2_zoo_state::economy::account_transaction_types::{TransactionDomain, TransactionKind};
use openzt2_zoo_state::economy::account_transaction_types::TransactionRejected;
use openzt2_zoo_state::economy::account_transaction_types::TransactionRequest;
use openzt2_zoo_state::economy::money_types::Money;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentIdAllocator;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::{
    award_and_progression_fact_types::ProgressionFactChanged,
    catalogue_definition_index_queries::find_catalogue_definition_index,
    catalogue_unlock_application::apply_catalogue_unlock_targets,
    catalogue_unlock_set_operations::catalogue_definition_is_unlocked_at_index,
    research_types::{
        PendingResearchProjectPayment, ResearchAvailabilityUnlockCountdown,
        ResearchDurationOverride, ResearchItemPayments, ResearchProject,
        ResearchProjectAvailability, ResearchProjectCompleted, ResearchProjectPaused,
        ResearchProjectPersistentId, SetResearchDuration, StartResearchProjectRequest,
        WorldResearchItems,
    },
    unlock_types::UnlockedCatalogueDefinitionSet,
};

#[derive(Component)]
pub(super) struct ReadyResearchProject;

pub(super) fn validate_start_research_requests_and_request_payments(
    mut requests: MessageReader<StartResearchProjectRequest>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    root_unlocks: Query<&UnlockedCatalogueDefinitionSet>,
    items: Query<
        (&WorldMember, &ResearchProjectAvailability),
        (Without<ResearchProject>, Without<ResearchItemPayments>),
    >,
    mut commands: Commands,
    mut transactions: MessageWriter<TransactionRequest>,
) {
    if requests.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut accepted_items = EntityHashSet::default();
    for request in requests.read() {
        let Ok((member, availability)) = items.get(request.item) else {
            continue;
        };
        let Ok(unlocks) = root_unlocks.get(member.root) else {
            continue;
        };
        if !availability.available || accepted_items.contains(&request.item) {
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
                    .is_some_and(|index| catalogue_definition_is_unlocked_at_index(unlocks, index))
            });
        if !research.prerequisites.iter().all(|id| {
            find_catalogue_definition_index(definitions, *id)
                .is_some_and(|index| catalogue_definition_is_unlocked_at_index(unlocks, index))
        }) || invalid_target
            || already_completed
        {
            continue;
        }
        accepted_items.insert(request.item);
        let operation = commands
            .spawn((
                PendingResearchProjectPayment {
                    item: request.item,
                },
                *member,
                openzt2_zoo_state::economy::general_economy_types::GeneralEconomyTransaction::Research,
            ))
            .id();
        transactions.write(TransactionRequest {
            domain: TransactionDomain::Player,
            operation,
            debit: Account::Zoo,
            credit: Account::External,
            amount: Money::from_cents(research.cost_cents),
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
    items: Query<(
        &WorldMember,
        &ResearchProjectAvailability,
        &ResearchDurationOverride,
        &ResearchItemPayments,
    )>,
    mut ids: Query<&mut PersistentIdAllocator, With<WorldRoot>>,
    seeds: Query<&ZooSeed>,
    time: Res<Time<Fixed>>,
    mut commands: Commands,
) {
    let definitions = active_definitions.get(&definitions);
    let mut consumed = EntityHashSet::default();
    for result in completed.read().filter(|result| {
        result.domain == TransactionDomain::Player && result.kind == TransactionKind::Research
    }) {
        let Ok(payment) = pending.get(result.operation) else {
            continue;
        };
        if !consumed.insert(result.operation) {
            continue;
        }
        commands.entity(result.operation).try_despawn();
        let Ok((member, availability, duration, current_payment)) = items.get(payment.item) else {
            continue;
        };
        if current_payment.pending_payment() != result.operation {
            continue;
        }
        let Some(definitions) = definitions else {
            continue;
        };
        let Some(definition) = definitions.find_research(availability.item) else {
            continue;
        };
        let Ok(seed) = seeds.get(member.root) else {
            continue;
        };
        let Ok(mut ids) = ids.get_mut(member.root) else {
            continue;
        };
        let Ok(id) = ids.allocate() else {
            continue;
        };
        let step = time.timestep();
        let required = duration
            .0
            .and_then(|seconds| Duration::try_from_secs_f32(seconds).ok())
            .unwrap_or_else(|| resolve_research_duration(definition.duration, step, *seed, id))
            .max(step);
        commands.entity(payment.item).insert((
            ResearchProject::new(availability.item, required),
            ResearchProjectPersistentId(id),
        ));
    }
}

pub(super) fn discard_rejected_research_payment_operations(
    mut rejected: MessageReader<TransactionRejected>,
    pending: Query<&PendingResearchProjectPayment>,
    mut commands: Commands,
) {
    for result in rejected.read().filter(|result| {
        result.domain == TransactionDomain::Player && result.kind == TransactionKind::Research
    }) {
        if pending.contains(result.operation) {
            commands.entity(result.operation).try_despawn();
        }
    }
}

pub(super) fn apply_research_duration_overrides(
    mut requests: MessageReader<SetResearchDuration>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    worlds: Query<(Entity, &WorldResearchItems), With<WorldRoot>>,
    mut durations: Query<&mut ResearchDurationOverride>,
) {
    let Some((_, items)) = active_world
        .single()
        .ok()
        .and_then(|world| worlds.get(world).ok())
    else {
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

#[derive(Component, Default, PartialEq, Eq)]
pub struct ResearchCompletionBasisPoints(pub i64);

fn research_progress(timer: &Timer) -> ResearchCompletionBasisPoints {
    let duration = timer.duration().as_nanos();
    let elapsed = timer.elapsed().as_nanos().min(duration);
    ResearchCompletionBasisPoints(((elapsed * 10_000) / duration.max(1)) as i64)
}

pub(super) fn publish_restored_research_progress(
    mut projects: Query<
        (&ResearchProject, &mut ResearchCompletionBasisPoints),
        Added<ResearchProject>,
    >,
) {
    for (project, mut progress) in &mut projects {
        progress.set_if_neq(research_progress(&project.timer));
    }
}

pub(super) fn advance_active_research_projects(
    time: Res<Time<Fixed>>,
    mut projects: Query<
        (
            Entity,
            &mut ResearchProject,
            &mut ResearchCompletionBasisPoints,
        ),
        (
            Without<ResearchProjectPaused>,
            Without<ReadyResearchProject>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, mut project, mut progress) in &mut projects {
        project.timer.tick(time.timestep());
        progress.set_if_neq(research_progress(&project.timer));
        if project.timer.is_finished() {
            commands.entity(entity).insert(ReadyResearchProject);
        }
    }
}

pub(super) fn advance_research_availability_unlock_countdowns(
    time: Res<Time<Fixed>>,
    mut countdowns: Query<(Entity, &mut ResearchAvailabilityUnlockCountdown)>,
    mut commands: Commands,
) {
    for (entity, mut countdown) in &mut countdowns {
        if countdown.timer.tick(time.timestep()).is_finished() {
            commands
                .entity(entity)
                .remove::<ResearchAvailabilityUnlockCountdown>();
        }
    }
}

pub(super) fn complete_research_projects_and_apply_authored_unlocks(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut root_unlocks: Query<&mut UnlockedCatalogueDefinitionSet>,
    projects: Query<(Entity, &ResearchProject, &WorldMember), With<ReadyResearchProject>>,
    mut commands: Commands,
    mut completed: MessageWriter<ResearchProjectCompleted>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    if projects.is_empty() {
        return;
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (entity, project, member) in &projects {
        let Some(research) = definitions.find_research(project.definition) else {
            continue;
        };
        let Ok(mut unlocks) = root_unlocks.get_mut(member.root) else {
            continue;
        };
        if !apply_catalogue_unlock_targets(
            definitions,
            &research.unlocks,
            &mut unlocks,
            &mut changed,
        ) {
            continue;
        }
        completed.write(ResearchProjectCompleted {
            root: member.root,
            project: entity,
            definition: project.definition,
        });
        changed.write(ProgressionFactChanged::Research(project.definition));
        commands.entity(entity).remove::<(
            ResearchProject,
            ResearchProjectPaused,
            ResearchProjectPersistentId,
            ReadyResearchProject,
            ResearchCompletionBasisPoints,
        )>();
    }
}

pub(super) fn refresh_authored_research_availability(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    roots: Query<
        (
            Ref<openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity>,
            Ref<super::fame_types::ZooMaximumFame>,
            Option<&super::research_types::WorldResearchItemMembers>,
        ),
        With<WorldRoot>,
    >,
    mut availability: ParamSet<(
        Query<(
            &WorldMember,
            &mut ResearchProjectAvailability,
            Option<&ResearchAvailabilityUnlockCountdown>,
        )>,
        Query<
            Entity,
            (
                With<ResearchProjectAvailability>,
                Or<(
                    Added<ResearchProjectAvailability>,
                    Added<ResearchAvailabilityUnlockCountdown>,
                    Changed<WorldMember>,
                )>,
            ),
        >,
    )>,
    mut expired: RemovedComponents<ResearchAvailabilityUnlockCountdown>,
    mut changed: MessageWriter<ProgressionFactChanged>,
) {
    let assets_changed = active_definitions.is_changed() || definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let mut dirty = Vec::new();
    for (identity, maximum, members) in &roots {
        if assets_changed || identity.is_changed() || maximum.is_changed() {
            dirty.extend(members.into_iter().flat_map(RelationshipTarget::iter));
        }
    }
    dirty.extend(availability.p1().iter().chain(expired.read()));
    let mut seen = EntityHashSet::default();
    dirty.retain(|&entity| seen.insert(entity));
    let mut availability = availability.p0();
    let mut dirty = availability.iter_many_mut(dirty);
    while let Some(row) = dirty.fetch_next() {
        let Ok((member, mut availability, countdown)) = row else {
            continue;
        };
        let Ok((identity, maximum, _)) = roots.get(member.root) else {
            continue;
        };
        let supported_mode = matches!(
            identity.mode,
            WorldSessionMode::Campaign | WorldSessionMode::Challenge
        );
        let available = supported_mode
            && countdown.is_none()
            && definitions
                .find_research(availability.item)
                .is_some_and(|research| {
                    research.minimum_fame_percent == 0.0
                        || maximum
                            .0
                            .is_some_and(|maximum| maximum >= research.minimum_fame_percent)
                });
        if availability.available != available {
            availability.available = available;
            changed.write(ProgressionFactChanged::Research(availability.item));
        }
    }
}
