//! The shipped `ZTDiseaseMgr` (`config/diseasemgr.xml`).

use super::world_definition_source_value_reading_and_conversion::{
    asset_list, element_asset, id, number_or, required_element_number, simple_error,
};
use crate::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::source_document_names_are_semantically_equal as same;
use openzt2_game_data::world_definitions::animal_health::{
    AuthoredDisease, DiseaseAnimalFilter, DiseaseComparison, DiseaseExposurePolicy, DiseaseZooTest,
};

pub(super) fn bind_disease_exposure_policy(
    record: &RecordView<'_, '_>,
) -> Result<DiseaseExposurePolicy, BindError> {
    let diseases = children(record.source_document_element(), "diseases")
        .flat_map(|list| children(list, "ZTDisease"))
        .map(bind_disease)
        .collect::<Result<_, _>>()?;
    Ok(DiseaseExposurePolicy {
        check_interval_seconds: number_or(record, &["diseaseCheckInterval"], 0.0)?,
        update_interval_seconds: number_or(record, &["updateInterval"], 0.0)?,
        cure_timeout_seconds: number_or(record, &["cureTimeout"], 0.0)?,
        half_star_minimum: number_or(record, &["halfStarMinimum"], 0)?,
        diseases,
    })
}

fn children<'a>(
    element: &'a OrderedSourceDocumentNode,
    name: &'a str,
) -> impl Iterator<Item = &'a OrderedSourceDocumentNode> + 'a {
    element
        .element_children()
        .filter(move |child| same(&child.name, name))
}

fn flag(element: &OrderedSourceDocumentNode, name: &str, default: bool) -> Result<bool, BindError> {
    match element.attribute_named_any(&[name]) {
        None => Ok(default),
        Some(value) if value.eq_ignore_ascii_case("true") || value == "1" => Ok(true),
        Some(value) if value.eq_ignore_ascii_case("false") || value == "0" => Ok(false),
        Some(value) => Err(simple_error(format!("invalid ZTDisease {name}: {value}"))),
    }
}

fn comparison(element: &OrderedSourceDocumentNode) -> Result<DiseaseComparison, BindError> {
    match element.attribute_named_any(&["test"]).unwrap_or("GE") {
        "LT" => Ok(DiseaseComparison::Less),
        "LE" => Ok(DiseaseComparison::LessOrEqual),
        "EQ" => Ok(DiseaseComparison::Equal),
        "GE" => Ok(DiseaseComparison::GreaterOrEqual),
        "GT" => Ok(DiseaseComparison::Greater),
        other => Err(simple_error(format!(
            "invalid disease predicate test {other}"
        ))),
    }
}

fn bind_disease(disease: &OrderedSourceDocumentNode) -> Result<AuthoredDisease, BindError> {
    let qualifiers = || children(disease, "qualifier");
    let zoo_tests = qualifiers()
        .flat_map(|qualifier| children(qualifier, "zooTests"))
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(|test| {
            if same(&test.name, "ZTEntityTypePredicate") {
                Ok(DiseaseZooTest::EntityCount {
                    entity_type: element_asset(test, &["type"]),
                    attribute: test
                        .attribute_named_any(&["attribute"])
                        .map(|attribute| Ok((id(attribute), flag(test, "value", true)?)))
                        .transpose()?,
                    quantity: required_element_number(test, &["quantity"])?,
                    comparison: comparison(test)?,
                    different: flag(test, "different", false)?,
                })
            } else if same(&test.name, "ZTZooFamePredicate") {
                Ok(DiseaseZooTest::Fame {
                    half_stars: required_element_number(test, &["fameHalfStars"])?,
                    comparison: comparison(test)?,
                })
            } else {
                Err(simple_error(format!(
                    "unknown disease zoo test {}",
                    test.name
                )))
            }
        })
        .collect::<Result<_, _>>()?;
    let animal_filters = qualifiers()
        .flat_map(|qualifier| children(qualifier, "animalFilters"))
        .flat_map(OrderedSourceDocumentNode::element_children)
        .map(|filter| {
            if same(&filter.name, "ZTAnimalAttributeFilter") {
                Ok(DiseaseAnimalFilter::Attribute {
                    attribute: element_asset(filter, &["attribute"]),
                    value: flag(filter, "boolValue", true)?,
                })
            } else if same(&filter.name, "ZTAnimalProximityFilter") {
                Ok(DiseaseAnimalFilter::Proximity {
                    entity_type: element_asset(filter, &["type"]),
                    distance_m: required_element_number(filter, &["distance"])?,
                    closer: flag(filter, "closer", true)?,
                })
            } else {
                Err(simple_error(format!(
                    "unknown disease animal filter {}",
                    filter.name
                )))
            }
        })
        .collect::<Result<_, _>>()?;
    let list = |name| {
        disease
            .attribute_named_any(&[name])
            .map(asset_list)
            .unwrap_or_default()
    };
    Ok(AuthoredDisease {
        name: element_asset(disease, &["name"]),
        token: element_asset(disease, &["token"]),
        cure_token: element_asset(disease, &["cureToken"]),
        clear_tokens: list("clearTokens"),
        target_types: list("targetTypes"),
        chance_percent: required_element_number(disease, &["chance"])?,
        chance_multiplier: disease
            .attribute_named_any(&["chanceMultiplier"])
            .filter(|value| !value.trim().is_empty())
            .map(id),
        single_qualifier: flag(disease, "singleQualifier", false)?,
        search_for_targets: flag(disease, "searchForTargets", true)?,
        force_only: flag(disease, "forceOnly", false)?,
        zoo_tests,
        animal_filters,
    })
}
