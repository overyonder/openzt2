use super::source_queries::authored_boolean_is_true;
use super::source_queries::find_all_semantically_named_source_descendants;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode;
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::source_document_names_are_semantically_equal;
use openzt2_game_data::species::NeedKind;
use openzt2_game_data::species::SpeciesNeed;

#[derive(Default)]
struct EffectiveAuthoredAnimalNeedState {
    value: Option<f64>,
    value_minimum: Option<f64>,
    value_maximum: Option<f64>,
    cessation_threshold: Option<f64>,
    trigger_threshold: Option<f64>,
    critical_threshold: Option<f64>,
    display_threshold: Option<f64>,
    pressing_threshold: Option<f64>,
    advanced: bool,
}

pub(super) fn lower_effective_authored_species_need_facts(
    species: &RecordView<'_, '_>,
) -> Result<Vec<SpeciesNeed>, BindError> {
    [
        (NeedKind::Hunger, "hunger"),
        (NeedKind::Thirst, "thirst"),
        (NeedKind::Rest, "rest"),
        (NeedKind::Privacy, "privacy"),
        (NeedKind::Social, "social"),
        (NeedKind::Exercise, "exercise"),
        (NeedKind::Stimulation, "stimulation"),
        (NeedKind::Environment, "environment"),
        (NeedKind::Health, "health"),
        (NeedKind::Hygiene, "hygiene"),
        (NeedKind::Bathroom, "bathroom"),
    ]
    .into_iter()
    .map(|(kind, authored_name)| {
        lower_effective_authored_animal_need_to_canonical_species_need(species, kind, authored_name)
    })
    .collect()
}

fn lower_effective_authored_animal_need_to_canonical_species_need(
    species: &RecordView<'_, '_>,
    kind: NeedKind,
    authored_name: &str,
) -> Result<SpeciesNeed, BindError> {
    let mut state = EffectiveAuthoredAnimalNeedState::default();
    let mut adjustment = 0.0;
    let mut found_state = false;
    let mut family_records = species
        .type_tokens()
        .into_iter()
        .filter(|family_key| !source_document_names_are_semantically_equal(family_key, species.key))
        .filter_map(|family_key| species.find_resolved_source_record_by_reference(&family_key))
        .collect::<Vec<_>>();
    family_records.push(*species);

    for family_record in family_records {
        for need_adjustments in find_all_semantically_named_source_descendants(
            family_record.source_document_element(),
            "BFAINeedAdjusts",
        )
        .into_iter()
        .filter(|element| {
            element
                .attribute_named_any(&["Name"])
                .is_none_or(|name| name.trim().is_empty())
        }) {
            if let Some(value) = need_adjustments.attribute_named_any(&[authored_name]) {
                adjustment = parse_finite_authored_animal_need_number(
                    species,
                    authored_name,
                    "adjustment",
                    value,
                )?;
            }
        }
        for state_variable in find_all_semantically_named_source_descendants(
            family_record.source_document_element(),
            "BFAIStateVar",
        )
        .into_iter()
        .filter(|element| {
            element.attribute_named_any(&["Name"]).is_some_and(|name| {
                source_document_names_are_semantically_equal(name, authored_name)
            })
        }) {
            found_state = true;
            apply_authored_animal_need_state_variable_override(
                species,
                authored_name,
                state_variable,
                &mut state,
            )?;
        }
    }

    if !found_state {
        return Err(BindError::record(
            species,
            format!("animal need {authored_name} has no source-backed BFAIStateVar"),
        ));
    }

    let minimum = state
        .value
        .unwrap_or(state.value_minimum.unwrap_or_default());
    let maximum = state
        .value
        .unwrap_or(state.value_maximum.unwrap_or(minimum));
    let initial_wellness_permille_range = [
        lower_authored_animal_need_value_to_wellness_permille(species, authored_name, maximum)?,
        lower_authored_animal_need_value_to_wellness_permille(species, authored_name, minimum)?,
    ];
    let wellness_adjustment_per_update_q16 = (-adjustment * 10.0 * 65_536.0)
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX))
        as i32;

    Ok(SpeciesNeed {
        kind,
        initial_wellness_permille_range,
        wellness_adjustment_per_update_q16,
        cessation_wellness_threshold:
            lower_optional_authored_animal_need_threshold_to_wellness_permille(
                state.cessation_threshold,
            ),
        trigger_wellness_threshold:
            lower_optional_authored_animal_need_threshold_to_wellness_permille(
                state.trigger_threshold,
            ),
        critical_wellness_threshold:
            lower_optional_authored_animal_need_threshold_to_wellness_permille(
                state.critical_threshold,
            ),
        display_wellness_threshold:
            lower_optional_authored_animal_need_threshold_to_wellness_permille(
                state.display_threshold,
            ),
        pressing_wellness_threshold:
            lower_optional_authored_animal_need_threshold_to_wellness_permille(
                state.pressing_threshold,
            ),
        advanced: state.advanced,
        preferences: Vec::new(),
    })
}

fn apply_authored_animal_need_state_variable_override(
    species: &RecordView<'_, '_>,
    authored_name: &str,
    state_variable: &'_ OrderedSourceDocumentNode,
    state: &mut EffectiveAuthoredAnimalNeedState,
) -> Result<(), BindError> {
    macro_rules! replace_authored_number_if_present {
        ($field:ident, $attribute:literal) => {
            if let Some(value) = state_variable.attribute_named_any(&[$attribute]) {
                state.$field = (!value.trim().is_empty())
                    .then(|| {
                        parse_finite_authored_animal_need_number(
                            species,
                            authored_name,
                            $attribute,
                            value,
                        )
                    })
                    .transpose()?;
            }
        };
    }

    replace_authored_number_if_present!(value, "Value");
    replace_authored_number_if_present!(value_minimum, "ValueMin");
    replace_authored_number_if_present!(value_maximum, "ValueMax");
    replace_authored_number_if_present!(cessation_threshold, "CessationThreshold");
    replace_authored_number_if_present!(trigger_threshold, "TriggerThreshold");
    replace_authored_number_if_present!(critical_threshold, "CriticalThreshold");
    replace_authored_number_if_present!(display_threshold, "DisplayThreshold");
    replace_authored_number_if_present!(pressing_threshold, "PressingThreshold");
    if let Some(value) = state_variable.attribute_named_any(&["Advanced"]) {
        state.advanced = authored_boolean_is_true(value);
    }
    Ok(())
}

fn parse_finite_authored_animal_need_number(
    species: &RecordView<'_, '_>,
    authored_name: &str,
    property: &str,
    value: &str,
) -> Result<f64, BindError> {
    parse_blue_fang_source_numeric_lexeme::<f64>(value)
        .filter(|value| value.is_finite())
        .ok_or_else(|| {
            BindError::record(
                species,
                format!("animal need {authored_name} has invalid {property} {value}"),
            )
        })
}

fn lower_authored_animal_need_value_to_wellness_permille(
    species: &RecordView<'_, '_>,
    authored_name: &str,
    value: f64,
) -> Result<u16, BindError> {
    if !(0.0..=100.0).contains(&value) {
        return Err(BindError::record(
            species,
            format!("animal need {authored_name} initial value {value} is outside 0..100"),
        ));
    }
    Ok((1_000.0 - value * 10.0).round() as u16)
}

fn lower_optional_authored_animal_need_threshold_to_wellness_permille(
    value: Option<f64>,
) -> Option<u16> {
    value
        .filter(|value| (0.0..=100.0).contains(value))
        .map(|value| (1_000.0 - value * 10.0).round() as u16)
}
