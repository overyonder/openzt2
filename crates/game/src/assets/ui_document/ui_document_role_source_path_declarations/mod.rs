//! Ordered UI role and source-document path declarations.

use openzt2_game_data::ui_document::document::UiDocumentRole;

pub(in crate::assets::ui_document) const UI_DOCUMENT_ROLE_AND_SOURCE_PATH_DECLARATIONS: &[(
    UiDocumentRole,
    &str,
)] = &[
    // The loose application descriptor owns the two process-wide tooltip
    // presentations beneath its embedded UIRoot.
    (UiDocumentRole::ApplicationRoot, "ztapp4.xml"),
    // The shipped main-menu document owns both the timed splash sequence and
    // the interactive menu. Lowering projects it once for each live role.
    (UiDocumentRole::Splash, "ui/layout/mainmenu.xml"),
    (UiDocumentRole::MainMenu, "ui/layout/mainmenu.xml"),
    (UiDocumentRole::MainMenuBackdrop, "ui/layout/mainmenu.xml"),
    (UiDocumentRole::ProfileSelect, "ui/layout/profiledialog.xml"),
    (UiDocumentRole::MapSelect, "ui/layout/freeformselection.xml"),
    (UiDocumentRole::Globe, "ui/layout/freeformselection.xml"),
    (UiDocumentRole::Loading, "ui/layout/loadingscreen.xml"),
    (UiDocumentRole::Options, "ui/layout/options.xml"),
    (UiDocumentRole::InGameOptions, "ui/layout/ingameoptions.xml"),
    (UiDocumentRole::SavedGames, "ui/layout/save.xml"),
    (UiDocumentRole::InGameHud, "ui/layout/shell.xml"),
    (
        UiDocumentRole::InGamePersistentStatus,
        "ui/layout/zoobucksdisplay.xml",
    ),
    (UiDocumentRole::TranquilizerHud, "ui/layout/tranqm.xml"),
    (UiDocumentRole::Modal, "ui/layout/confirm.xml"),
    (UiDocumentRole::EntityInfo, "ui/layout/entityinfo.xml"),
    (UiDocumentRole::PurchaseCatalogue, "ui/layout/buyinfo.xml"),
    (UiDocumentRole::AnimalCareCatalogue, "ui/layout/sort.xml"),
    (UiDocumentRole::ZooStatus, "ui/layout/zoostatus.xml"),
    (UiDocumentRole::MultiList, "ui/layout/multilist.xml"),
    (UiDocumentRole::Zoopedia, "ui/layout/zoopedia.xml"),
    (UiDocumentRole::Scenario, "ui/layout/goals.xml"),
    (UiDocumentRole::ChallengeOffer, "ui/layout/challenge.xml"),
    (UiDocumentRole::PhotoAlbum, "ui/layout/photoalbum.xml"),
    (UiDocumentRole::PhotoMode, "ui/layout/photom.xml"),
    (UiDocumentRole::ShowEditor, "ui/layout/showmixer/mixer.xml"),
    (UiDocumentRole::Downloads, "ui/layout/downloads.xml"),
    (UiDocumentRole::ModeHelp, "ui/layout/modehelp.xml"),
    (UiDocumentRole::Overview, "ui/layout/overview.xml"),
];
