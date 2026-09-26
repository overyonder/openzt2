use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    overview_map_presentation::UiMapLayerRecord, widget::UiWidgetRecord,
    widget_live_collection::UiWidgetLiveCollectionSource,
};
use openzt2_game_data::AssetId;

use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
    },
    plugins::ui::{
        authored_reusable_list_and_table_runtime_types::{
            SetUiListRowCount, UiListPolicy, UiListRow, UiTablePolicy,
        },
        authored_ui_change_activation_dispatch::UiPreviousSelection,
        authored_ui_node_projection_components::UiDocumentOwner,
        authored_ui_node_projection_components::UiDocumentRoot,
        authored_ui_node_projection_components::UiNodeId,
        authored_ui_selection_state::UiSelected,
        authored_ui_visual_types::UiVisualLayer,
    },
};

use super::overview_types::{OverviewLegendListState, OverviewLegendRow, OverviewMapCanvas};

fn find_authored_overview_map_layer_records(
    ui_document: &UiDocumentAsset,
) -> Option<&[UiMapLayerRecord]> {
    ui_document
        .canonical_ui_document()
        .nodes
        .iter()
        .find_map(|node| match &node.widget {
            UiWidgetRecord::WorldMap { layers, .. } => Some(layers.as_slice()),
            _ => None,
        })
}

fn entity_is_descendant_of_ancestor(
    descendant_entity: Entity,
    ancestor_entity: Entity,
    parent_relationships: &Query<&ChildOf>,
) -> bool {
    let mut current_ancestor_candidate = descendant_entity;
    while let Ok(parent_relationship) = parent_relationships.get(current_ancestor_candidate) {
        if parent_relationship.parent() == ancestor_entity {
            return true;
        }
        current_ancestor_candidate = parent_relationship.parent();
    }
    false
}

pub(in crate::plugins::information) fn request_authored_overview_map_legend_row_entities(
    overview_legend_lists: Query<(
        Entity,
        &UiListPolicy,
        &UiDocumentOwner,
        Option<&OverviewLegendListState>,
    )>,
    overview_map_surfaces: Query<(&UiNodeId, &UiDocumentOwner, &UiTablePolicy)>,
    ui_document_roots: Query<&UiDocumentRoot>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
    mut commands: Commands,
) {
    for (legend_list, list_policy, document_owner, current_list_state) in &overview_legend_lists {
        if list_policy.source != UiWidgetLiveCollectionSource::OverviewLayers {
            continue;
        }
        let Ok(document_root) = ui_document_roots.get(document_owner.0) else {
            continue;
        };
        let Some(ui_document) = ui_documents.get(&document_root.document) else {
            continue;
        };
        let Some((overview_map_node, _, _)) =
            overview_map_surfaces
                .iter()
                .find(|(_, candidate_owner, candidate_table_policy)| {
                    candidate_owner.0 == document_owner.0
                        && **candidate_table_policy == UiTablePolicy::WorldMap
                })
        else {
            continue;
        };
        let Some(UiWidgetRecord::WorldMap { layers, .. }) = ui_document
            .canonical_ui_document()
            .nodes
            .get(overview_map_node.index as usize)
            .map(|node_record| &node_record.widget)
        else {
            continue;
        };
        let required_row_count = layers.len().min(usize::from(u16::MAX)) as u16;
        if current_list_state.is_none_or(|state| state.rows != required_row_count) {
            row_count_requests.write(SetUiListRowCount {
                list: legend_list,
                count: required_row_count,
            });
            commands
                .entity(legend_list)
                .insert(OverviewLegendListState {
                    rows: required_row_count,
                });
        }
    }
}

