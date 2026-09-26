use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_game_data::world_definitions::staff_management::StaffRequestControllerDefinition;
use openzt2_game_data::AssetId;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::feeding::container_quantity::DrinkContainer;
use crate::plugins::feeding::container_quantity::FoodContainer;
use crate::plugins::staff::staff_job_types::StaffJob;
use crate::plugins::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use crate::plugins::staff::staff_lifecycle_messages::StaffJobRequest;
use crate::plugins::world_spawn::world_membership_types::DefinitionId;
use super::staff_request_state_types::StaffRequestControllerSample;
use super::staff_request_state_types::StaffRequestRowIdentity;
use super::staff_request_state_types::StaffRequestRuntimeState;
use super::staff_request_state_types::StaffRequestRuntimeBinding;
use super::staff_request_state_types::StaffRequestBindingPending;
use super::staff_request_state_types::StaffRequestJobKey;
use super::staff_request_state_types::StaffRequestJobEvent;
use super::staff_request_threshold_comparison::evaluate_staff_threshold;

#[cfg(test)]
mod tests;

fn food_level_key() -> AssetId {
    AssetId::from_key("f_foodlevel")
}
fn fill_food_container_token() -> AssetId {
    AssetId::from_key("t_fillfoodcontainer")
}
fn empty_trash_token() -> AssetId {
    AssetId::from_key("t_emptytrash")
}
fn empty_recycling_bin_token() -> AssetId {
    AssetId::from_key("t_emptyrecyclingbin")
}

fn canonical_rows<'a>(
    active_world_definitions: &'a WorldDefinitions,
    world_definition_assets: &'a Assets<WorldDefinitionAsset>,
    definition: AssetId,
) -> Option<&'a [StaffRequestControllerDefinition]> {
    active_world_definitions
        .get(world_definition_assets)?
        .staff_request_rows(definition)
}

fn bind_rows(
    definition: AssetId,
    rows: &[StaffRequestControllerDefinition],
) -> Vec<StaffRequestRuntimeState> {
    rows.iter()
        .filter(|row| row.id == definition)
        .scan(std::collections::HashMap::<(Option<AssetId>, Option<AssetId>, Option<AssetId>), u16>::new(), |ordinals, row| {
            let key = (row.binder, row.attribute_key, row.request.token);
            let ordinal = ordinals.entry(key).or_insert(0);
            let state = StaffRequestRuntimeState {
                row: StaffRequestRowIdentity {
                    definition,
                    binder: row.binder,
                    attribute_key: row.attribute_key,
                    token: row.request.token,
                    ordinal: *ordinal,
                },
                previous: None,
            };
            *ordinal = ordinal.saturating_add(1);
            Some(state)
        })
        .collect()
}

fn reconcile_rows(
    definition: AssetId,
    rows: &[StaffRequestControllerDefinition],
    previous: &[StaffRequestRuntimeState],
) -> Vec<StaffRequestRuntimeState> {
    bind_rows(definition, rows)
        .into_iter()
        .map(|mut state| {
            state.previous = previous
                .iter()
                .find(|old| old.row == state.row)
                .and_then(|old| old.previous);
            state
        })
        .collect()
}

/// Binds only newly spawned or definition-changed entities. Pending asset
/// resolution leaves the entity marked pending and retries without inventing
/// an empty controller set.
pub(super) fn bind_staff_request_rows_on_definition_change(
    active_world_definitions: Res<WorldDefinitions>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
    entities: Query<
        (Entity, &DefinitionId, Option<&StaffRequestRuntimeBinding>),
        Or<(
            Added<DefinitionId>,
            Changed<DefinitionId>,
            With<StaffRequestBindingPending>,
        )>,
    >,
    jobs: Query<(Entity, &StaffJob)>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for (entity, definition, previous) in &entities {
        let Some(rows) = canonical_rows(
            &active_world_definitions,
            &world_definition_assets,
            definition.0,
        ) else {
            commands.entity(entity).insert(StaffRequestBindingPending);
            continue;
        };
        let next = previous.map_or_else(
            || bind_rows(definition.0, rows),
            |previous| reconcile_rows(definition.0, rows, &previous.rows),
        );
        if let Some(previous) = previous {
            cancel_removed_rows(entity, &previous.rows, &next, &jobs, &mut cancellations);
        }
        commands
            .entity(entity)
            .insert(StaffRequestRuntimeBinding {
                revision: active_world_definitions.catalogue_revision(),
                rows: next,
            })
            .remove::<StaffRequestBindingPending>();
    }
}

