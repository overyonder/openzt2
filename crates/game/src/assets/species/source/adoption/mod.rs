use super::source_queries::find_inherited_animal_shared_attribute;
use crate::assets::source_document::blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme;
use crate::assets::source_document::resolved_source_record_index::BindError;
use crate::assets::source_document::resolved_source_record_index::RecordView;
use openzt2_game_data::species::AnimalAdoptionOfferDefinition;
use openzt2_game_data::species::Sex;

pub(super) fn lower_authored_animal_adoption_offer_definition(
    species: &RecordView<'_, '_>,
) -> Result<AnimalAdoptionOfferDefinition, BindError> {
    let inherited_number = |attribute: &str, default: f32| {
        find_inherited_animal_shared_attribute(species, attribute)
            .map(|value| {
                parse_blue_fang_source_numeric_lexeme::<f32>(value)
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| {
                        BindError::record(
                            species,
                            format!(
                                "animal adoption attribute {attribute} has invalid value {value}"
                            ),
                        )
                    })
            })
            .transpose()
            .map(|value| value.unwrap_or(default))
    };
    let unlock_seconds = inherited_number("f_adoptUnlockTime", 0.0)?;
    let minimum_unlock_seconds = inherited_number("f_adoptMinUnlockTime", unlock_seconds)?;
    Ok(AnimalAdoptionOfferDefinition {
        rarity_fame_percent: inherited_number("f_adoptRarity", 0.0)?
            .round()
            .clamp(0.0, f32::from(u16::MAX)) as u16,
        unlock_seconds_range: [
            minimum_unlock_seconds.min(unlock_seconds),
            minimum_unlock_seconds.max(unlock_seconds),
        ],
        remove_after_seconds: inherited_number("f_adoptRemoveTime", 0.0)?,
        dismiss_cooldown_seconds: inherited_number("f_adoptDismissCooldown", 1.0)?,
    })
}

pub(super) fn lower_authored_animal_adoption_count_range(
    candidate: &RecordView<'_, '_>,
    sex: Sex,
) -> Result<[u16; 2], BindError> {
    let Some(authored_count) = find_inherited_animal_shared_attribute(candidate, "f_adoptCount")
    else {
        return Ok(match sex {
            Sex::Female => [1, 5],
            Sex::Male => [0, 2],
            Sex::Any => [0, 0],
        });
    };
    let trimmed = authored_count.trim();
    let parse_count = |value: &str| {
        value.trim().parse::<u16>().map_err(|_| {
            BindError::record(
                candidate,
                format!("animal adoption count has invalid value {authored_count}"),
            )
        })
    };
    let Some(arguments) = trimmed
        .strip_prefix("rand(")
        .and_then(|value| value.strip_suffix(')'))
    else {
        let count = parse_count(trimmed)?;
        return Ok([count, count]);
    };
    let mut values = arguments.split(',');
    let minimum = values.next().map(parse_count).transpose()?.ok_or_else(|| {
        BindError::record(candidate, "animal adoption count range has no minimum")
    })?;
    let maximum = values.next().map(parse_count).transpose()?.ok_or_else(|| {
        BindError::record(candidate, "animal adoption count range has no maximum")
    })?;
    if values.next().is_some() || maximum < minimum {
        return Err(BindError::record(
            candidate,
            format!("animal adoption count has invalid range {authored_count}"),
        ));
    }
    Ok([minimum, maximum])
}
