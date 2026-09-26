//! Bevy asset-path selection for one UI document role.

use openzt2_game_data::ui_document::document::UiDocumentRole;

pub(crate) fn select_bevy_asset_path_for_canonical_ui_document_role(
    requested_role: UiDocumentRole,
) -> Option<String> {
    let role_and_source_path_declarations = super::ui_document_role_source_path_declarations::
        UI_DOCUMENT_ROLE_AND_SOURCE_PATH_DECLARATIONS;
    let requested_source_path = role_and_source_path_declarations
        .iter()
        .find_map(|(role, source_path)| (*role == requested_role).then_some(source_path))?;
    let primary_role_for_requested_source_path = role_and_source_path_declarations
        .iter()
        .find_map(|(role, source_path)| (source_path == requested_source_path).then_some(*role));
    Some(
        if primary_role_for_requested_source_path == Some(requested_role) {
            (*requested_source_path).to_owned()
        } else {
            format!(
                "{requested_source_path}#Role/{}",
                requested_role.stable_key()
            )
        },
    )
}
