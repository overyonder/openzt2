use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::AssetId;

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_scenario::world_scenario_asset_set_state_and_borrowing_queries::WorldScenarios;
use crate::assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
use crate::plugins::persistence::profile_types::ProfileIndex;
use crate::plugins::ui::authored_ui_focus_state::UiFocusable;
use crate::plugins::ui::authored_ui_image_content_binding::UiImageBinding;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use super::shell_selection_types::{ShellSelection, WorldChoice, WorldChoiceView};
use crate::plugins::ui::authored_ui_action_projection_components::UiShellActions;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CampaignChoice(pub AssetId);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectedWorldCatalogueFilter {
    All,
    BiomeLocationGroup(AssetId),
    ExpansionPack(u16),
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SecondaryGlobe(pub AssetId);

#[derive(SystemParam)]
pub(super) struct WorldSelectionPresentation<'w, 's> {
    pub(super) texts: Query<'w, 's, &'static mut Text>,
    pub(super) text_nodes: Query<'w, 's, &'static mut Node>,
    pub(super) images: Query<'w, 's, &'static mut ImageNode>,
    pub(super) focusable: Query<'w, 's, &'static mut UiFocusable>,
    pub(super) enabled: Query<'w, 's, &'static mut UiInteractionEnabled>,
    pub(super) interactions: Query<'w, 's, &'static mut Interaction>,
}

#[derive(SystemParam)]
pub(super) struct WorldSelectionParams<'w, 's> {
    pub(super) commands: Commands<'w, 's>,
    pub(super) selection: Res<'w, ShellSelection>,
    pub(super) profiles: Res<'w, ProfileIndex>,
    pub(super) active_catalogue: Res<'w, WorldScenarios>,
    pub(super) catalogues: Res<'w, Assets<WorldScenarioDocumentAsset>>,
    pub(super) documents: Res<'w, Assets<UiDocumentAsset>>,
    pub(super) choices: Query<'w, 's, &'static WorldChoice>,
    pub(super) selectable: Query<'w, 's, (&'static WorldChoiceView, &'static mut UiSelected)>,
    pub(super) text_bindings:
        Query<'w, 's, (Entity, &'static UiTextBinding, Option<&'static Children>)>,
    pub(super) image_bindings: Query<'w, 's, (Entity, &'static UiImageBinding)>,
    pub(super) visible_bindings:
        Query<'w, 's, (&'static UiVisibleBinding, &'static mut Visibility)>,
    pub(super) action_ranges:
        Query<'w, 's, (Entity, &'static UiShellActions, &'static UiDocumentOwner)>,
    pub(super) roots: Query<'w, 's, &'static UiDocumentRoot>,
    pub(super) changed_views: Query<'w, 's, (), Changed<WorldChoiceView>>,
    pub(super) added_actions: Query<'w, 's, (), Added<UiShellActions>>,
    pub(super) added_text_bindings: Query<'w, 's, (), Added<UiTextBinding>>,
    pub(super) added_image_bindings: Query<'w, 's, (), Added<UiImageBinding>>,
    pub(super) added_visible_bindings: Query<'w, 's, (), Added<UiVisibleBinding>>,
    pub(super) presentation: WorldSelectionPresentation<'w, 's>,
    pub(super) active: Option<Res<'w, LocalizationPrecedenceIndex>>,
    pub(super) localizations: Res<'w, Assets<LocalizationAsset>>,
}
