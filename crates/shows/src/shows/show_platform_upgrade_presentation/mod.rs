use bevy::prelude::*;
use openzt2_game_data::ui_document::action::animal_shows::{UiShowAction, UiShowPlatformUpgrade};
use openzt2_interface_state::information::entity_selection_types::SelectionChanged;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::economy::zoo_cash_types::{UnlimitedZooCash, ZooCash};

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::information::entity_selection_types::WorldUiSelection;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;

use super::{
    show_platform_upgrade_lifecycle::find_stage_platform_upgrade,
    show_platform_upgrade_types::{
        InstalledShowPlatformUpgrade, SelectedShowPlatformUpgrade, ShowPlatformUpgradeKind,
        ShowPlatformUpgrades,
    },
    show_stage_types::ShowStage,
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiShowActions;

pub(super) fn project_show_platform_upgrade_selection_and_purchase_availability_into_authored_controls(
    documents: Res<Assets<UiDocumentAsset>>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    selected_entity: WorldUiSelection,
    mut selection_changes: MessageReader<SelectionChanged>,
    economy: Query<(Ref<ZooCash>, Option<Ref<UnlimitedZooCash>>)>,
    members: Query<&WorldMember>,
    roots: Query<(Ref<UiDocumentRoot>, Option<Ref<ChildOf>>)>,
    controllers: Query<Ref<SelectedShowPlatformUpgrade>>,
    stages: Query<Ref<ShowStage>>,
    installed: Query<&InstalledShowPlatformUpgrade>,
    stage_upgrades: Query<&ShowPlatformUpgrades>,
    changed_upgrades: Query<(), Changed<ShowPlatformUpgrades>>,
    mut removed_upgrades: RemovedComponents<ShowPlatformUpgrades>,
    mut nodes: Query<(
        &UiShowActions,
        &UiDocumentOwner,
        Option<&mut UiSelected>,
        Option<&mut UiInteractionEnabled>,
    )>,
) {
    let selection_changed = selection_changes.read().count() != 0;
    let stage = selected_entity
        .subject()
        .filter(|entity| stages.contains(*entity));
    let budget = stage.and_then(|stage| economy.get(members.get(stage).ok()?.root).ok());
    let upgrades_removed = removed_upgrades.read().any(|entity| Some(entity) == stage);
    removed_upgrades.clear();
    let projection_inputs_changed = documents.is_changed()
        || world_definition_assets.is_changed()
        || active_world_definitions.is_changed()
        || selection_changed
        || budget.as_ref().is_some_and(|(cash, unlimited)| {
            cash.is_changed() || unlimited.as_ref().is_some_and(|policy| policy.is_changed())
        })
        || roots.iter().any(|(root, parent)| {
            root.is_changed() || parent.is_some_and(|parent| parent.is_changed())
        })
        || controllers.iter().any(|controller| controller.is_changed())
        || stages.iter().any(|stage| stage.is_changed())
        || stage.is_some_and(|entity| changed_upgrades.contains(entity))
        || upgrades_removed;
    if !projection_inputs_changed {
        return;
    }
    for (range, owner, mut selected, mut interaction_enabled) in &mut nodes {
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let controller = parent.as_deref().map_or(owner.0, ChildOf::parent);
        let selected_upgrade = controllers.get(controller).ok().map(|value| *value);
        let records = range.authored_action_records(document);
        for record in records {
            match &record.action {
                UiShowAction::SelectPlatformUpgrade { upgrade } => {
                    let kind = match upgrade {
                        UiShowPlatformUpgrade::Canopy => ShowPlatformUpgradeKind::Canopy,
                        UiShowPlatformUpgrade::Television => ShowPlatformUpgradeKind::Television,
                    };
                    if let Some(selected) = selected.as_deref_mut() {
                        if selected.0 != selected_upgrade.is_some_and(|chosen| chosen.0 == kind) {
                            selected.0 = selected_upgrade.is_some_and(|chosen| chosen.0 == kind);
                        }
                    }
                }
                UiShowAction::PurchaseSelectedPlatformUpgrade => {
                    let enabled = stage.zip(selected_upgrade).is_some_and(|(stage, chosen)| {
                        let already_installed = find_stage_platform_upgrade(
                            stage,
                            chosen.0,
                            &stage_upgrades,
                            &installed,
                        )
                        .is_some();
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
                                    budget.as_ref().is_some_and(|(cash, policy)| {
                                        policy.as_ref().is_some_and(|policy| policy.0)
                                            || cash.0 .0 >= purchase_cost_cents
                                    })
                                }))
                    });
                    if let Some(interaction_enabled) = interaction_enabled.as_deref_mut() {
                        if interaction_enabled.0 != enabled {
                            interaction_enabled.0 = enabled;
                        }
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
