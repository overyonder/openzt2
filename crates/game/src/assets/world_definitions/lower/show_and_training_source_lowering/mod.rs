use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, asset_list, id, number_or, required_number, simple_error,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::world_definitions::behavior_auxiliary_source_types::LoweredBehaviorAuxiliary;
use openzt2_game_data::world_definitions::animal_shows_and_training::{
    ShowRuleDefinition, ShowStageDefinition, TrickDefinition, TrickOutcomeTokens,
};
use openzt2_game_data::world_definitions::extinct_animal_recovery::FossilPlacementPolicy;
use openzt2_game_data::world_definitions::UNAUTHORED_TRICK_DISPLAY_ORDER;
use openzt2_game_data::AssetId;

pub(super) fn bind_show_stage(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let tricks = record
        .value(&["supportedTricks", "tricks"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.show_stages.push(ShowStageDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity"]),
        performer_slots: required_number(record, &["performerSlots", "participantSlots"])?,
        audience_capacity: required_number(record, &["audienceCapacity"])?,
        schedule_slots: required_number(record, &["scheduleSlots"])?,
        supported_tricks: tricks,
        admission_cents: number_or(record, &["admissionCents", "admission"], 0)?,
    });
    Ok(())
}

pub(super) fn bind_trick(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let species = record
        .value(&["species", "eligibleSpecies"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.tricks.push(TrickDefinition {
        id: id(record.key),
        species,
        animation: asset(record, &["animation", "behavior"]),
        training_ticks: required_number(record, &["trainingTicks"])?,
        difficulty: required_number(record, &["difficulty"])?,
        welfare_cost: number_or(record, &["welfareCost"], 0)?,
        entertainment: required_number(record, &["entertainment", "entertainmentValue"])?,
        name_key: asset(record, &["nameKey", "displayName", "displayNameToken"]),
        icon: asset(record, &["icon", "iconPath"]),
        popularity_q16: 1 << 16,
        display_order: UNAUTHORED_TRICK_DISPLAY_ORDER,
        target: asset(record, &["target"]),
        prerequisite: None,
        levels: Vec::new(),
    });
    Ok(())
}

pub(super) fn bind_behavior_auxiliary(
    auxiliary: &LoweredBehaviorAuxiliary,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let mut display_order = auxiliary.tricks.iter().collect::<Vec<_>>();
    display_order.sort_unstable_by(|left, right| {
        match (left.sort_key.as_deref(), right.sort_key.as_deref()) {
            (Some(left), Some(right)) => left.cmp(right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| left.source_ordinal.cmp(&right.source_ordinal))
        .then_with(|| left.id.0.cmp(&right.id.0))
    });
    let display_order = display_order
        .into_iter()
        .enumerate()
        .map(|(ordinal, trick)| (trick.id.0, ordinal as u32))
        .collect::<std::collections::BTreeMap<_, _>>();

    for source in &auxiliary.tricks {
        if source.prerequisite.is_some_and(|prerequisite| {
            !auxiliary
                .tricks
                .iter()
                .any(|trick| trick.id == prerequisite.trick)
        }) {
            return Err(simple_error(
                "behavior auxiliary trick prerequisite does not resolve to a compiled trick",
            ));
        }
        if source.levels.is_empty()
            || source
                .levels
                .windows(2)
                .any(|levels| levels[0].minimum_score >= levels[1].minimum_score)
            || source.levels.iter().any(|level| {
                level.failure_percent > 100
                    || level.success_percent > 100
                    || level.critical_percent > 100
            })
        {
            return Err(simple_error(
                "behavior auxiliary contains invalid trick proficiency levels",
            ));
        }
        let mut eligible_species = source.eligible_species.clone();
        eligible_species.sort_unstable_by_key(|id| id.0);
        eligible_species.dedup();
        if eligible_species.is_empty() {
            return Err(simple_error(
                "behavior auxiliary trick has no eligible species",
            ));
        }
        let species = eligible_species;
        let levels = source.levels.clone();
        let prerequisite = source.prerequisite;

        if let Some(existing) = output
            .document
            .tricks
            .iter_mut()
            .find(|trick| trick.id == source.id)
        {
            existing.species = species;
            existing.difficulty = source.difficulty;
            existing.name_key = source.localized_name;
            existing.icon = source.icon;
            existing.popularity_q16 = source.popularity_q16;
            existing.display_order = display_order[&source.id.0];
            existing.target = source.target;
            existing.prerequisite = prerequisite;
            existing.levels = levels;
        } else {
            // .trk files omit these show-policy fields; absent values stay zero.
            output.document.tricks.push(TrickDefinition {
                id: source.id,
                species,
                animation: AssetId::default(),
                training_ticks: 0,
                difficulty: source.difficulty,
                welfare_cost: 0,
                entertainment: 0,
                name_key: source.localized_name,
                icon: source.icon,
                popularity_q16: source.popularity_q16,
                display_order: display_order[&source.id.0],
                target: source.target,
                prerequisite,
                levels,
            });
        }

        let tokens = source.outcome_tokens.ok_or_else(|| {
            simple_error(
                "behavior auxiliary reached the world catalogue without materialized trick outcome tokens",
            )
        })?;
        output
            .document
            .trick_outcome_tokens
            .push(TrickOutcomeTokens {
                id: source.id,
                failure: tokens.failure,
                success: tokens.success,
                critical: tokens.critical,
            });
    }

    if let Some(policy) = auxiliary.puzzle_placement_policy.as_ref() {
        if output.document.fossil_placement.is_some() {
            return Err(simple_error("duplicate fossil placement policy"));
        }
        if policy.puzzle_root.trim().is_empty() || policy.entity_root.trim().is_empty() {
            return Err(simple_error(
                "fossil placement policy has an empty authored root",
            ));
        }
        let mut placeable = policy.placeable_objects.clone();
        placeable.sort_unstable_by_key(|id| id.0);
        placeable.dedup();
        let mut non_placeable = policy.non_placeable_objects.clone();
        non_placeable.sort_unstable_by_key(|id| id.0);
        non_placeable.dedup();
        if placeable.is_empty()
            || non_placeable.is_empty()
            || placeable.iter().any(|id| {
                non_placeable
                    .binary_search_by_key(&id.0, |other| other.0)
                    .is_ok()
            })
        {
            return Err(simple_error(
                "fossil placement policy lists are empty or overlap",
            ));
        }
        let placeable_objects = placeable;
        let non_placeable_objects = non_placeable;
        output.document.fossil_placement = Some(FossilPlacementPolicy {
            puzzle_root: AssetId::from_virtual_path(&policy.puzzle_root),
            entity_root: AssetId::from_virtual_path(&policy.entity_root),
            placeable_objects,
            non_placeable_objects,
            minimum_sonar_distance_squared: policy.minimum_sonar_distance_squared,
            maximum_sonar_distance_squared: policy.maximum_sonar_distance_squared,
            minimum_sonar_view_dot: policy.minimum_sonar_view_dot,
            dig_distance_m: policy.dig_distance_m,
        });
    }
    Ok(())
}

pub(super) fn bind_show_rule(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.show_rules.push(ShowRuleDefinition {
        id: id(record.key),
        minimum_tricks: required_number(record, &["minimumTricks"])?,
        duration_ticks: array(
            record,
            &["minimumDurationTicks", "maximumDurationTicks"],
            [0_u32; 2],
        )?,
        cooldown_ticks: required_number(record, &["cooldownTicks"])?,
        payout_per_guest_cents: number_or(record, &["payoutPerGuestCents"], 0)?,
    });
    Ok(())
}
