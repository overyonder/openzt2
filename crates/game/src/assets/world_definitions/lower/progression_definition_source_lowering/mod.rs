use crate::assets::source_document::resolved_source_record_index::{
    BindError,
    RecordView,
};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::catalogue_and_progression::research_and_unlock_definition_types::{
    ResearchDefinition,
    ResearchDuration,
    UnlockDefinition,
    UnlockRequirement,
};
use openzt2_game_data::world_definitions::catalogue_and_progression::zoo_rating_fame_and_award_definition_types::{
    AwardCondition,
    AwardDefinition,
    FameThreshold,
    RatingDefinition,
    RatingInput,
};
use super::zoo_rating_source_vocabulary::{
    comparison,
    rating_input,
};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    asset,
    asset_list,
    element_number,
    id,
    number_or,
    required_element,
    required_element_number,
    required_number,
};

pub(super) fn bind_research(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let prerequisites = record
        .value(&["prerequisites"])
        .map(asset_list)
        .unwrap_or_default();
    let unlocks = record
        .value(&["unlocks"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.research.push(ResearchDefinition {
        id: id(record.key),
        name_key: asset(record, &["nameKey", "displayName"]),
        cost_cents: required_number(record, &["costCents", "cost"])?,
        duration: ResearchDuration::Ticks(required_number(record, &["durationTicks"])?),
        minimum_fame_percent: 0.0,
        prerequisites,
        unlocks,
    });
    Ok(())
}

pub(super) fn bind_unlock(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let requirement = match canonicalize_source_document_record_key(
        record
            .value(&["requirement", "requirementType"])
            .unwrap_or("always"),
    )
    .as_str()
    {
        "always" => UnlockRequirement::Always,
        "fame" => UnlockRequirement::Fame(required_number(record, &["fame", "value"])?),
        "research" => UnlockRequirement::Research(asset(record, &["research", "value"])),
        "scenario" => UnlockRequirement::Scenario(asset(record, &["scenario", "value"])),
        "award" => UnlockRequirement::Award(asset(record, &["award", "value"])),
        value => {
            return Err(BindError::record(
                record,
                format!("unknown unlock requirement {value}"),
            ));
        }
    };
    output.document.unlocks.push(UnlockDefinition {
        id: id(record.key),
        target: asset(record, &["target"]),
        requirement,
    });
    Ok(())
}

pub(super) fn bind_rating(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let mut inputs = Vec::new();
    for input in record.children_named(&["input", "ratingInput"]) {
        inputs.push(RatingInput {
            kind: rating_input(required_element(&input, &["kind", "type"])?)?,
            weight: required_element_number(&input, &["weight"])?,
            minimum: element_number(&input, &["minimum", "min"], 0)?,
            maximum: required_element_number(&input, &["maximum", "max"])?,
        });
    }
    output.document.rating_definitions.push(RatingDefinition {
        id: id(record.key),
        inputs,
        minimum: number_or(record, &["minimum", "min"], 0)?,
        maximum: required_number(record, &["maximum", "max"])?,
        smoothing_ticks: number_or(record, &["smoothingTicks"], 0)?,
    });
    Ok(())
}

pub(super) fn bind_fame(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let unlocks = record
        .value(&["unlocks"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.fame_thresholds.push(FameThreshold {
        level: required_number(record, &["level", "halfStars"])?,
        minimum_rating: required_number(record, &["minimumRating", "rating"])?,
        minimum_guests: number_or(record, &["minimumGuests", "guests"], 0)?,
        unlocks,
    });
    Ok(())
}

pub(super) fn bind_award(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let mut conditions = Vec::new();
    for condition in record.children_named(&["condition", "awardCondition"]) {
        conditions.push(AwardCondition {
            kind: rating_input(required_element(&condition, &["kind", "type"])?)?,
            comparison: comparison(required_element(&condition, &["comparison", "operator"])?)?,
            value: required_element_number(&condition, &["value", "threshold"])?,
            duration_ticks: element_number(&condition, &["durationTicks"], 0)?,
        });
    }
    let unlocks = record
        .value(&["unlocks"])
        .map(asset_list)
        .unwrap_or_default();
    output.document.awards.push(AwardDefinition {
        id: id(record.key),
        name_key: asset(record, &["nameKey", "nameToken", "locid"]),
        description_key: asset(record, &["descriptionKey", "descriptionToken"]),
        conditions,
        reward_cents: number_or(record, &["rewardCents", "cashReward"], 0)?,
        unlocks,
    });
    Ok(())
}

pub(super) fn bind_authored_catalogue_research(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    use super::source_element_tree_search::authored_type_family_component_attribute;
    use super::world_definition_source_value_reading_and_conversion::{money_cents, seconds_ns};
    let attribute =
        |name| authored_type_family_component_attribute(record, "BFAIEntityDataShared", name);
    if !attribute("b_Researchable")
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return Ok(());
    }
    let target = id(record.key);
    let Some(entry) = output
        .document
        .catalogue
        .iter()
        .find(|entry| entry.definition == target)
    else {
        return Ok(());
    };
    let name_key = entry.name_key;
    let number = |name| -> Result<Option<f64>, BindError> {
        attribute(name)
            .map(|value| {
                crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme::<f64>(value)
                    .ok_or_else(|| BindError::record(record, format!("invalid {name}: {value}")))
            })
            .transpose()
    };
    let duration = number("f_researchUnlockTime")?
        .map(|seconds| seconds_ns(seconds, record).map(ResearchDuration::Nanoseconds))
        .transpose()?
        .unwrap_or(ResearchDuration::RandomDefault);
    let cost_cents = money_cents(number("f_researchCost")?.unwrap_or(0.0), record)?;
    let minimum_fame_percent = number("f_FameReq")?.unwrap_or(0.0);
    if !minimum_fame_percent.is_finite()
        || !(0.0..=f64::from(u16::MAX)).contains(&minimum_fame_percent)
    {
        return Err(BindError::record(
            record,
            "invalid research fame requirement",
        ));
    }
    output.document.research.push(ResearchDefinition {
        id: target,
        name_key,
        cost_cents: i64::from(cost_cents),
        duration,
        minimum_fame_percent: minimum_fame_percent as f32,
        prerequisites: Vec::new(),
        unlocks: vec![target],
    });
    output.document.unlocks.push(UnlockDefinition {
        id: id(&format!("research/{}", record.key)),
        target,
        requirement: UnlockRequirement::Research(target),
    });
    Ok(())
}
