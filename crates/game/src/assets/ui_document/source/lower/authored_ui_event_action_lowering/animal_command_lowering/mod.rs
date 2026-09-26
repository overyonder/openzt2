use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_scalar_value_lowering::invalid_at;
use openzt2_game_data::ui_document::action::animals::{
    UiAnimalAction, UiAnimalActionRecord, UiAnimalGender,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use std::io;

pub(super) fn lower_animal_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_SET_GENDER" => Ok(UiActionRecord::Animal(UiAnimalActionRecord {
            trigger,
            action: UiAnimalAction::SetAdoptionCatalogueGenderFilter {
                gender: match event
                    .string
                    .as_deref()
                    .or(event.value.as_deref())
                    .unwrap_or_default()
                {
                    "female" => UiAnimalGender::Female,
                    "male" => UiAnimalGender::Male,
                    value => {
                        return Err(invalid_at(
                            input,
                            format!("unsupported catalogue gender {value:?}"),
                        ));
                    }
                },
            },
        })),
        "ZT_ANIMAL_RELEASE_ZOO" => Ok(UiActionRecord::Animal(UiAnimalActionRecord {
            trigger,
            action: UiAnimalAction::ReleaseSelectedAnimalFromCrateIntoZoo,
        })),
        "ZT_ANIMAL_RELEASE_WILD" => Ok(UiActionRecord::Animal(UiAnimalActionRecord {
            trigger,
            action: UiAnimalAction::ReleaseSelectedAnimalToWild,
        })),
        "ZT_DECLINE_ADOPTION" | "ZT_DECLINE_ALL_ADOPTIONS" => {
            Ok(UiActionRecord::Animal(UiAnimalActionRecord {
                trigger,
                action: UiAnimalAction::DeclineAllCurrentAdoptionOffers,
            }))
        }
        _ => return Ok(None),
    };
    result.map(Some)
}
