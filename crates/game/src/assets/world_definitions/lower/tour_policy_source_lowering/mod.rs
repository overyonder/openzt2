use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_value_reading_and_conversion::{
    array_required, id, required_element_number, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::world_definitions::transportation_and_tours::{
    TourCategoryScore, TourRatingRange, TourScoringPolicy,
};

pub(super) fn lower_authored_tour_category_scores_to_world_definition_tables(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    for category in record.source_document_element().element_children() {
        output.tour_category_scores.push(TourCategoryScore {
            category: id(category.name.as_str()),
            value: required_element_number(&category, &["value"])?,
        });
    }
    Ok(())
}

pub(super) fn lower_authored_tour_scoring_policy_to_world_definition_tables(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    if output.document.tour_scoring.is_some() {
        return Err(BindError::record(
            record,
            "duplicate resolved ZTTourDataAttr policy",
        ));
    }
    output.document.tour_scoring = Some(TourScoringPolicy {
        species_base_inducement: required_number(record, &["SpeciesBaseInducementValue"])?,
        view_event_curve: array_required(
            record,
            &["viewEventTourValueMultiplier", "viewEventTourValueExponent"],
        )?,
        static_object_curve: array_required(
            record,
            &[
                "staticObjectTourValueMultiplier",
                "staticObjectTourValueExponent",
            ],
        )?,
        animal_curve: array_required(
            record,
            &["animalTourValueMultiplier", "animalTourValueExponent"],
        )?,
        view_target_memory: required_number(record, &["viewTargetMemorySize"])?,
        tour_history: required_number(record, &["tourHistorySize"])?,
        inducement_random_factor: array_required(
            record,
            &["inducementRandMinFactor", "inducementRandMaxFactor"],
        )?,
        score_threshold: required_number(record, &["tourScoreThreshold"])?,
        need_threshold: required_number(record, &["tourNeedThreshold"])?,
        animal_feedback_probability: required_number(record, &["animalFeedbackProbability"])?,
        animal_feedback_threshold: required_number(record, &["animalFeedbackThreshold"])?,
        object_feedback_thresholds: [
            required_number(record, &["objectFeedbackNegativeThreshold"])?,
            required_number(record, &["objectFeedbackPositiveThreshold"])?,
        ],
        categories: Vec::new(),
        rating_ranges: Vec::new(),
    });
    Ok(())
}

pub(super) fn lower_authored_tour_rating_ranges_to_world_definition_tables(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    for sets in record.children_named(&["ZTTourRatingSets"]) {
        for row in sets.element_children().filter(|child| {
            canonicalize_source_document_record_key(child.name.as_str()) == "zttourratingrange"
        }) {
            output.tour_rating_ranges.push(TourRatingRange {
                score: [
                    required_element_number(&row, &["min"])?,
                    required_element_number(&row, &["max"])?,
                ],
                rating: [
                    required_element_number(&row, &["valueMin"])?,
                    required_element_number(&row, &["valueMax"])?,
                ],
            });
        }
    }
    Ok(())
}
