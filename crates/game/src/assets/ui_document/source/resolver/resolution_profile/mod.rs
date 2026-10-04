//! Selected UI source inputs and their direct source-loading policy.

use std::collections::{BTreeMap, BTreeSet};

use openzt2_game_data::ui_document::document::UiDocumentRole;

use super::super::lower::authored_ui_document_lowering::UiResolvedDependencies;

/// Final selected-layer UI inputs. Documents must be supplied in resolved archive
/// order; a later document with the same normalized path is the winner.
#[derive(Clone, Debug)]
pub(in crate::assets::ui_document::source) struct SelectedUiSourceResolutionProfile {
    pub(in crate::assets::ui_document::source) role_paths: Vec<(UiDocumentRole, String)>,
    /// Documents inserted beneath each screen's root.
    pub(super) role_compositions: Vec<(UiDocumentRole, String)>,
    /// Hotkey modes that role documents attach or reference by name.
    pub(super) hotkey_path: Option<String>,
    /// Original gameplay-mode tree which owns pointer cursor selection.
    pub(super) interaction_mode_path: Option<String>,
    pub(super) role_hotkey_modes: Vec<(UiDocumentRole, String)>,
    /// Hidden source nodes extracted as separate screens.
    pub(super) role_surfaces: Vec<(UiDocumentRole, String)>,
    /// Nodes inside another role's document that other documents address by
    /// name, such as dialogs they open. The names resolve to that role without
    /// loading it.
    pub(super) role_addressed_nodes: Vec<(UiDocumentRole, String)>,
    pub(super) theme: Option<String>,
    /// Expansion availability reported to the original UI loader. This is
    /// deliberately independent of which archives supplied data: Complete
    /// Collection loads maps and objects from every archive while its UI
    /// availability registry reports no numbered expansion guards.
    pub(super) available_xpacks: BTreeSet<u32>,
    pub(in crate::assets::ui_document::source) available_assets: Vec<String>,
    pub(in crate::assets::ui_document::source) resolved_dependencies: UiResolvedDependencies,
    /// Selected source order, lowest precedence first. This is independent of
    /// normalized path order because named templates and skins share a global
    /// authored namespace.
    pub(in crate::assets::ui_document::source) source_precedence: BTreeMap<String, (u32, u32)>,
}

impl SelectedUiSourceResolutionProfile {
    fn core() -> Self {
        Self {
            role_paths: crate::assets::ui_document::ui_document_role_source_path_declarations::UI_DOCUMENT_ROLE_AND_SOURCE_PATH_DECLARATIONS
                .iter()
                .map(|(role, source_path)| (*role, (*source_path).to_owned()))
                .collect(),
            role_compositions: vec![
                // Edge scrolling includes its hover regions, cursors and pan events.
                (UiDocumentRole::InGameHud, "ui/layout/edgescroll.xml".into()),
                (UiDocumentRole::InGameHud, "ui/layout/pause.xml".into()),
                // The money cursor uses this layout's 1024x768 coordinate space.
                (UiDocumentRole::InGameHud, "ui/layout/cursormoneylayout.xml".into()),
                (UiDocumentRole::InGameHud, "ui/layout/cameraset.xml".into()),
                (UiDocumentRole::InGameHud, "ui/layout/message.xml".into()),
            ],
            hotkey_path: Some("ui/hotkeys/hotkeys.xml".into()),
            interaction_mode_path: Some("ui/modes/modes.xml".into()),
            role_hotkey_modes: vec![
                (UiDocumentRole::MainMenu, "mainmode".into()),
                (UiDocumentRole::InGameHud, "gamemode".into()),
                (UiDocumentRole::Overview, "overview".into()),
                (UiDocumentRole::PhotoMode, "photomode".into()),
            ],
            role_surfaces: vec![
                (UiDocumentRole::ProfileSelect, "profile_manager_dialog".into()),
                (UiDocumentRole::AnimalCareCatalogue, "Sort Panel".into()),
                (UiDocumentRole::InGameOptions, "In Game Options".into()),
                (UiDocumentRole::TranquilizerHud, "tranq_gun_screen".into()),
                (UiDocumentRole::ZooStatus, "zoo status layout".into()),
                (UiDocumentRole::MultiList, "multilist layout".into()),
                (UiDocumentRole::Zoopedia, "ztzoopedia".into()),
                (UiDocumentRole::Scenario, "goals layout".into()),
                (UiDocumentRole::ChallengeOffer, "challenge layout".into()),
                (UiDocumentRole::PhotoAlbum, "photoalbum_layout".into()),
                (UiDocumentRole::ModeHelp, "modehelpmainlayout".into()),
                (UiDocumentRole::Overview, "overview_screen".into()),
            ],
            // The main menu's Exit button opens this confirmation in confirm.xml.
            role_addressed_nodes: vec![(UiDocumentRole::Modal, "exit_zoo".into())],
            theme: None,
            available_xpacks: BTreeSet::new(),
            available_assets: Vec::new(),
            resolved_dependencies: UiResolvedDependencies::default(),
            source_precedence: BTreeMap::new(),
        }
    }

    pub(in crate::assets::ui_document::source) fn for_primary_source_path(path: &str) -> Self {
        let path = path.replace('\\', "/").to_ascii_lowercase();
        let mut profile = Self::core();
        profile
            .role_paths
            .retain(|(_, candidate)| candidate.eq_ignore_ascii_case(&path));
        profile.role_compositions.retain(|(role, _)| {
            profile
                .role_paths
                .iter()
                .any(|(candidate, _)| candidate == role)
        });
        profile.role_hotkey_modes.retain(|(role, _)| {
            profile
                .role_paths
                .iter()
                .any(|(candidate, _)| candidate == role)
        });
        if profile.role_paths.is_empty() {
            profile.hotkey_path = None;
        }
        if !profile
            .role_paths
            .iter()
            .any(|(role, _)| *role == UiDocumentRole::InGameHud)
        {
            profile.interaction_mode_path = None;
        }
        // Every primary document must retain the authored names which open
        // the other canonical roles. The destination documents remain
        // demand-loaded; these declarations only let source events such as
        // `UI_CHILD photoalbum_layout -> UI_SHOW` lower to a typed role
        // transition instead of a nonexistent local-node mutation.
        profile
    }

    pub(in crate::assets::ui_document::source) fn supporting_source_paths(
        &self,
    ) -> BTreeSet<String> {
        if self.role_paths.is_empty() {
            return BTreeSet::new();
        }
        self.role_compositions
            .iter()
            .map(|(_, path)| path.clone())
            .chain(self.hotkey_path.iter().cloned())
            .chain(self.interaction_mode_path.iter().cloned())
            .chain(
                self.role_paths
                    .iter()
                    .filter(|(role, _)| *role == UiDocumentRole::InGameHud)
                    .flat_map(|_| {
                        [
                            "ui/layout/adoptbutton.xml".to_owned(),
                            "ui/layout/blankslot.xml".to_owned(),
                            "ui/template/adopt.xml".to_owned(),
                        ]
                    }),
            )
            .chain(["ztapp4.xml".to_owned()])
            .collect()
    }
}
