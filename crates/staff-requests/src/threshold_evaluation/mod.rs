use bevy::prelude::*;
use openzt2_game_data::world_definitions::staff_management::StaffJobKind;
use openzt2_game_data::world_definitions::staff_management::StaffRequestControllerDefinition;
use openzt2_game_data::world_definitions::staff_management::StaffRequestThresholdValue;
use openzt2_game_data::AssetId;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::feeding::container_quantity::DrinkContainer;
use openzt2_zoo_state::feeding::container_quantity::FoodContainer;
use openzt2_zoo_state::maintenance::maintenance_types::{FacilityContainedWaste, LooseLitterWaste, AnimalHabitatWaste};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Disease;
use openzt2_zoo_state::animal_health::types::Escaped;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_welfare::types::HealthNeed;
use openzt2_zoo_state::animal_welfare::types::Hunger;
use openzt2_zoo_state::animal_welfare::types::HygieneNeed;
use openzt2_zoo_state::animal_welfare::types::StimulationNeed;
use openzt2_zoo_state::animal_welfare::types::Thirst;
use openzt2_zoo_state::animal_welfare::types::MAX_NEED_Q16;
use openzt2_world_state::world_spawn::world_entity_crating::WorldEntityIsCrated;
use openzt2_zoo_state::staff::staff_job_types::authored_staff_request_urgency;
use openzt2_zoo_state::staff::staff_job_types::StaffJob;
use openzt2_zoo_state::staff::staff_job_types::StaffJobRouting;
use openzt2_zoo_state::staff::staff_lifecycle_messages::CancelStaffJobRequest;
use openzt2_zoo_state::staff::staff_lifecycle_messages::StaffJobRequest;
use openzt2_world_state::world_spawn::world_membership_types::DefinitionId;
use super::staff_request_state_types::StaffRequestControllerSample;
use super::staff_request_state_types::StaffRequestRowIdentity;
use super::staff_request_state_types::StaffRequestRuntimeState;
use super::staff_request_state_types::StaffRequestThresholdState;
use super::staff_request_state_types::StaffRequestRuntimeBinding;
use super::staff_request_state_types::StaffRequestJobKey;
use super::staff_request_state_types::StaffRequestJobEvent;
use super::staff_request_threshold_comparison::staff_threshold_matches;

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
    Option<&'static HygieneNeed>,
    Option<&'static StimulationNeed>,
    Has<Escaped>,
    Has<Disease>,
);

/// The animal attributes that shipped staff-request controllers name.
#[derive(Clone, Copy)]
enum AnimalRequestAttribute {
    Hunger,
    Thirst,
    Health,
    Hygiene,
    Stimulation,
    Escaped,
    Diseased,
}

impl AnimalRequestAttribute {
    fn of(attribute: AssetId) -> Option<Self> {
        [
            ("hunger", Self::Hunger),
            ("thirst", Self::Thirst),
            ("health", Self::Health),
            ("hygiene", Self::Hygiene),
            ("stimulation", Self::Stimulation),
            ("b_escaped", Self::Escaped),
            ("b_disease", Self::Diseased),
        ]
        .into_iter()
        .find_map(|(key, kind)| (AssetId::from_key(key) == attribute).then_some(kind))
    }

    fn sample(
        self,
        (hunger, thirst, health, hygiene, stimulation, escaped, diseased): (
            Option<&Hunger>,
            Option<&Thirst>,
            Option<&HealthNeed>,
            Option<&HygieneNeed>,
            Option<&StimulationNeed>,
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
            Self::Hygiene => hygiene.map(|need| deprivation(need.0)),
            Self::Stimulation => stimulation.map(|need| deprivation(need.0)),
            Self::Escaped => Some(StaffRequestControllerSample::Boolean(escaped)),
            Self::Diseased => Some(StaffRequestControllerSample::Boolean(diseased)),
        }
    }
}

fn bind_rows(
    definition: AssetId,
    rows: &[StaffRequestControllerDefinition],
    kind_for_token: impl Fn(AssetId) -> Option<StaffJobKind>,
) -> Vec<StaffRequestRuntimeState> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.id == definition)
        .scan(
            std::collections::HashMap::new(),
            |ordinals, (row_index, row)| {
                let routing = StaffJobRouting {
                    subject_type: row.request.subject_type,
                    recipient: row.request.staff,
                };
                let key = (row.binder, row.attribute_key, row.request.token, routing);
                let ordinal = ordinals.entry(key).or_insert(0_u16);
                let state = StaffRequestRuntimeState {
                    row: StaffRequestRowIdentity {
                        definition,
                        binder: row.binder,
                        attribute_key: row.attribute_key,
                        token: row.request.token,
                        routing,
                        ordinal: *ordinal,
                    },
                    row_index,
                    job_kind: row.request.token.and_then(&kind_for_token),
                    previous: None,
                };
                *ordinal = ordinal.saturating_add(1);
                Some(state)
            },
        )
        .collect()
}

