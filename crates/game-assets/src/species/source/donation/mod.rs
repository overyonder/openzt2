use super::source_queries::find_effective_authored_animal_shared_attribute;
use crate::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::source_document::resolved_source_record_index::{BindError, RecordView};
use crate::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use crate::world_definitions::lower::world_definition_source_value_reading_and_conversion::money_cents;
use openzt2_game_data::species::AnimalDonationPolicy;
use openzt2_game_data::AssetId;

pub(super) fn lower_animal_donation_policy(
    record: &RecordView<'_, '_>,
) -> Result<Option<AnimalDonationPolicy>, BindError> {
    let inherited = |attribute| find_effective_authored_animal_shared_attribute(record, attribute);
    let Some(amount) = inherited("f_BaseDonationAmount") else {
        return Ok(None);
    };
    let number = |value: &str| {
        parse_blue_fang_source_numeric_lexeme::<f64>(value)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| BindError::record(record, "invalid animal donation value"))
    };
    let Some(factor) = inherited("f_MaxSpeciesDonationFactor") else {
        return Ok(None);
    };
    let factor = number(factor)?;
    if factor > f64::from(f32::MAX) {
        return Err(BindError::record(
            record,
            "animal donation factor exceeds stored range",
        ));
    }
    Ok(Some(AnimalDonationPolicy {
        token: inherited("s_DonateToken")
            .filter(|token| !token.trim().is_empty())
            .map(|token| AssetId::from_key(&canonicalize_source_document_record_key(token))),
        base_amount_cents: money_cents(number(amount)?, record)?,
        maximum_species_factor: factor as f32,
    }))
}
