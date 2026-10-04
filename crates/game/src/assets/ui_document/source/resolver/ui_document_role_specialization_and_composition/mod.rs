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
    let mode_node =
        find_authored_hotkey_mode(hotkeys, mode).map_err(|count| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: hotkeys.path.key(),
            span: hotkeys.root.span,
            message: format!(
                "role {role:?} global hotkey mode {mode:?} resolved to {count} authored nodes"
            ),
        })?;
    source
        .root
        .hotkeys
        .extend(mode_node.hotkeys.iter().cloned());
    Ok(())
}

/// Replaces each `<UIHotKeys><file name node/>` reference with the bindings of
/// that mode, on the node that declares it. Dialogs name their Escape bindings
/// this way (`esccancel`, `escclose`, `esctextcancel`, ...).
pub(super) fn include_referenced_authored_hotkey_modes(
    source: &mut SourceUiDocument,
    hotkeys: &SourceUiDocument,
) -> Result<(), UiSourceDocumentGap> {
    fn include(
        node: &mut SourceUiNode,
        hotkeys: &SourceUiDocument,
        virtual_path: &str,
    ) -> Result<(), UiSourceDocumentGap> {
        let span = node.span;
        let gap = |message: String| UiSourceDocumentGap {
            family: UiSourceDocumentFamily::Ui,
            kind: UiSourceDocumentGapKind::UnsupportedVocabulary,
            virtual_path: virtual_path.to_owned(),
            span,
            message,
        };
        for reference in std::mem::take(&mut node.hotkey_mode_references) {
            if reference.file.key() != hotkeys.path.key() {
                return Err(gap(format!(
                    "hotkey mode {:?} names unsupported hotkey document {:?}",
                    reference.mode,
                    reference.file.key()
                )));
            }
            let mode_node =
                find_authored_hotkey_mode(hotkeys, &reference.mode).map_err(|count| {
                    gap(format!(
                        "hotkey mode {:?} resolved to {count} authored nodes",
                        reference.mode
                    ))
                })?;
            node.hotkeys.extend(mode_node.hotkeys.iter().cloned());
        }
        node.children
            .iter_mut()
            .try_for_each(|child| include(child, hotkeys, virtual_path))
    }

    let virtual_path = source.path.key();
    include(&mut source.root, hotkeys, &virtual_path)
}

/// The one mode node with this name, or the number of matches.
fn find_authored_hotkey_mode<'a>(
    hotkeys: &'a SourceUiDocument,
    mode: &str,
) -> Result<&'a SourceUiNode, usize> {
    fn find<'a>(node: &'a SourceUiNode, mode: &str, matches: &mut Vec<&'a SourceUiNode>) {
        if node
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(mode))
        {
            matches.push(node);
        }
        node.children
            .iter()
            .for_each(|child| find(child, mode, matches));
    }

    let mut matches = Vec::new();
    find(&hotkeys.root, mode, &mut matches);
    match matches.as_slice() {
        [mode_node] => Ok(mode_node),
        matches => Err(matches.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
    };

    fn hotkey_document() -> SourceUiDocument {
        let parsed = parse_blue_fang_source_document(
            AssetPath::new("ui/hotkeys/hotkeys.xml"),
            br#"<hotkeys>
                <esctextcancel>
                    <down code="27" msg="UI_CHILD" data="UIChildData" name="TextButton Cancel">
                        <child msg="UI_ACTIVATE"/>
                    </down>
                </esctextcancel>
                <gamemode><down code="80" msg="ZT_PAUSE_KEY"/></gamemode>
            </hotkeys>"#,
        )
        .unwrap();
        SourceUiDocument::parse_hotkey_modes(&parsed).unwrap()
    }

    fn load_dialog(mode: &str) -> SourceUiDocument {
        let source = format!(
            r#"<UILayout name="Load Game Shell" modal="true">
                <UIHotKeys><file name="UI/hotkeys/hotkeys.xml" node="{mode}"/></UIHotKeys>
                <children><UIButton name="TextButton Cancel"/></children>
            </UILayout>"#
        );
        let parsed = parse_blue_fang_source_document(
            AssetPath::new("ui/layout/load.xml"),
            source.as_bytes(),
        )
        .unwrap();
        SourceUiDocument::parse(&parsed)
    }

    #[test]
    fn a_dialog_receives_the_escape_binding_its_hotkey_file_reference_names() {
        let mut dialog = load_dialog("esctextcancel");
        assert_eq!(dialog.root.hotkey_mode_references.len(), 1);
        assert!(dialog.root.hotkeys.is_empty());

        include_referenced_authored_hotkey_modes(&mut dialog, &hotkey_document()).unwrap();

        assert!(dialog.root.hotkey_mode_references.is_empty());
        let [escape] = dialog.root.hotkeys.as_slice() else {
            panic!(
                "expected one Escape binding, found {:?}",
                dialog.root.hotkeys
            );
        };
        assert_eq!(escape.code, Some(27));
        assert_eq!(escape.event.message, "UI_CHILD");
        assert_eq!(
            escape.event.target_child.as_deref(),
            Some("TextButton Cancel")
        );
        assert!(dialog.root.children[0].hotkeys.is_empty());
    }

    #[test]
    fn an_unknown_hotkey_mode_reference_is_rejected() {
        let mut dialog = load_dialog("escmissing");
        assert!(include_referenced_authored_hotkey_modes(&mut dialog, &hotkey_document()).is_err());
    }
}
