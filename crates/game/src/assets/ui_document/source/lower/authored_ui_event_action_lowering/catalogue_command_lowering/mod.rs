use crate::assets::source_document::ui::model::SourceUiEvent;
use crate::assets::ui_document::source::lower::authored_ui_action_argument_lowering::target_node;
use crate::assets::ui_document::source::lower::authored_ui_document_lowering::AuthoredUiDocument;
use crate::assets::ui_document::source::lower::authored_ui_event_collection_lowering::information_list_category;
use openzt2_game_data::ui_document::action::catalogue_type_list::UiPopulateCatalogueTypeListActionRecord;
use openzt2_game_data::ui_document::action::information::{
    UiInformationAction, UiInformationActionRecord,
};
use openzt2_game_data::ui_document::action::{UiActionRecord, UiTrigger};
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::AssetId;
use std::io;

pub(super) fn lower_catalogue_command(
    trigger: UiTrigger,
    event: &SourceUiEvent,
    role: UiDocumentRole,
    current: AssetId,
    input: &AuthoredUiDocument,
) -> io::Result<Option<UiActionRecord>> {
    let result = match event.message.as_str() {
        "ZT_AUTOPOPULATE_LIST" if event.string.is_none() && event.value.is_none() => Ok(
            UiActionRecord::PopulateCatalogueTypeList(UiPopulateCatalogueTypeListActionRecord {
                trigger,
                target_type_list_node: target_node(event, role, current),
            }),
        ),
        "ZT_AUTOPOPULATE_LIST" => Ok(UiActionRecord::Information(UiInformationActionRecord {
            trigger,
            action: UiInformationAction::PopulateEntityList {
                list: target_node(event, role, current),
                category: information_list_category(event.string.as_deref(), input)?,
            },
        })),
        _ => return Ok(None),
    };
    result.map(Some)
}
