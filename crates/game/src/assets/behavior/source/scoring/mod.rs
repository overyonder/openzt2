//! Conversion of authored behavior evaluation attributes into canonical scores.

use std::io;

use openzt2_game_data::{behavior::score::BehaviorScore, AssetId};

use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;

use super::invalid_behavior_source_data;

pub(super) fn lower_behavior_evaluation(
    node: &OrderedSourceDocumentNode,
) -> io::Result<Vec<BehaviorScore>> {
    let mut scores = node
        .attributes
        .iter()
        .map(|attribute| lower_behavior_score(attribute.name(), attribute.value()))
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    for child in node.element_children() {
        if !matches!(
            child.name.as_str(),
            "BFAIAttributeFloatMap" | "BFAIBiomeMap"
        ) || child.element_children().next().is_some()
        {
            return Err(invalid_behavior_source_data(format!(
                "unmapped behavior evaluation child {}",
                child.name
            )));
        }
        for attribute in &child.attributes {
            scores.push(if child.name == "BFAIBiomeMap" {
                BehaviorScore::BiomeWeight {
                    biome: AssetId::from_key(attribute.name()),
                    weight: parse_behavior_source_float(attribute.value())?,
                }
            } else {
                BehaviorScore::AttributeValue {
                    attribute: AssetId::from_key(attribute.name()),
                    value: super::scalar::lower_behavior_scalar_q16(attribute.value())?,
                }
            });
        }
    }
    Ok(scores)
}

fn lower_behavior_score(
    authored_score_name: &str,
    authored_score_value: &str,
) -> io::Result<Option<BehaviorScore>> {
    match authored_score_name {
        "fixedScore" => parse_behavior_source_float(authored_score_value)
            .map(BehaviorScore::Fixed)
            .map(Some),
        "distanceInfluenced" => parse_behavior_score_boolean(authored_score_value)
            .map(BehaviorScore::DistanceInfluence)
            .map(Some),
        "leaveZoo" => lower_optional_boolean_behavior_score(
            authored_score_value,
            BehaviorScore::LeaveZooPressure,
        ),
        "needPointsGood" | "needGoodPoints" | "f_needPointsGood" => {
            parse_behavior_source_float(authored_score_value)
                .map(|weight| Some(BehaviorScore::AggregateGoodNeeds { weight }))
        }
        "needPointsBad" | "f_needPointsBad" => parse_behavior_source_float(authored_score_value)
            .map(|weight| Some(BehaviorScore::AggregateBadNeeds { weight })),
        unsupported => Err(invalid_behavior_source_data(format!(
            "unmapped behavior score input {unsupported}"
        ))),
    }
}

fn lower_optional_boolean_behavior_score(
    authored_boolean_value: &str,
    enabled_behavior_score: BehaviorScore,
) -> io::Result<Option<BehaviorScore>> {
    parse_behavior_score_boolean(authored_boolean_value)
        .map(|enabled| enabled.then_some(enabled_behavior_score))
}

fn parse_behavior_score_boolean(authored_boolean_value: &str) -> io::Result<bool> {
    match authored_boolean_value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(invalid_behavior_source_data("invalid behavior score flag")),
    }
}

fn parse_behavior_source_float(authored_number: &str) -> io::Result<f32> {
    let value: f32 = parse_blue_fang_source_numeric_lexeme(authored_number).ok_or_else(|| {
        invalid_behavior_source_data(format!("invalid behavior number {authored_number}"))
    })?;
    if !value.is_finite() {
        return Err(invalid_behavior_source_data("non-finite behavior score"));
    }
    Ok(value)
}