fn reconcile_rows(
    definition: AssetId,
    rows: &[StaffRequestControllerDefinition],
    previous: &[StaffRequestRuntimeState],
    kind_for_token: impl Fn(AssetId) -> Option<StaffJobKind>,
) -> Vec<StaffRequestRuntimeState> {
    bind_rows(definition, rows, kind_for_token)
        .into_iter()
        .map(|mut state| {
            state.previous = previous
                .iter()
                .find(|old| old.row == state.row && old.job_kind == state.job_kind)
                .and_then(|old| old.previous);
            state
        })
        .collect()
}

pub(super) fn bind_staff_request_rows_on_definition_change(
    active_world_definitions: Res<WorldDefinitions>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    mut commands: Commands,
    mut asset_events: MessageReader<AssetEvent<WorldDefinitionAsset>>,
    all_entities: Query<Entity, With<DefinitionId>>,
    changed_entities: Query<Entity, Changed<DefinitionId>>,
    mut entities: Query<(&DefinitionId, Option<&mut StaffRequestRuntimeBinding>)>,
    jobs: Query<(Entity, &StaffJob, Option<&StaffJobRouting>)>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    let assets_changed = asset_events
        .read()
        .filter(|event| {
            matches!(
                event,
                AssetEvent::Added { .. } | AssetEvent::Modified { .. } | AssetEvent::Removed { .. }
            )
        })
        .count()
        != 0;
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    let definitions_changed = active_world_definitions.is_changed() || assets_changed;
    let candidates = all_entities
        .iter()
        .take(if definitions_changed { usize::MAX } else { 0 })
        .chain(
            changed_entities
                .iter()
                .take(if definitions_changed { 0 } else { usize::MAX }),
        );
    let staff_manager = definitions.staff_manager();
    let kind_for_token = |token| {
        definitions
            .find_staff_job(token)
            .map(|job| job.kind)
            .or_else(|| {
                staff_manager
                    .is_none_or(|policy| {
                        policy
                            .tokens
                            .iter()
                            .any(|rule| rule.token == token && rule.name == Some(token))
                    })
                    .then(|| terminal_job_kind_for_token(token))
                    .flatten()
            })
    };
    for entity in candidates {
        let Ok((definition, binding)) = entities.get_mut(entity) else {
            continue;
        };
        let Some(rows) = definitions.staff_request_rows(definition.0) else {
            continue;
        };
        let previous = binding
            .as_deref()
            .map_or(&[][..], |binding| binding.rows.as_slice());
        let mut next = reconcile_rows(definition.0, rows, previous, kind_for_token);
        next.retain(|state| {
            state.row.token.is_some_and(|token| {
                staff_manager.is_none_or(|policy| {
                    policy
                        .tokens
                        .iter()
                        .any(|rule| rule.token == token && rule.name.is_some())
                })
            })
        });
        if let Some(mut binding) = binding {
            for old in &binding.rows {
                if next
                    .iter()
                    .any(|state| state.row == old.row && state.job_kind == old.job_kind)
                {
                    continue;
                }
                if let Some(key) = old.job_key(entity) {
                    cancel_staff_jobs(key, Some(key.routing), &jobs, &mut cancellations);
                }
            }
            if next.is_empty() {
                commands
                    .entity(entity)
                    .remove::<StaffRequestRuntimeBinding>();
            } else if next != binding.rows {
                binding.rows = next;
            }
        } else if !next.is_empty() {
            commands
                .entity(entity)
                .insert(StaffRequestRuntimeBinding { rows: next });
        }
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
        Option<&LooseLitterWaste>,
        Option<&AnimalHabitatWaste>,
        Option<&openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter>,
        Option<&openzt2_zoo_state::economy::facility_economy_types::MaintenanceScheduleIndex>,
    )>,
    animal_needs: Query<
        AnimalRequestFacts,
        (With<Animal>, Without<Dead>, Without<WorldEntityIsCrated>),
    >,
    jobs: Query<(Entity, &StaffJob, Option<&StaffJobRouting>)>,
    mut requests: MessageWriter<StaffJobRequest>,
    mut cancellations: MessageWriter<CancelStaffJobRequest>,
) {
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (
        entity,
        definition,
        mut binding,
        food,
        drink,
        waste,
        litter,
        animal_waste,
        filter,
        schedule,
    ) in &mut entities
    {
        let Some(rows) = definitions.staff_request_rows(definition.0) else {
            continue;
        };
        let sample = match (food, drink) {
            (Some(_), Some(_)) | (None, None) => None,
            (Some(food), None) => Some(food.amount_q16),
            (None, Some(drink)) => Some(drink.amount_q16),
        };
        for state in &mut binding.rows {
            let Some(row) = rows.get(state.row_index) else {
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
            } else if row.attribute_key == Some(AssetId::from_key("f_filterdirtylevel")) {
                filter.map(|filter| {
                    StaffRequestControllerSample::NumberQ16((filter.dirt_percent * 65536.0) as i32)
                })
            } else if row.attribute_key.is_none() {
                // A controller without an attribute raises on creation only.
                if litter.is_some_and(|litter| litter.uncontained_waste_units == 0)
                    || animal_waste.is_some_and(|waste| !waste.is_valid())
                {
                    None
                } else {
                    Some(StaffRequestControllerSample::NoValue)
                }
            } else if let Some(attribute) = row.attribute_key.and_then(AnimalRequestAttribute::of) {
                let Some(sample) = animal_needs
                    .get(entity)
                    .ok()
                    .and_then(|animal| attribute.sample(animal))
                else {
                    // A dead, crated or no-longer-initialized animal cannot own
                    // care work. Resume with a fresh sample if it returns.
                    state.previous = None;
                    if let Some(key) = state.job_key(entity) {
                        cancel_staff_jobs(key, None, &jobs, &mut cancellations);
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
            let scheduled = filter.and_then(|filter| filter_schedule_row(row, filter, schedule));
            if let Some(event) = evaluate_staff_request_row(
                scheduled.as_ref().unwrap_or(row),
                state,
                entity,
                Some(sample),
            ) {
                match event {
                    StaffRequestJobEvent::Raised { key, urgency } => {
                        requests.write(StaffJobRequest {
                            kind: state.job_kind,
                            target: key.target,
                            urgency,
                            token: Some(key.token),
                            routing: key.routing,
                        });
                    }
                    StaffRequestJobEvent::Cancelled { key } => {
                        cancel_staff_jobs(key, None, &jobs, &mut cancellations);
                    }
                }
            }
        }
    }
}

/// A filter's cleaning request with the thresholds of the schedule chosen in
/// its panel, which default to regular as the panel does.
fn filter_schedule_row(
    row: &StaffRequestControllerDefinition,
    filter: &openzt2_zoo_state::aquatic::aquatic_simulation_types::WaterFilter,
    schedule: Option<&openzt2_zoo_state::economy::facility_economy_types::MaintenanceScheduleIndex>,
) -> Option<StaffRequestControllerDefinition> {
    if row.attribute_key != Some(AssetId::from_key("f_filterdirtylevel")) {
        return None;
    }
    let thresholds = filter.policy.maintenance_thresholds?;
    let [threshold, cancel] = *thresholds.get(usize::from(schedule.map_or(1, |index| index.0)))?;
    let mut row = row.clone();
    row.threshold = StaffRequestThresholdValue::Number(threshold);
    row.cancel_threshold = StaffRequestThresholdValue::Number(cancel);
    Some(row)
}

impl StaffRequestRuntimeState {
    fn job_key(&self, owner: Entity) -> Option<StaffRequestJobKey> {
        self.row.routing.subject_type?;
        // Binding a controller sets the payload target to the watched entity.
        Some(StaffRequestJobKey {
            target: owner,
            token: self.row.token?,
            routing: self.row.routing,
        })
    }
}

fn cancel_staff_jobs(
    key: StaffRequestJobKey,
    required_routing: Option<StaffJobRouting>,
    jobs: &Query<(Entity, &StaffJob, Option<&StaffJobRouting>)>,
    cancellations: &mut MessageWriter<CancelStaffJobRequest>,
) {
    for (entity, job, routing) in jobs.iter() {
        if job.target == key.target
            && job.token == Some(key.token)
            && required_routing
                .is_none_or(|required| routing.copied().unwrap_or_default() == required)
        {
            cancellations.write(CancelStaffJobRequest { job: entity });
        }
    }
}

fn evaluate_staff_request_row(
    row: &StaffRequestControllerDefinition,
    state: &mut StaffRequestRuntimeState,
    target: Entity,
    current: Option<StaffRequestControllerSample>,
) -> Option<StaffRequestJobEvent> {
    let current = current.map(|sample| StaffRequestThresholdState {
        trigger: staff_threshold_matches(row.threshold_comparison, row.threshold, sample),
        cancel: staff_threshold_matches(row.cancel_comparison, row.cancel_threshold, sample),
    });
    // Keep each result under the thresholds in effect when it was sampled.
    let previous = state.previous;
    state.previous = current.or_else(|| {
        row.attribute_key
            .is_none()
            .then(StaffRequestThresholdState::default)
    });
    let trigger_on_creation = previous.is_none() && row.trigger_on_creation;
    let previous = previous.unwrap_or_default();
    let current = current.unwrap_or_default();
    let event = if trigger_on_creation || (current.trigger && !previous.trigger) {
        true
    } else if current.cancel && !previous.cancel {
        false
    } else {
        return None;
    };
    let key = state.job_key(target)?;
    if event {
        Some(StaffRequestJobEvent::Raised {
            key,
            urgency: authored_staff_request_urgency(row.request.priority)?,
        })
    } else {
        Some(StaffRequestJobEvent::Cancelled { key })
    }
}

fn terminal_job_kind_for_token(token: AssetId) -> Option<StaffJobKind> {
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
    } else if token == AssetId::from_key("t_cleanwaterfilter") {
        Some(StaffJobKind::MaintainTank)
    } else {
        None
    }
}
