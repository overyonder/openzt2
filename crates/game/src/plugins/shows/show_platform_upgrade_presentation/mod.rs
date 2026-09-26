use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowPlatformUpgrade};

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::information::entity_selection_types::SelectedEntity;
use crate::plugins::ui::authored_ui_focus_state::UiFocusable;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;

use super::{
    show_platform_upgrade_types::{
        InstalledShowPlatformUpgrade, InstalledShowPlatformUpgradeStage,
        SelectedShowPlatformUpgrade, ShowPlatformUpgradeKind,
    },
    show_stage_types::ShowStage,
};
use crate::plugins::ui::authored_ui_action_projection_components::UiShowActions;

pub(super) fn project_show_platform_upgrade_selection_and_purchase_availability_into_authored_controls(
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected_entity: Res<SelectedEntity>,
    cash: Res<crate::plugins::economy::zoo_cash_types::ZooCash>,
    unlimited_cash: Option<Res<crate::plugins::economy::zoo_cash_types::UnlimitedZooCash>>,
    roots: Query<(Ref<UiDocumentRoot>, Option<Ref<ChildOf>>)>,
    controllers: Query<Ref<SelectedShowPlatformUpgrade>>,
    stages: Query<Ref<ShowStage>>,
    installed: Query<(
        Ref<InstalledShowPlatformUpgrade>,
        Ref<InstalledShowPlatformUpgradeStage>,
    )>,
    mut nodes: Query<(
        &UiShowActions,
        &UiDocumentOwner,
        Option<&mut UiSelected>,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiFocusable>,
    )>,
) {
    let projection_inputs_changed = documents.is_changed()
        || world_definition_assets.is_changed()
        || active_world_definitions.is_changed()
        || selected_entity.is_changed()
        || cash.is_changed()
        || unlimited_cash
            .as_ref()
            .is_some_and(|cash| cash.is_changed())
        || roots.iter().any(|(root, parent)| {
            root.is_changed() || parent.is_some_and(|parent| parent.is_changed())
        })
        || controllers.iter().any(|controller| controller.is_changed())
        || stages.iter().any(|stage| stage.is_changed())
        || installed
            .iter()
            .any(|(upgrade, stage)| upgrade.is_changed() || stage.is_changed());
    if !projection_inputs_changed {
        return;
    }
    for (range, owner, mut selected, mut interaction_enabled, mut focusable) in &mut nodes {
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let controller = parent.as_deref().map_or(owner.0, ChildOf::parent);
        let selected_upgrade = controllers.get(controller).ok().map(|value| *value);
        let stage = selected_entity.0.filter(|entity| stages.contains(*entity));
        let records = range.authored_action_records(document);
        for record in records {
            match &record.action {
                UiShowAction::SelectPlatformUpgrade { upgrade } => {
                    let kind = match upgrade {
                        UiShowPlatformUpgrade::Canopy => ShowPlatformUpgradeKind::Canopy,
                        UiShowPlatformUpgrade::Television => ShowPlatformUpgradeKind::Television,
                    };
                    if let Some(selected) = selected.as_deref_mut() {
                        selected.0 = selected_upgrade.is_some_and(|chosen| chosen.0 == kind);
                    }
                }
                UiShowAction::PurchaseSelectedPlatformUpgrade => {
                    let enabled = stage.zip(selected_upgrade).is_some_and(|(stage, chosen)| {
                        let already_installed = installed
                            .iter()
                            .any(|(kind, owner)| kind.0 == chosen.0 && owner.0 == stage);
                        let purchase_cost_cents = active_world_definitions
                            .get(&world_definition_assets)
                            .and_then(|world_definitions| {
                                world_definitions.show_platform_upgrades()
                            })
                            .map(|platform_upgrade_policy| match chosen.0 {
                                ShowPlatformUpgradeKind::Canopy => {
                                    platform_upgrade_policy.canopy_cost_cents
                                }
                                ShowPlatformUpgradeKind::Television => {
                                    platform_upgrade_policy.television_cost_cents
                                }
                            });
                        already_installed
                            || (stages.contains(stage)
                                && purchase_cost_cents.is_some_and(|purchase_cost_cents| {
                                    unlimited_cash.is_some() || cash.0 .0 >= purchase_cost_cents
                                }))
                    });
                    if let Some(interaction_enabled) = interaction_enabled.as_deref_mut() {
                        interaction_enabled.0 = enabled;
                    }
                    if let Some(focusable) = focusable.as_deref_mut() {
                        focusable.enabled = enabled;
                    }
                }
                UiShowAction::ToggleSelectedEnabled
                | UiShowAction::EditSelected
                | UiShowAction::CancelEdit
                | UiShowAction::SetSelectedName
                | UiShowAction::ShowMixerPanel
                | UiShowAction::ToggleEditing
                | UiShowAction::DismissMixerDropdown
                | UiShowAction::SetSelectedOpen { .. }
                | UiShowAction::RequestAddShow
                | UiShowAction::AddShow
                | UiShowAction::RequestDeleteSelected
                | UiShowAction::DeleteSelected
                | UiShowAction::ViewSelected
                | UiShowAction::AddBreak
                | UiShowAction::MoveSelectedUp
                | UiShowAction::MoveSelectedDown
                | UiShowAction::ResolvePlatformDeletion { .. } => {}
            }
        }
    }
}
