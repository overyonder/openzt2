//! The items a selected guest carries, one `iconitem.xml` row each in its
//! panel's `Inventory` list.

use bevy::prelude::*;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::information::entity_selection_types::InfoPanel;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListPolicy, UiListRow,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_interface_state::ui::pointer_capture_types::UiPointerInteraction;
use openzt2_ui::authored_tooltip_presentation::UiShortTooltipKey;
use openzt2_game_data::AssetId;
use openzt2_interface_state::information::entity_selection_types::Inspectable;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy;

/// The selected entity's carried items that have a definition, in the order
/// they were taken.
fn carried_items(
    occupancy: &InteractionContainerOccupancy,
    holder: Entity,
    definitions: &Query<&Inspectable>,
) -> Vec<AssetId> {
    occupancy
        .members_of_container(holder)
        .filter_map(|item| definitions.get(item).ok().map(|item| item.definition))
        .collect()
}

pub(crate) fn request_selected_entity_inventory_row_counts(
    occupancy: Res<InteractionContainerOccupancy>,
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner)>,
    panels: Query<&InfoPanel>,
    items: Query<&Inspectable>,
    mut requested: Local<bevy::platform::collections::HashMap<Entity, u16>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    requested.retain(|list, _| lists.contains(*list));
    for (list, policy, owner) in &lists {
        if policy.source != UiWidgetLiveCollectionSource::SelectedEntityInventory {
            continue;
        }
        let count = panels.get(owner.0).map_or(0, |panel| {
            carried_items(&occupancy, panel.subject, &items).len()
        });
        let count = u16::try_from(count).unwrap_or(u16::MAX);
        if requested.insert(list, count) != Some(count) {
            row_count_requests.write(SetUiListRowCount { list, count });
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn project_selected_entity_inventory_to_list_rows(
    mut commands: Commands,
    occupancy: Res<InteractionContainerOccupancy>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    lists: Query<(&UiListPolicy, &UiDocumentOwner)>,
    panels: Query<&InfoPanel>,
    items: Query<&Inspectable>,
    mut rows: Query<(
        Entity,
        &UiListRow,
        Option<&mut ImageNode>,
        Option<&UiShortTooltipKey>,
    )>,
    children: Query<&Children>,
    mut visual_images: Query<&mut ImageNode, (With<UiVisualLayer>, Without<UiListRow>)>,
) {
    let Some(definitions) = active_world_definitions.get(&world_definition_assets) else {
        return;
    };
    for (row_entity, row, image, tooltip) in &mut rows {
        let Ok((policy, owner)) = lists.get(row.list) else {
            continue;
        };
        if policy.source != UiWidgetLiveCollectionSource::SelectedEntityInventory {
            continue;
        }
        let Some(object) = panels
            .get(owner.0)
            .ok()
            .and_then(|panel| {
                carried_items(&occupancy, panel.subject, &items)
                    .get(usize::from(row.index))
                    .copied()
            })
            .and_then(|item| definitions.find_object(item))
        else {
            continue;
        };
        let icon = definitions.texture_image(object.icon).unwrap_or_default();
        // The row's own image, or the visual layers it draws through.
        if let Some(mut image) = image {
            if image.image != icon {
                image.image = icon.clone();
            }
        }
        for child in children.get(row_entity).into_iter().flat_map(|c| c.iter()) {
            if let Ok(mut image) = visual_images.get_mut(child) {
                if image.image != icon {
                    image.image = icon.clone();
                }
            }
        }
        if tooltip.map(|tooltip| tooltip.0) != Some(object.name_key) {
            commands.entity(row_entity).insert((
                UiShortTooltipKey(object.name_key),
                Pickable::default(),
                UiPointerInteraction::default(),
            ));
        }
    }
}
