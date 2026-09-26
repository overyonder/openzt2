//! Main-menu source specialization into splash, menu, and backdrop roles.

use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::assets::source_document::ui::parser::SourceUiDocument;

pub(super) fn specialize_main_menu_ui_source_document_for_semantic_role(
    source: &mut SourceUiDocument,
    role: UiDocumentRole,
) {
    if source.path.key() != "ui/layout/mainmenu.xml" {
        return;
    }
    if role == UiDocumentRole::MainMenu {
        source.root.children.iter_mut().for_each(|child| {
            if child.name.as_deref() == Some("back ground pieces") {
                child.state.visible = true;
            }
        });
        source.root.children.retain(|child| {
            !matches!(
                child.name.as_deref(),
                Some(
                    "Splash Background"
                        | "Click To Continue"
                        | "Logo Screen"
                        | "Zoo Logo Animation"
                        | "Tools Menu",
                )
            )
        });
    } else if role == UiDocumentRole::Splash {
        source.root.children.iter_mut().for_each(|child| {
            child.state.visible = matches!(
                child.name.as_deref(),
                Some(
                    "Splash Background"
                        | "Click To Continue"
                        | "Logo Screen"
                        | "Zoo Logo Animation",
                )
            );
        });
        source.root.children.retain(|child| {
            child.name.as_deref() != Some("Main Menu Layout")
                && child.name.as_deref() != Some("Tools Menu")
        });
    } else if role == UiDocumentRole::MainMenuBackdrop {
        source
            .root
            .children
            .retain(|child| child.name.as_deref() == Some("back ground pieces"));
        source
            .root
            .children
            .iter_mut()
            .for_each(|child| child.state.visible = true);
    }
}
