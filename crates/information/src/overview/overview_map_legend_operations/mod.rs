use bevy::{
    ecs::relationship::RelationshipTarget,
    picking::{events::PointerPress, pointer::PointerButton},
    prelude::*,
};
use openzt2_game_data::{
    ui_document::{
        overview_map_presentation::UiMapLayerRecord, widget::UiWidgetRecord,
        widget_live_collection::UiWidgetLiveCollectionSource,
    },
    AssetId,
};

use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_interface_state::information::overview::overview_types::{
    OverviewLegendControls, OverviewLegendLayer,
};
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        RequestedUiListRowCount, UiListPolicy, UiListRow,
    },
    authored_ui_node_projection_components::{
        UiDocumentMembers, UiDocumentOwner, UiDocumentRoot, UiNodeId,
    },
    authored_ui_selection_state::UiSelected,
    authored_ui_visual_types::UiVisualLayer,
};

fn authored_overview_map_layers(document: &UiDocumentAsset) -> Option<&[UiMapLayerRecord]> {
    document
        .canonical_ui_document()
        .nodes
        .iter()
        .find_map(|node| {
            if let UiWidgetRecord::WorldMap { layers, .. } = &node.widget {
                Some(layers.as_slice())
            } else {
                None
            }
        })
}

pub(crate) fn size_authored_overview_map_legends(
    mut lists: Query<(
        Ref<UiListPolicy>,
        Ref<UiDocumentOwner>,
        &mut RequestedUiListRowCount,
    )>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
) {
    for (policy, owner, mut requested) in &mut lists {
        if policy.source != UiWidgetLiveCollectionSource::OverviewLayers
            || (requested.0.is_some()
                && !policy.is_changed()
                && !owner.is_changed()
                && !documents.is_changed())
        {
            continue;
        }
        let Some(layers) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .and_then(authored_overview_map_layers)
        else {
            continue;
        };
        let count = u16::try_from(layers.len()).unwrap_or(u16::MAX);
        requested.set_if_neq(RequestedUiListRowCount(Some(count)));
    }
}

pub(crate) fn bind_authored_overview_map_legend_controls(
    rows: Query<(
        Entity,
        Ref<UiListRow>,
        Ref<ChildOf>,
        Option<&OverviewLegendLayer>,
    )>,
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    roots: Query<(&UiDocumentRoot, &UiDocumentMembers)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_locale: Res<LocalizationPrecedenceIndex>,
    locales: Res<Assets<LocalizationAsset>>,
    children: Query<&Children>,
    ids: Query<&UiNodeId>,
    bindings: Query<&OverviewLegendLayer>,
    visibility: Query<&Visibility>,
    selections: Query<(), With<UiSelected>>,
    mut text: Query<&mut Text>,
    mut icons: Query<&mut ImageNode, With<UiVisualLayer>>,
    mut commands: Commands,
) {
    let Some(locale) = active_locale.borrow_loaded_localization_view(&locales) else {
        return;
    };
    let definitions_changed =
        documents.is_changed() || active_locale.is_changed() || locales.is_changed();
    let label_id = AssetId::from_key("ui/role/fragment/node/textname");
    let checkbox_id = AssetId::from_key("ui/role/fragment/node/checkbox");
    for (entity, row, parent, current) in &rows {
        if current.is_some() && !definitions_changed && !row.is_changed() && !parent.is_changed() {
            continue;
        }
        let Ok((policy, owner)) = lists.get(parent.parent()) else {
            continue;
        };
        if policy.source != UiWidgetLiveCollectionSource::OverviewLayers {
            continue;
        }
        let resolved = (|| {
            let (root, members) = roots.get(owner.0).ok()?;
            let document = documents.get(&root.document)?;
            let record = authored_overview_map_layers(document)?.get(usize::from(row.index))?;
            let layer_id = AssetId(record.node.0);
            let layer = members.iter().find(|&node| {
                ids.get(node).is_ok_and(|id| id.id == layer_id) && visibility.contains(node)
            })?;
            Some((document, record, layer))
        })();
        let Some((document, record, layer)) = resolved else {
            if current.is_some() {
                commands.entity(entity).remove::<OverviewLegendLayer>();
                for child in children.iter_descendants::<Children>(entity) {
                    if bindings.contains(child) {
                        commands.entity(child).remove::<OverviewLegendLayer>();
                    }
                }
            }
            continue;
        };
        let binding = OverviewLegendLayer(layer);
        if current != Some(&binding) {
            commands.entity(entity).insert(binding);
        }
        let label = locale
            .find_plain_localized_text(AssetId(record.localization_key.0))
            .unwrap_or("Overview layer");
        let icon = document.cloned_texture_image_handle(AssetId(record.icon.0));
        for node in children.iter_descendants::<Children>(entity) {
            if ids.get(node).is_ok_and(|id| id.id == label_id) {
                if let Ok(mut text) = text.get_mut(node) {
                    if text.0 != label {
                        text.0.clear();
                        text.0.push_str(label);
                    }
                }
            } else if ids.get(node).is_ok_and(|id| id.id == checkbox_id) {
                if bindings.get(node) != Ok(&binding) {
                    commands.entity(node).insert(binding);
                }
                if !selections.contains(node) {
                    commands
                        .entity(node)
                        .insert(UiSelected(visibility.get(layer) != Ok(&Visibility::Hidden)));
                }
                if let Some(icon) = &icon {
                    for descendant in children.iter_descendants::<Children>(node) {
                        if let Ok(mut image) = icons.get_mut(descendant) {
                            if image.image != *icon {
                                image.image = icon.clone();
                            }
                        }
                    }
                }
            }
        }
    }
}

pub(crate) fn toggle_overview_map_layers_from_pressed_legend_rows(
    mut presses: MessageReader<PointerPress>,
    rows: Query<&OverviewLegendLayer>,
    mut layers: Query<&mut Visibility, With<OverviewLegendControls>>,
) {
    for press in presses
        .read()
        .filter(|press| press.pointer.id.is_mouse() && press.button == PointerButton::Primary)
    {
        let Ok(binding) = rows.get(press.entity) else {
            continue;
        };
        if let Ok(mut visibility) = layers.get_mut(binding.0) {
            *visibility = if *visibility == Visibility::Hidden {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
    }
}

pub(crate) fn project_overview_legend_checkbox_selection(
    layers: Query<
        (&Visibility, &OverviewLegendControls),
        Or<(Changed<Visibility>, Changed<OverviewLegendControls>)>,
    >,
    mut selected: Query<&mut UiSelected>,
) {
    for (visibility, controls) in &layers {
        let value = UiSelected(*visibility != Visibility::Hidden);
        for entity in controls.iter() {
            if let Ok(mut selected) = selected.get_mut(entity) {
                selected.set_if_neq(value);
            }
        }
    }
}