/// Reconciles once when the canonical definition revision changes. Removed
/// rows cancel their exact existing jobs before their primitive state is
/// discarded; retained rows preserve only their previous samples.
pub(super) fn reconcile_staff_request_rows_on_definition_revision(
    active_world_definitions: Res<WorldDefinitions>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
    entities: Query<(Entity, &DefinitionId, &StaffRequestRuntimeBinding)>,
    jobs: Query<(Entity, &StaffJob)>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
    mut last_revision: Local<Option<u64>>,
) {
    let revision = active_world_definitions.catalogue_revision();
    if *last_revision == Some(revision) {
        return;
    }
    if entities.iter().any(|(_, definition, _)| {
        canonical_rows(
            &active_world_definitions,
            &world_definition_assets,
            definition.0,
        )
        .is_none()
    }) {
        return;
    }
    *last_revision = Some(revision);
    for (entity, definition, binding) in &entities {
        if binding.revision == revision {
            continue;
        }
        let Some(rows) = canonical_rows(
            &active_world_definitions,
            &world_definition_assets,
            definition.0,
        ) else {
            continue;
        };
        let next = reconcile_rows(definition.0, rows, &binding.rows);
        cancel_removed_rows(entity, &binding.rows, &next, &jobs, &mut cancellations);
        commands
            .entity(entity)
            .insert(StaffRequestRuntimeBinding {
                revision,
                rows: next,
            })
            .remove::<StaffRequestBindingPending>();
    }
}

/// Evaluates requests against food and drink container levels. Trash-level
/// requests have no live sample until their units are known.
pub(super) fn evaluate_staff_request_rows(
    active_world_definitions: Res<WorldDefinitions>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut entities: Query<(
        Entity,
        &DefinitionId,
        &mut StaffRequestRuntimeBinding,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
    )>,
    jobs: Query<(Entity, &StaffJob)>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for (entity, definition, mut binding, food, drink) in &mut entities {
        let Some(rows) = canonical_rows(
            &active_world_definitions,
            &world_definition_assets,
            definition.0,
        ) else {
            continue;
        };
        let sample = match (food, drink) {
            (Some(_), Some(_)) | (None, None) => None,
            (Some(food), None) => Some(food.amount_q16),
            (None, Some(drink)) => Some(drink.amount_q16),
        };
        for state in &mut binding.rows {
            let Some(row) = find_row(rows, state.row) else {
                continue;
            };
            if row.attribute_key != Some(food_level_key()) {
                continue;
            }
            let Some(sample) = sample else {
                continue;
            };
            if let Some(event) = evaluate_staff_request_row(
                row,
                state,
                entity,
                Some(StaffRequestControllerSample::NumberQ16(sample)),
            ) {
                emit_job_event(event, &jobs, &mut requests, &mut cancellations);
            }
        }
    }
}

fn find_row<'a>(
    rows: &'a [StaffRequestControllerDefinition],
    identity: StaffRequestRowIdentity,
) -> Option<&'a StaffRequestControllerDefinition> {
    rows.iter()
        .filter(|row| {
            row.id == identity.definition
                && row.binder == identity.binder
                && row.attribute_key == identity.attribute_key
                && row.request.token == identity.token
        })
        .nth(identity.ordinal as usize)
}

