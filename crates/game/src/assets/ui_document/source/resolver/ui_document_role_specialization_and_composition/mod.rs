//! UI document role specialization and authored composition.

use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::assets::source_document::ui::{model::SourceUiNode, parser::SourceUiDocument};

use super::{
    super::ui_source_document_gap::{
        UiSourceDocumentFamily, UiSourceDocumentGap, UiSourceDocumentGapKind,
    },
    main_menu_ui_role_source_specialization::specialize_main_menu_ui_source_document_for_semantic_role,
};

pub(super) fn specialize_authored_ui_source_document_for_role(
    source: &SourceUiDocument,
    role: UiDocumentRole,
) -> Result<SourceUiDocument, UiSourceDocumentGap> {
    let mut source = source.clone();
    specialize_main_menu_ui_source_document_for_semantic_role(&mut source, role);
    if matches!(
        role,
        UiDocumentRole::MainMenu | UiDocumentRole::MainMenuBackdrop
    ) {
        // Main-menu specialization already selects the menu and removes the splash.
        return Ok(source);
    }
    if !matches!(role, UiDocumentRole::Globe | UiDocumentRole::MapSelect) {
        return Ok(source);
    }
    fn select_map_screen(node: &mut SourceUiNode, role: UiDocumentRole) -> bool {
        let mut found = false;
        if node.name.as_deref() == Some("Map Selection") {
            node.state.visible = true;
            found = true;
        }
        match node.name.as_deref() {
            Some("Campaign Selection Layout") => {
                node.state.visible = role == UiDocumentRole::MapSelect;
            }
            Some("Location Selection Layout") => {
                node.state.visible = role == UiDocumentRole::Globe;
            }
            _ => {}
        }
        node.children.iter_mut().fold(found, |found, child| {
            select_map_screen(child, role) || found
        })
    }
    if !select_map_screen(&mut source.root, role) {
        return Err(UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: source.path.key(),
            span: source.root.span,
            message: "map-selection role source has no authored Map Selection screen".into(),
        });
    }
    Ok(source)
}

pub(super) fn expose_authored_ui_surface_for_document_role(
    source: &mut SourceUiDocument,
    role: UiDocumentRole,
    surface: &str,
) -> Result<(), UiSourceDocumentGap> {
    fn show(node: &mut SourceUiNode, surface: &str) -> (usize, bool) {
        let self_matched = node
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(surface));
        let (descendant_count, has_matching_descendant) = node
            .children
            .iter_mut()
            .map(|child| show(child, surface))
            .fold((0, false), |(count, found), (child_count, child_found)| {
                (count + child_count, found || child_found)
            });
        let path_matched = self_matched || has_matching_descendant;
        if path_matched {
            // Opening a nested authored surface necessarily exposes its
            // containing layouts. Leaving a hidden modal ancestor in place
            // discards its dimmer, clipping and show-event scope while its
            // child is projected as though it were a detached screen.
            node.state.visible = true;
        }
        (usize::from(self_matched) + descendant_count, path_matched)
    }

    if role == UiDocumentRole::ChallengeOffer {
        // This source file owns separate branch, instant, offer and result
        // modals. The offer role installs only its authored surface beneath
        // the original canvas; other dialogs retain their separate semantics.
        source.root.children.retain(|child| {
            child
                .name
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(surface))
        });
    }
    match show(&mut source.root, surface).0 {
        1 => Ok(()),
        count => Err(UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: source.path.key(),
            span: source.root.span,
            message: format!(
                "role {role:?} open surface {surface:?} resolved to {count} authored nodes"
            ),
        }),
    }
}

pub(super) fn attach_authored_global_hotkey_mode_to_document_role(
    source: &mut SourceUiDocument,
    role: UiDocumentRole,
    hotkeys: &SourceUiDocument,
    mode: &str,
) -> Result<(), UiSourceDocumentGap> {
    fn find_mode<'a>(node: &'a SourceUiNode, mode: &str, matches: &mut Vec<&'a SourceUiNode>) {
        if node
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(mode))
        {
            matches.push(node);
        }
        node.children
            .iter()
            .for_each(|child| find_mode(child, mode, matches));
    }

    let mut matches = Vec::new();
    find_mode(&hotkeys.root, mode, &mut matches);
    let [mode_node] = matches.as_slice() else {
        return Err(UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: hotkeys.path.key(),
            span: hotkeys.root.span,
            message: format!(
                "role {role:?} global hotkey mode {mode:?} resolved to {} authored nodes",
                matches.len()
            ),
        });
    };
    source
        .root
        .hotkeys
        .extend(mode_node.hotkeys.iter().cloned());
    Ok(())
}
