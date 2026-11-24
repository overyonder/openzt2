//! The items a selected guest carries and its favourite animal, one
//! `iconitem.xml` row each in its panel's `Inventory` and `guest favorites`
//! lists.

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
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;
use openzt2_zoo_state::animal_behavior::interaction_container_occupancy::InteractionContainerOccupancy;
use openzt2_zoo_state::guests::guest_simulation_types::GuestFavouriteAnimal;

/// The objects a list shows for the selected entity: the items it carries,
/// in the order they were taken, or its favourite animal, shown as the
/// first adoptable animal of that species.
fn listed_objects(
    source: UiWidgetLiveCollectionSource,
    subject: Entity,
    occupancy: &InteractionContainerOccupancy,
    items: &Query<&Inspectable>,
    favourites: &Query<&GuestFavouriteAnimal>,
    definitions: Option<WorldDefinitionsView<'_>>,
) -> Option<Vec<AssetId>> {
    match source {
        UiWidgetLiveCollectionSource::SelectedEntityInventory => Some(
            occupancy
                .members_of_container(subject)
                .filter_map(|item| items.get(item).ok().map(|item| item.definition))
                .collect(),
        ),
        UiWidgetLiveCollectionSource::SelectedGuestFavorites => Some(
            favourites
                .get(subject)
                .ok()
                .and_then(|favourite| favourite.0)
                .and_then(|species| {
                    definitions?
                        .animal_adoption_species_eligibility()
                        .find(|animal| animal.species == species)
                        .map(|animal| animal.id)
                })
                .into_iter()
                .collect(),
        ),
        _ => None,
    }
}

pub(crate) fn request_selected_entity_inventory_row_counts(
    occupancy: Res<InteractionContainerOccupancy>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    lists: Query<(Entity, &UiListPolicy, &UiDocumentOwner)>,
    panels: Query<&InfoPanel>,
    items: Query<&Inspectable>,
    favourites: Query<&GuestFavouriteAnimal>,
    mut requested: Local<bevy::platform::collections::HashMap<Entity, u16>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
) {
    let definitions = active_world_definitions.get(&world_definition_assets);
    requested.retain(|list, _| lists.contains(*list));
    for (list, policy, owner) in &lists {
        if !matches!(
            policy.source,
            UiWidgetLiveCollectionSource::SelectedEntityInventory
                | UiWidgetLiveCollectionSource::SelectedGuestFavorites
        ) {
            continue;
        }
        let Some(objects) = panels.get(owner.0).map_or(Some(Vec::new()), |panel| {
            listed_objects(
                policy.source,
                panel.subject,
                &occupancy,
                &items,
                &favourites,
                definitions,
            )
        }) else {
            continue;
        };
        let count = objects.len();
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
    favourites: Query<&GuestFavouriteAnimal>,
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
        let Some(object) = panels
            .get(owner.0)
            .ok()
            .and_then(|panel| {
                listed_objects(
                    policy.source,
                    panel.subject,
                    &occupancy,
                    &items,
                    &favourites,
                    Some(definitions),
                )?
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
