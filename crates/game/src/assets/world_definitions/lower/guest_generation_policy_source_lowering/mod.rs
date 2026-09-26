use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array_required, element_bool, money_cents, percent_permille, required_bool,
    required_element_number, required_number, seconds_ns,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::behavior_selection_policy::BehaviorSelectionPolicy;
use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestGenerationPolicy, GuestNeedSurveyPolicy, GuestNeedSurveySignals, GuestPriceAdjustment,
    GuestSpeciesAdjustment,
};

pub(super) fn bind_guest_policy(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.guest_generation.is_some() {
        return Err(BindError::record(
            record,
            "duplicate resolved ZTAIGuestMgr policy",
        ));
    }
    let ai_policy = record
        .find_resolved_source_record_by_reference("ai/ztai.xml")
        .and_then(|policy| policy.descendant_named("General"))
        .ok_or_else(|| BindError::record(record, "guest policy is missing ai/ztai.xml General"))?;
    let minimum_score: f32 = required_element_number(&ai_policy, &["LowScore"])?;
    let outside_range_need_value: f32 =
        required_element_number(&ai_policy, &["OutsideRangeNeedValue"])?;
    if !minimum_score.is_finite() || !outside_range_need_value.is_finite() {
        return Err(BindError::record(
            record,
            "non-finite behavior selection policy",
        ));
    }
    output.document.behavior_selection_policy = Some(BehaviorSelectionPolicy {
        enabled: element_bool(&ai_policy, &["AI_On"], true)?,
        minimum_score,
        outside_range_need_value,
    });
    let need_adjustment_interval: u32 =
        required_element_number(&ai_policy, &["StateThinkerInterval"])?;
    let need_adjustment_delay_steps = need_adjustment_interval
        .checked_mul(2)
        .filter(|steps| *steps > 0)
        .ok_or_else(|| BindError::record(record, "invalid StateThinkerInterval"))?;
    let need_adjustments_enabled = element_bool(&ai_policy, &["Need_Adjust_On"], true)?;
    let mut price_adjustments = Vec::new();
    for container in record.children_named(&["ZTSpawnPriceEffects"]) {
        for adjustment in container.element_children().filter(|child| {
            canonicalize_source_document_record_key(child.name.as_str()) == "bfpercentadjust"
        }) {
            price_adjustments.push(GuestPriceAdjustment {
                percent_range: [
                    required_element_number(&adjustment, &["PercentRangeMin"])?,
                    required_element_number(&adjustment, &["PercentRangeMax"])?,
                ],
                percent_delta: required_element_number(&adjustment, &["PercentDelta"])?,
                percent_adjustment: required_element_number(&adjustment, &["PercentAdjust"])?,
            });
        }
    }
    let species = record
        .children_named(&["ZTSpawnAnimalEffects"])
        .first()
        .copied()
        .ok_or_else(|| BindError::record(record, "ZTAIGuestMgr is missing ZTSpawnAnimalEffects"))?;
    let needs = record
        .children_named(&["ZTSpawnNeedEffects"])
        .first()
        .copied()
        .ok_or_else(|| BindError::record(record, "ZTAIGuestMgr is missing ZTSpawnNeedEffects"))?;
    let monitored = needs
        .element_children()
        .find(|child| {
            canonicalize_source_document_record_key(child.name.as_str()) == "ztmonitoredneeds"
        })
        .ok_or_else(|| {
            BindError::record(record, "ZTSpawnNeedEffects is missing ZTMonitoredNeeds")
        })?;
    let mut monitored_signals = GuestNeedSurveySignals::EMPTY;
    for signal in monitored.attribute_names() {
        let signal = match canonicalize_source_document_record_key(signal).as_str() {
            "hunger" => GuestNeedSurveySignals::HUNGER,
            "thirst" => GuestNeedSurveySignals::THIRST,
            "dessert" => GuestNeedSurveySignals::DESSERT,
            "gift" => GuestNeedSurveySignals::GIFT,
            "bathroom" => GuestNeedSurveySignals::BATHROOM,
            value => {
                return Err(BindError::record(
                    record,
                    format!("unsupported monitored guest survey signal {value}"),
                ));
            }
        };
        monitored_signals = monitored_signals.with_additional_signals(signal);
    }
    output.document.guest_generation = Some(GuestGenerationPolicy {
        need_adjustments_enabled,
        need_adjustment_delay_steps,
        enabled: required_bool(record, &["On"])?,
        use_test_type: required_bool(record, &["UseAITestType"])?,
        maximum_guests_base: required_number(record, &["MaxGuestsBase"])?,
        maximum_guests_per_half_star: required_number::<u32>(record, &["MaxGuestsPerStar"])? / 2,
        spawn_at_entrance: required_bool(record, &["SpawnAtEntrance"])?,
        arrival_offset_cm: array_required(record, &["DisplacementX", "DisplacementY"])?,
        departure_offset_cm: array_required(record, &["DisplacementXLeave", "DisplacementYLeave"])?,
        delay_ns: [
            seconds_ns(required_number(record, &["MinDelaySecs"])?, record)?,
            seconds_ns(required_number(record, &["MaxDelaySecs"])?, record)?,
        ],
        default_emitter_cm: array_required(record, &["DefaultEmitterX", "DefaultEmitterY"])?,
        stagger: required_bool(record, &["StaggerXY"])?,
        stagger_square_cm: required_number(record, &["StaggerSquare"])?,
        base_spawn_probability: required_number(record, &["BaseSpawnProbability"])?,
        minimum_spawn_rate: required_number(record, &["MinSpawnRate"])?,
        normal_admission_cents: [
            money_cents(required_number(record, &["NormalAdultAdmission"])?, record)?,
            money_cents(required_number(record, &["NormalChildAdmission"])?, record)?,
        ],
        fame_rate_increase_permille_per_half_star: percent_permille(
            required_number(record, &["ZooFamePercentRateIncreasePerStar"])?,
            record,
        )? / 2,
        rarity_thresholds: [
            required_number(record, &["RareThreshold"])?,
            required_number(record, &["UncommonThreshold"])?,
        ],
        price_adjustments,
        species_adjustment: GuestSpeciesAdjustment {
            species_per_adjustment: required_element_number(&species, &["NumSpeciesPerAdjust"])?,
            percent_per_adjustment: required_element_number(
                &species,
                &["PercentAdjustPerNumSpecies"],
            )?,
            maximum_percent_adjustment: required_element_number(&species, &["MaxPercentAdjust"])?,
        },
        need_survey: GuestNeedSurveyPolicy {
            check_interval_ns: seconds_ns(
                required_element_number(&needs, &["CriticalCheckInterval"])?,
                record,
            )?,
            maximum_critical_hits_per_month: required_element_number(
                &needs,
                &["MaxCriticalNeedHitsPerMonth"],
            )?,
            maximum_education_points_per_view: required_element_number(
                &needs,
                &["MaxEduPointsPerView"],
            )?,
            maximum_entertainment_points_per_view: required_element_number(
                &needs,
                &["MaxEntPointsPerView"],
            )?,
            monitored: monitored_signals,
        },
    });
    Ok(())
}
