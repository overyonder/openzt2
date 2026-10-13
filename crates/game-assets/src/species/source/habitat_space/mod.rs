use openzt2_game_data::species::{NonNegativeSquareMetres, SpeciesHabitatSpace};

use super::source_queries::find_inherited_animal_shared_attribute;
use crate::source_document::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    resolved_source_record_index::{BindError, RecordView},
};

pub(super) fn lower_authored_species_habitat_space(
    species: &RecordView<'_, '_>,
) -> Result<Option<SpeciesHabitatSpace>, BindError> {
    let inherited_number = |attribute| {
        let value = find_inherited_animal_shared_attribute(species, attribute).or_else(|| {
            species.type_tokens().into_iter().rev().find_map(|key| {
                species
                    .find_resolved_source_record_by_reference(&key)
                    .and_then(|family| find_inherited_animal_shared_attribute(&family, attribute))
            })
        });
        value
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .and_then(NonNegativeSquareMetres::from_finite_non_negative)
                    .ok_or_else(|| {
                        BindError::record(
                            species,
                            format!("animal space attribute {attribute} has invalid value {value}"),
                        )
                    })
            })
            .transpose()
    };
    let initial = inherited_number("f_RequiredInitialSpace")?;
    let additional = inherited_number("f_RequiredAdditionalSpace")?;
    Ok(initial
        .zip(additional)
        .map(
            |(initial_square_metres, additional_square_metres)| SpeciesHabitatSpace {
                initial_square_metres,
                additional_square_metres,
            },
        ))
}

#[cfg(test)]
mod tests;