fn cancel_removed_rows(
    target: Entity,
    previous: &[StaffRequestRuntimeState],
    next: &[StaffRequestRuntimeState],
    jobs: &Query<(Entity, &StaffJob)>,
    cancellations: &mut MessageWriter<CancelStaffJobRequest>,
) {
    for old in previous {
        if next.iter().any(|state| state.row == old.row) {
            continue;
        }
        let Some(token) = old.row.token else {
            continue;
        };
        let Some(kind) = job_kind(token) else {
            continue;
        };
        cancel_matching_job(
            StaffRequestJobKey {
                kind,
                target,
                token,
            },
            jobs,
            cancellations,
        );
    }
}

fn emit_job_event(
    event: StaffRequestJobEvent,
    jobs: &Query<(Entity, &StaffJob)>,
    requests: &mut MessageWriter<StaffJobRequest>,
    cancellations: &mut MessageWriter<CancelStaffJobRequest>,
) {
    match event {
        StaffRequestJobEvent::Raised { key, urgency } => {
            requests.write(StaffJobRequest {
                kind: key.kind,
                target: key.target,
                urgency,
                token: Some(key.token),
            });
        }
        StaffRequestJobEvent::Cancelled { key } => {
            cancel_matching_job(key, jobs, cancellations);
        }
    }
}

fn cancel_matching_job(
    key: StaffRequestJobKey,
    jobs: &Query<(Entity, &StaffJob)>,
    cancellations: &mut MessageWriter<CancelStaffJobRequest>,
) {
    for (job_entity, job) in jobs.iter() {
        if job.kind == key.kind && job.target == key.target && job.token == Some(key.token) {
            cancellations.write(CancelStaffJobRequest { job: job_entity });
        }
    }
}

fn evaluate_staff_request_row(
    row: &StaffRequestControllerDefinition,
    state: &mut StaffRequestRuntimeState,
    target: Entity,
    current: Option<StaffRequestControllerSample>,
) -> Option<StaffRequestJobEvent> {
    let previous = state.previous;
    state.previous = current.or_else(|| {
        row.attribute_key
            .is_none()
            .then_some(StaffRequestControllerSample::NoValue)
    });
    let token = row.request.token?;
    let trigger_result = current.map(|sample| {
        evaluate_staff_threshold(row.threshold_comparison, row.threshold, previous, sample)
    });
    let cancel_result = current.map(|sample| {
        evaluate_staff_threshold(
            row.cancel_comparison,
            row.cancel_threshold,
            previous,
            sample,
        )
    });
    let event = if previous.is_none() && row.trigger_on_creation {
        true
    } else if trigger_result == Some(1) {
        true
    } else if cancel_result == Some(1) {
        false
    } else {
        return None;
    };
    let key = StaffRequestJobKey {
        kind: job_kind(token)?,
        target,
        token,
    };
    if event {
        Some(StaffRequestJobEvent::Raised {
            key,
            urgency: priority_to_urgency(row.request.priority),
        })
    } else {
        Some(StaffRequestJobEvent::Cancelled { key })
    }
}

fn job_kind(token: AssetId) -> Option<StaffJobKind> {
    if token == fill_food_container_token() {
        Some(StaffJobKind::Feed)
    } else if token == empty_trash_token() || token == empty_recycling_bin_token() {
        Some(StaffJobKind::EmptyBin)
    } else {
        None
    }
}

fn priority_to_urgency(priority: f32) -> u16 {
    // Winning x300/x301 authored request priorities are integers or half
    // integers. The existing StaffJob urgency is u16, so preserve that entire
    // shipped ordering exactly in half-priority units instead of rounding 1.5
    // to the same urgency as 2.0.
    if !priority.is_finite() || priority <= 0.0 {
        0
    } else if priority >= f32::from(u16::MAX) / 2.0 {
        u16::MAX
    } else {
        (priority * 2.0).round() as u16
    }
}
