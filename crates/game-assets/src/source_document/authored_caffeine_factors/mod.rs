use openzt2_game_data::CaffeineFactors;

use super::{
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    resolved_source_record_index::{BindError, RecordView},
};

pub(crate) fn lower_caffeine_factors<'a>(
    record: &RecordView<'_, '_>,
    attribute: impl Fn(&str) -> Option<&'a str>,
) -> Result<CaffeineFactors, BindError> {
    let factor = |name| {
        attribute(name)
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|value| value.is_finite() && *value > 0.0)
                    .ok_or_else(|| {
                        BindError::record(record, format!("invalid locomotion factor {name}"))
                    })
            })
            .transpose()
            .map(|value| value.unwrap_or(1.0))
    };
    Ok(CaffeineFactors {
        caffeinated: factor("f_CaffeineModeFactor")?,
        decaffeinated: factor("f_DeCaffeineModeFactor")?,
    })
}
