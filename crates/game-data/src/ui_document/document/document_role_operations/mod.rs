use super::UiDocumentRole;

impl UiDocumentRole {
    #[must_use]
    pub fn node_id(self, name: &str) -> crate::AssetId {
        crate::AssetId::from_key(&format!(
            "ui/role/{}/node/{}",
            self.stable_key(),
            name.trim().to_ascii_lowercase()
        ))
    }

    #[must_use]
    pub const fn stable_key(self) -> &'static str {
        match self {
            Self::ApplicationRoot => "application-root",
            Self::Splash => "splash",
            Self::MainMenu => "main-menu",
            Self::MainMenuBackdrop => "main-menu-backdrop",
            Self::ProfileSelect => "profile-select",
            Self::Globe => "globe",
            Self::MapSelect => "map-select",
            Self::Loading => "loading",
            Self::Options => "options",
            Self::InGameOptions => "in-game-options",
            Self::SavedGames => "saved-games",
            Self::InGameHud => "in-game-hud",
            Self::InGamePersistentStatus => "in-game-persistent-status",
            Self::TranquilizerHud => "tranquilizer-hud",
            Self::Modal => "modal",
            Self::EntityInfo => "entity-info",
            Self::PurchaseCatalogue => "purchase-catalogue",
            Self::ZooStatus => "zoo-status",
            Self::MultiList => "multi-list",
            Self::Zoopedia => "zoopedia",
            Self::Scenario => "scenario",
            Self::ChallengeOffer => "challenge-offer",
            Self::PhotoAlbum => "photo-album",
            Self::PhotoMode => "photo-mode",
            Self::AnimalCareCatalogue => "animal-care-catalogue",
            Self::ShowEditor => "show-editor",
            Self::Downloads => "downloads",
            Self::ModeHelp => "mode-help",
            Self::Overview => "overview",
            Self::Fragment => "fragment",
        }
    }
}
