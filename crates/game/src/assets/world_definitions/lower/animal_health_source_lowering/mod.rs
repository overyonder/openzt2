use super::object_facility_staff_and_guest_source_vocabulary::staff_role_kind;
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_flag_vocabulary::tranquilizer_flag;
use super::world_definition_source_value_reading_and_conversion::{
    array, asset, asset_list, flags, id, required, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::animal_health::{
    DiseaseDefinition, RampageRule, TranquilizerDefinition, TranquilizerEligibility,
    TranquilizerModePolicy, TreatmentDefinition,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_disease(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let species = record
        .value(&["eligibleSpecies", "species"])
        .map(asset_list)
        .unwrap_or_default();
    let symptoms = record
        .value(&["symptoms"])
        .map(asset_list)
        .unwrap_or_default();
    let treatments = record
        .value(&["treatments"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.diseases.push(DiseaseDefinition {
        id: id(record.key),
        eligible_species: species,
        check_interval_ticks: required_number(record, &["checkIntervalTicks"])?,
        chance_per_check: required_number(record, &["chancePerCheck", "probability"])?,
        severity_per_tick_q16: required_number(record, &["severityPerTickQ16"])?,
        vitality_per_tick_q16: required_number(record, &["vitalityPerTickQ16"])?,
        symptoms,
        treatments,
        fatal_threshold: required_number(record, &["fatalThreshold"])?,
        hint_thresholds: array(
            record,
            &[
                "firstHintThreshold",
                "secondHintThreshold",
                "thirdHintThreshold",
            ],
            [0_u16; 3],
        )?,
    });
    Ok(())
}

pub(super) fn bind_treatment(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.treatments.push(TreatmentDefinition {
        id: id(record.key),
        disease: asset(record, &["disease"]),
        required_staff: staff_role_kind(required(record, &["requiredStaff"])?)?,
        duration_ticks: required_number(record, &["durationTicks"])?,
        severity_delta: required_number(record, &["severityDelta"])?,
        vitality_delta: required_number(record, &["vitalityDelta"])?,
        research: asset(record, &["research"]),
    });
    Ok(())
}

pub(super) fn bind_tranquilizer(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let eligibility_flag_bits = flags(record.value(&["eligibleStates"]), tranquilizer_flag)?;
    let eligible_states = u8::try_from(eligibility_flag_bits)
        .ok()
        .and_then(TranquilizerEligibility::from_flag_bits)
        .ok_or_else(|| {
            BindError::record(
                record,
                "tranquilizer eligibility contains unknown flag bits",
            )
        })?;
    output.document.tranquilizers.push(TranquilizerDefinition {
        id: id(record.key),
        duration_ticks: required_number(record, &["durationTicks"])?,
        eligible_states,
        recovery_ticks: required_number(record, &["recoveryTicks"])?,
    });
    Ok(())
}

pub(super) fn bind_tranquilizer_mode(
    records: &[RecordView<'_, '_>],
) -> Result<Option<TranquilizerModePolicy>, BindError> {
    records
        .iter()
        .find(|record| {
            canonicalize_source_document_record_key(&record.semantic_type()) == "zttranquilizemode"
        })
        .map(|record| {
            let meters_to_cm = |value: f32| (value * 100.0).round() as i32;
            let particle = required(record, &["gunParticleSystem"])?;
            Ok(TranquilizerModePolicy {
                range_cm: meters_to_cm(required_number(record, &["gunMaxRangeInMeters"])?) as u32,
                charge_points_per_second: required_number(record, &["gunChargePointsPerSec"])?,
                charge_decrease_points_per_second: required_number(
                    record,
                    &["gunChargeDecreasePointsPerSec"],
                )?,
                required_charge_points: required_number(record, &["defaultTargetChargePoints"])?,
                use_distance_cm: meters_to_cm(required_number(record, &["useDistance"])?) as u32,
                shot_effect: id(&format!("particle/{particle}")),
                shot_effect_distance_cm: meters_to_cm(required_number(
                    record,
                    &["gunParticleDistance"],
                )?),
                shot_effect_offset_cm: [
                    meters_to_cm(required_number(record, &["gunParticleXOffset"])?),
                    meters_to_cm(required_number(record, &["gunParticleZOffset"])?),
                ],
                recoil_duration_seconds: required_number(record, &["recoilDuration"])?,
                misfire_duration_seconds: required_number(record, &["misfireDuration"])?,
                shot_cue: AssetId::from_key("audio-cue:tranqmode_shot"),
                misfire_cue: AssetId::from_key("audio-cue:tranqmode_misfire"),
                begin_charge_cue: AssetId::from_key("audio-cue:tranqmode_begincharge"),
                charge_cue: AssetId::from_key("audio-cue:tranqmode_chargeup"),
                end_charge_cue: AssetId::from_key("audio-cue:tranqmode_endcharge"),
                lose_tracking_cue: AssetId::from_key("audio-cue:tranqmode_losetracking"),
                reticle_charging: AssetId::from_virtual_path(
                    "ui/minigames/tranqgun_reticule_red.dds",
                ),
                reticle_ready: AssetId::from_virtual_path(
                    "ui/minigames/tranqgun_reticule_green.dds",
                ),
            })
        })
        .transpose()
}

pub(super) fn bind_rampage(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    output.document.rampage_rules.push(RampageRule {
        id: id(record.key),
        species: asset(record, &["species"]),
        welfare_below: required_number(record, &["welfareBelow"])?,
        disease_above: required_number(record, &["diseaseAbove"])?,
        probability: required_number(record, &["probability"])?,
        minimum_ticks: required_number(record, &["minimumTicks"])?,
        behavior: asset(record, &["behavior"]),
    });
    Ok(())
}
