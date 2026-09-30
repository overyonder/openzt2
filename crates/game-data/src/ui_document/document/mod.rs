use super::{
    gameplay_pointer_presentation::{
        UiConstructionPlacementPreviewDefinition, UiGameplayInteractionCursorDefinition,
    },
    image_selection::UiImageSelectionGroupDefinition,
    node::UiNodeDefinition,
};
use crate::AssetId;
use serde::{Deserialize, Serialize};

mod document_role_operations;

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiDocument {
    pub id: AssetId,
    pub role: UiDocumentRole,
    pub logical_size: [f32; 2],
    pub nodes: Vec<UiNodeDefinition>,
    pub gameplay_interaction_cursors: UiGameplayInteractionCursorDefinition,
    pub construction_placement_preview: UiConstructionPlacementPreviewDefinition,
    pub image_selection_groups: Vec<UiImageSelectionGroupDefinition>,
    pub dependencies: Vec<UiDependency>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct UiDependency {
    pub id: AssetId,
    pub path: String,
    pub kind: UiDependencyKind,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiDependencyKind {
    Texture,
    InteractiveTexture,
    Scene,
    Document,
    Audio,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum UiDocumentRole {
    ApplicationRoot,
    Splash,
    MainMenu,
    MainMenuBackdrop,
    ProfileSelect,
    Globe,
    MapSelect,
    Loading,
    Options,
    InGameOptions,
    SavedGames,
    InGameHud,
    InGamePersistentStatus,
    TranquilizerHud,
    Modal,
    EntityInfo,
    PurchaseCatalogue,
    ZooStatus,
    MultiList,
    Zoopedia,
    Scenario,
    PhotoAlbum,
    ShowEditor,
    Downloads,
    ModeHelp,
    Overview,
    Fragment,
    PhotoMode,
    AnimalCareCatalogue,
    ChallengeOffer,
    SaveGame,
}
