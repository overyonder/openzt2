use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_game_data::world_definitions::staff_management::StaffRequestControllerDefinition;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::maintenance::maintenance_types::FacilityContainedWaste;
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Escaped;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::staff::staff_job_types::authored_staff_request_urgency;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
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

type AnimalRequestFacts = (
    Option<&'static Hunger>,
    Option<&'static Thirst>,
    Option<&'static HealthNeed>,
    Has<Escaped>,
    Has<Disease>,
);

/// The animal attributes that shipped staff-request controllers name.
#[derive(Clone, Copy)]
enum AnimalRequestAttribute {
    Hunger,
    Thirst,
    Health,
    Escaped,
    Diseased,
}

impl AnimalRequestAttribute {
    fn of(attribute: AssetId) -> Option<Self> {
        [
            ("hunger", Self::Hunger),
            ("thirst", Self::Thirst),
            ("health", Self::Health),
            ("b_escaped", Self::Escaped),
            ("b_disease", Self::Diseased),
        ]
        .into_iter()
        .find_map(|(key, kind)| (AssetId::from_key(key) == attribute).then_some(kind))
    }

    fn sample(
        self,
        (hunger, thirst, health, escaped, diseased): (
            Option<&Hunger>,
            Option<&Thirst>,
            Option<&HealthNeed>,
            bool,
            bool,
        ),
    ) -> Option<StaffRequestControllerSample> {
        let deprivation = |wellness: i32| {
            // Controllers use authored deprivation on 0..100, whereas the
            // welfare owner stores wellness on 0..1000 in Q16.
            StaffRequestControllerSample::NumberQ16(
                (MAX_NEED_Q16 - wellness.clamp(0, MAX_NEED_Q16)) / 10,
            )
        };
        match self {
            Self::Hunger => hunger.map(|need| deprivation(need.0)),
            Self::Thirst => thirst.map(|need| deprivation(need.0)),
            Self::Health => health.map(|need| deprivation(need.0)),
            Self::Escaped => Some(StaffRequestControllerSample::Boolean(escaped)),
            Self::Diseased => Some(StaffRequestControllerSample::Boolean(diseased)),
        }
    }
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

/// Evaluates authored container and animal care controllers against their
/// live owners. Unsupported attributes remain unsampled.
pub(super) fn evaluate_staff_request_rows(
    active_world_definitions: Res<WorldDefinitions>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut entities: Query<(
        Entity,
        &DefinitionId,
        &mut StaffRequestRuntimeBinding,
        Option<&FoodContainer>,
        Option<&DrinkContainer>,
        Option<&FacilityContainedWaste>,
    )>,
    animal_needs: Query<
        AnimalRequestFacts,
        (With<Animal>, Without<Dead>, Without<WorldEntityIsCrated>),
    >,
    jobs: Query<(Entity, &StaffJob)>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    for (entity, definition, mut binding, food, drink, waste) in &mut entities {
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
            let sample = if row.attribute_key == Some(food_level_key()) {
                sample.map(StaffRequestControllerSample::NumberQ16)
            } else if row.attribute_key == Some(AssetId::from_key("f_trashlevel")) {
                // The container's contained waste is its authored trash level.
                waste.map(|waste| {
                    StaffRequestControllerSample::NumberQ16(
                        i32::from(waste.contained_waste_units) << 16,
                    )
                })
            } else if row.attribute_key.is_none() {
                // A controller without an attribute raises on creation only.
                Some(StaffRequestControllerSample::NoValue)
            } else if let Some(attribute) = row.attribute_key.and_then(AnimalRequestAttribute::of) {
                let Some(sample) = animal_needs
                    .get(entity)
                    .ok()
                    .and_then(|animal| attribute.sample(animal))
                else {
                    // A dead, crated or no-longer-initialized animal cannot own
                    // care work. Resume with a fresh sample if it returns.
                    state.previous = None;
                    if let Some(token) = row.request.token {
                        if let Some(kind) = job_kind(token) {
                            cancel_matching_job(
                                StaffRequestJobKey {
                                    kind,
                                    target: entity,
                                    token,
                                },
                                &jobs,
                                &mut cancellations,
                            );
                        }
                    }
                    continue;
                };
                Some(sample)
            } else {
                None
            };
            let Some(sample) = sample else {
                continue;
            };
            if let Some(event) = evaluate_staff_request_row(row, state, entity, Some(sample)) {
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
            urgency: authored_staff_request_urgency(row.request.priority)?,
        })
    } else {
        Some(StaffRequestJobEvent::Cancelled { key })
    }
}

fn job_kind(token: AssetId) -> Option<StaffJobKind> {
    if token == fill_food_container_token() || token == AssetId::from_key("t_feedanimal") {
        Some(StaffJobKind::Feed)
    } else if token == AssetId::from_key("t_wateranimal") {
        Some(StaffJobKind::RefillWater)
    } else if token == AssetId::from_key("t_healanimal")
        || token == AssetId::from_key("t_cureanimal")
    {
        Some(StaffJobKind::Treat)
    } else if token == AssetId::from_key("t_crateanimal") {
        Some(StaffJobKind::Capture)
    } else if token == empty_trash_token() || token == empty_recycling_bin_token() {
        Some(StaffJobKind::EmptyBin)
    } else if token == AssetId::from_key("t_sweeptrash") {
        Some(StaffJobKind::SweepLitter)
    } else if token == AssetId::from_key("t_rakepoo") {
        Some(StaffJobKind::CleanHabitat)
    } else {
        None
    }
}