pub(in crate::plugins::information) fn bind_authored_overview_map_layer_data_to_legend_rows(
    legend_rows: Query<(Entity, Ref<UiListRow>, Option<&OverviewLegendRow>)>,
    legend_list_owners: Query<&UiDocumentOwner>,
    ui_document_roots: Query<&UiDocumentRoot>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    parent_relationships: Query<&ChildOf>,
    projected_ui_nodes: Query<(Entity, &UiNodeId)>,
    visual_layer_entities: Query<Entity, With<UiVisualLayer>>,
    mut text_nodes: Query<&mut Text>,
    mut image_nodes: Query<&mut ImageNode>,
    mut commands: Commands,
) {
    let Some(active_localization) =
        active_localization.borrow_loaded_localization_view(&localization_assets)
    else {
        return;
    };
    let legend_text_node_id = AssetId::from_key("ui/role/fragment/node/textname");
    let legend_checkbox_node_id = AssetId::from_key("ui/role/fragment/node/checkbox");
    for (row_entity, list_row, current_legend_row) in &legend_rows {
        if !list_row.is_added() && current_legend_row.is_some() {
            continue;
        }
        let Ok(document_owner) = legend_list_owners.get(list_row.list) else {
            continue;
        };
        let Some((ui_document, layer_index, layer_record)) = ui_document_roots
            .get(document_owner.0)
            .ok()
            .and_then(|document_root| ui_documents.get(&document_root.document))
            .and_then(|ui_document| {
                find_authored_overview_map_layer_records(ui_document).and_then(|layer_records| {
                    let layer_index = u32::from(list_row.index);
                    layer_records
                        .get(layer_index as usize)
                        .map(|layer_record| (ui_document, layer_index, layer_record))
                })
            })
        else {
            continue;
        };
        let legend_row_presentation = OverviewLegendRow {
            owner: document_owner.0,
            layer: layer_index,
            visible: current_legend_row.is_none_or(|current_row| current_row.visible),
        };
        if current_legend_row != Some(&legend_row_presentation) {
            commands.entity(row_entity).insert(legend_row_presentation);
        }
        let localized_layer_label = active_localization
            .find_plain_localized_text(AssetId(layer_record.localization_key.0))
            .unwrap_or("Overview layer");
        let layer_icon = ui_document.cloned_texture_image_handle(AssetId(layer_record.icon.0));
        for (projected_node, node_id) in &projected_ui_nodes {
            if !entity_is_descendant_of_ancestor(projected_node, row_entity, &parent_relationships)
            {
                continue;
            }
            if node_id.id == legend_text_node_id {
                if let Ok(mut text) = text_nodes.get_mut(projected_node) {
                    if text.0 != localized_layer_label {
                        text.0.clear();
                        text.0.push_str(localized_layer_label);
                    }
                }
            } else if node_id.id == legend_checkbox_node_id {
                commands.entity(projected_node).insert((
                    legend_row_presentation,
                    UiSelected(legend_row_presentation.visible),
                    UiPreviousSelection::from_current_selection(legend_row_presentation.visible),
                ));
                if let Some(layer_icon) = layer_icon.clone() {
                    for visual_layer_entity in &visual_layer_entities {
                        if entity_is_descendant_of_ancestor(
                            visual_layer_entity,
                            projected_node,
                            &parent_relationships,
                        ) {
                            if let Ok(mut image_node) = image_nodes.get_mut(visual_layer_entity) {
                                image_node.image = layer_icon.clone();
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(in crate::plugins::information) fn toggle_overview_map_layers_from_pressed_legend_rows(
    mut pressed_legend_rows: Query<
        (
            &mut OverviewLegendRow,
            &Interaction,
            Option<&mut UiSelected>,
        ),
        Changed<Interaction>,
    >,
    ui_document_roots: Query<&UiDocumentRoot>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    mut overview_map_canvases: Query<(&OverviewMapCanvas, &mut Visibility)>,
    mut projected_ui_nodes: Query<
        (&UiNodeId, &UiDocumentOwner, &mut Visibility),
        Without<OverviewMapCanvas>,
    >,
) {
    for (mut legend_row, interaction, selected_state) in &mut pressed_legend_rows {
        if *interaction != Interaction::Pressed {
            continue;
        }
        legend_row.visible = !legend_row.visible;
        if let Some(mut selected_state) = selected_state {
            selected_state.0 = legend_row.visible;
        }
        for (overview_map_canvas, mut visibility) in &mut overview_map_canvases {
            if overview_map_canvas.layer == legend_row.layer {
                *visibility = if legend_row.visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
        let Some(authored_layer_node_id) = ui_document_roots
            .get(legend_row.owner)
            .ok()
            .and_then(|document_root| ui_documents.get(&document_root.document))
            .and_then(|ui_document| {
                find_authored_overview_map_layer_records(ui_document)?
                    .get(legend_row.layer as usize)
            })
            .map(|layer_record| AssetId(layer_record.node.0))
        else {
            continue;
        };
        for (node_id, document_owner, mut visibility) in &mut projected_ui_nodes {
            if document_owner.0 == legend_row.owner && node_id.id == authored_layer_node_id {
                *visibility = if legend_row.visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                break;
            }
        }
    }
}
