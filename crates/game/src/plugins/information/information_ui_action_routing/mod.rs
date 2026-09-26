use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::UiInformationAction;

use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentRoot;
use crate::plugins::ui::authored_ui_node_projection_components::UiNodeId;

use super::{
    catalogue_types::SelectedCatalogueEntry,
    entity_selection_types::Inspectable,
    information_graph_types::InformationGraph,
    information_list_types::InformationEntityListPanel,
    information_view_types::{EntityEditorDataRootSurface, InformationViewFilters},
    zoopedia::zoopedia_navigation_types::{ZoopediaHistory, ZoopediaPage},
};
use crate::plugins::ui::authored_ui_action_projection_components::UiInformationActions;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;

/// Routes each authored information action to its direct domain owner. The UI
/// activation identifies the authored node and trigger; concrete settings,
/// selection, filtering, lists, graphs, catalogue, and Zoopedia work remains
/// in the procedure owner named by each match arm.
#[derive(SystemParam)]
pub(super) struct InformationActionParams<'w, 's> {
    activations: MessageReader<'w, 's, UiNodeActivated>,
    documents: Res<'w, Assets<UiDocumentAsset>>,
    world_definition_assets: Res<'w, Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<'w, WorldDefinitions>,
    action_nodes: Query<'w, 's, (&'static UiInformationActions, &'static UiDocumentOwner)>,
    roots: Query<'w, 's, (&'static UiDocumentRoot, &'static ChildOf)>,
    selection: super::information_selection_action_types::InformationSelectionActionTargets<'w, 's>,
    settings: crate::plugins::settings::settings_action_types::SettingsActionTargets<'w, 's>,
    lists: Query<'w, 's, (Entity, &'static mut InformationEntityListPanel)>,
    ui_nodes: Query<'w, 's, (Entity, &'static UiDocumentOwner, &'static UiNodeId)>,
    selected_catalogue: Res<'w, SelectedCatalogueEntry>,
    inspectables: Query<'w, 's, &'static Inspectable>,
    zoopedia_pages: Query<
        'w,
        's,
        (
            Entity,
            &'static UiDocumentOwner,
            &'static mut ZoopediaPage,
            Option<&'static mut ZoopediaHistory>,
        ),
    >,
    graphs: Query<'w, 's, (&'static UiDocumentOwner, &'static mut InformationGraph)>,
    view_filters: ResMut<'w, InformationViewFilters>,
    overview_exports: MessageWriter<'w, super::overview::overview_types::ExportOverviewMap>,
    commands: Commands<'w, 's>,
}

pub(super) fn route_authored_information_actions_from_activated_ui_nodes(
    params: InformationActionParams,
) {
    let InformationActionParams {
        mut activations,
        documents,
        world_definition_assets,
        active_world_definitions,
        action_nodes,
        roots,
        mut selection,
        mut settings,
        mut lists,
        ui_nodes,
        selected_catalogue,
        inspectables,
        mut zoopedia_pages,
        mut graphs,
        mut view_filters,
        mut overview_exports,
        mut commands,
    } = params;
    for activation in activations.read() {
        let Ok((range, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Ok((root, lifecycle_owner)) = roots.get(owner.0) else {
            continue;
        };
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let records = range.authored_action_records(document);
        for record in records {
            if activation.trigger != record.trigger {
                continue;
            }
            match &record.action {
                UiInformationAction::ExportOverviewMap { return_control } => {
                    overview_exports.write(super::overview::overview_types::ExportOverviewMap {
                        document_owner: owner.0,
                        export_control: activation.node,
                        return_control: *return_control,
                    });
                }
                UiInformationAction::RenameSelected
                | UiInformationAction::RenameZoo
                | UiInformationAction::SelectNextDiseasedAnimal
                | UiInformationAction::SelectNextRampagingAnimal
                | UiInformationAction::SelectEntityFromSource => {
                    super::information_selection_action_operations::apply_authored_information_selection_action(
                        &record.action,
                        activation.node,
                        activation.source,
                        &mut selection,
                    );
                }
                UiInformationAction::SetViewFilter { category, visible } => {
                    super::information_view_filter_operations::set_information_view_category_visibility_from_authored_action(
                        *category,
                        *visible,
                        &mut view_filters,
                    );
                }
                UiInformationAction::PopulateEntityList { .. }
                | UiInformationAction::SortEntityList { .. }
                | UiInformationAction::SortEntityListDirected { .. }
                | UiInformationAction::SortEntityListByNeed { .. }
                | UiInformationAction::SortAnimalsByHappiness { .. }
                | UiInformationAction::SortAnimalsByPregnancy { .. }
                | UiInformationAction::SortGuestsByFavouriteAnimal { .. } => {
                    super::information_entity_list_operations::apply_authored_information_entity_list_action(
                        &record.action,
                        owner.0,
                        &ui_nodes,
                        &mut lists,
                        &mut commands,
                    );
                }
                UiInformationAction::ApplySetting { setting } => {
                    crate::plugins::settings::settings_ui_routing_and_projection::apply_authored_setting_action(
                        setting,
                        &mut settings,
                        &mut commands,
                    );
                }
                UiInformationAction::OpenEncyclopediaEntry { .. }
                | UiInformationAction::OpenContextEncyclopediaEntry
                | UiInformationAction::ZoopediaBack
                | UiInformationAction::ZoopediaForward => {
                    super::zoopedia::zoopedia_navigation_operations::apply_authored_zoopedia_navigation_action(
                        &record.action,
                        &mut super::zoopedia::zoopedia_navigation_types::ZoopediaNavigationActionContext {
                            source_document_role: &document.canonical_ui_document().role,
                            source_document_entity: owner.0,
                            source_lifecycle_owner_entity: lifecycle_owner.parent(),
                            selected_world_entity: selection.selected_entity.0,
                            selected_catalogue_entry: &selected_catalogue,
                            world_definitions: active_world_definitions
                                .get(&world_definition_assets),
                            inspectable_world_entities: &inspectables,
                            document_roots: &roots,
                            zoopedia_pages: &mut zoopedia_pages,
                            commands: &mut commands,
                        },
                    );
                }
                UiInformationAction::SetResearchFilter { .. }
                | UiInformationAction::SetCatalogueKindFilter { .. }
                | UiInformationAction::ClearTypeFilter { .. } => {
                    super::catalogue::type_list_filter_operations::apply_authored_catalogue_type_list_filter_action(
                        &record.action,
                        owner.0,
                        &ui_nodes,
                        &mut commands,
                    );
                }
                UiInformationAction::PopulateEntityEditorDataRoots => {
                    commands
                        .entity(activation.node)
                        .insert(EntityEditorDataRootSurface);
                }
                UiInformationAction::SelectGraph { .. }
                | UiInformationAction::SelectGraphType { .. } => {
                    super::information_graph_operations::apply_authored_information_graph_action(
                        &record.action,
                        owner.0,
                        activation.node,
                        &mut graphs,
                        &mut commands,
                    );
                }
            }
        }
    }
}
