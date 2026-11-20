use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::UiInformationAction;

use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{UiDocumentMembers, UiDocumentOwner};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;

use openzt2_interface_state::information::catalogue_types::{
    SelectedCatalogueEntry, TypeListFilter,
};
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_interface_state::information::information_graph_types::InformationGraph;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiRole;

use super::{
    information_list_types::InformationEntityListPanel,
    information_view_types::{EntityEditorDataRootSurface, InformationViewFilters},
    zoopedia::zoopedia_navigation_types::{PendingZoopediaEntry, ZoopediaHistory, ZoopediaPage},
};
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiInformationActions;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;

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
    lists: Query<'w, 's, (Entity, &'static mut InformationEntityListPanel)>,
    ui_nodes: Query<'w, 's, (Entity, &'static UiDocumentOwner, &'static UiNodeId)>,
    document_members: Query<'w, 's, &'static UiDocumentMembers>,
    type_list_filters: Query<'w, 's, (&'static UiNodeId, &'static mut TypeListFilter)>,
    selected_catalogue: Res<'w, SelectedCatalogueEntry>,
    inspectables: Query<'w, 's, &'static Inspectable>,
    zoopedia_pages: Query<'w, 's, (&'static mut ZoopediaPage, &'static mut ZoopediaHistory)>,
    document_children: Query<'w, 's, &'static Children>,
    pending_zoopedia_entries: Query<'w, 's, &'static mut PendingZoopediaEntry>,
    show_roles: MessageWriter<'w, ShowUiRole>,
    graphs: Query<'w, 's, (&'static UiDocumentOwner, &'static mut InformationGraph)>,
    view_filters: ResMut<'w, InformationViewFilters>,
    overview_exports: MessageWriter<
        'w,
        openzt2_interface_state::information::overview::overview_types::ExportOverviewMap,
    >,
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
        mut lists,
        ui_nodes,
        document_members,
        mut type_list_filters,
        selected_catalogue,
        inspectables,
        mut zoopedia_pages,
        document_children,
        mut pending_zoopedia_entries,
        mut show_roles,
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
                    overview_exports.write(openzt2_interface_state::information::overview::overview_types::ExportOverviewMap {
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
                UiInformationAction::ApplySetting { .. } => {}
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
                            selected_world_entity: selection.selected_entity.subject(),
                            selected_catalogue_entry: &selected_catalogue,
                            world_definitions: active_world_definitions
                                .get(&world_definition_assets),
                            inspectable_world_entities: &inspectables,
                            document_children: &document_children,
                            zoopedia_pages: &mut zoopedia_pages,
                            pending: &mut pending_zoopedia_entries,
                            show_roles: &mut show_roles,
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
                        &document_members,
                        &mut type_list_filters,
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
