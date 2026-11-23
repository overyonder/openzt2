use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_available;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_can_be_researched;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use openzt2_ui::authored_tooltip_presentation::UiDisplayNameKey;
use openzt2_ui::authored_tooltip_presentation::UiShortTooltipKey;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_interface_state::ui::authored_ui_visual_types::UiSourceRect;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::catalogue_type_list_row_count_operations::OrderedCatalogueEntries;
use super::catalogue_world_availability_context::{
    catalogue_world_session_or_scenario_availability_changed,
    resolve_catalogue_world_session_mode_and_scenario_availability, CatalogueResearchAccess,
};
use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;

pub(crate) fn project_catalogue_entries_to_authored_type_list_rows(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    research_availability: CatalogueResearchAccess,
    unlocks: Single<Ref<UnlockedCatalogueDefinitionSet>>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    scenario_disabled: Query<Ref<openzt2_interface_state::information::catalogue_types::ScenarioDisabledCatalogueDefinitions>, With<WorldRoot>>,
    lists: Query<Ref<OrderedCatalogueEntries>>,
    selected_entry: Single<Ref<SelectedCatalogueEntry>>,
    children: Query<&Children>,
    mut visual_images: Query<
        (Entity, Option<&mut ImageNode>, Option<&UiSourceRect>),
        (With<UiVisualLayer>, Without<UiListRow>),
    >,
    mut rows: Query<(
        Entity,
        Ref<UiListRow>,
        Ref<ChildOf>,
        Option<&mut CatalogueRowEntry>,
        Option<&mut UiDisplayNameKey>,
        Option<&mut UiShortTooltipKey>,
        Option<&mut ImageNode>,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiSelected>,
        &mut Visibility,
    )>,
) {
    let unlocks = unlocks.into_inner();
    let selected_entry = selected_entry.into_inner();
    let definitions_changed = active_definitions.is_changed() || definitions.is_changed();
    let scenario_disabled = scenario_disabled.single().ok();
    let availability_changed = scenario_disabled
        .as_ref()
        .is_some_and(|disabled| disabled.is_changed())
        || research_availability.changed()
        || unlocks.is_changed()
        || catalogue_world_session_or_scenario_availability_changed(&worlds);
    let details_changed = selected_entry.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = definitions;
    let (mode, scenario) = resolve_catalogue_world_session_mode_and_scenario_availability(&worlds);
    let selected_definition = selected_entry.0;
    for (
        entity,
        row,
        parent,
        bound,
        name_key,
        tooltip_key,
        image,
        interaction_enabled,
        selected,
        mut visibility,
    ) in &mut rows
    {
        let Ok(entries) = lists.get(parent.parent()) else {
            continue;
        };
        if bound.is_some()
            && !row.is_changed()
            && !parent.is_changed()
            && !definitions_changed
            && !availability_changed
            && !entries.is_changed()
        {
            if details_changed {
                if let (Some(bound), Some(mut selected)) = (bound, selected) {
                    selected.set_if_neq(UiSelected(selected_definition == Some(bound.definition)));
                }
            }
            continue;
        }
        let Some((catalogue_index, entry)) = entries
            .0
            .get(usize::from(row.index))
            .and_then(|&id| catalog.catalogue_entry_for_definition(id))
        else {
            if bound.is_some() {
                commands.entity(entity).remove::<CatalogueRowEntry>();
            }
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
        if let Some(mut interaction_enabled) = interaction_enabled {
            let enabled = !scenario_disabled
                .as_ref()
                .is_some_and(|disabled| disabled.0.contains(&entry.definition))
                && (catalogue_entry_can_be_researched(
                    catalog,
                    entry,
                    research_availability.available_for(entry.definition),
                ) || catalogue_entry_is_available(
                    catalog,
                    catalogue_index,
                    entry,
                    mode,
                    scenario,
                    &unlocks,
                ));
            interaction_enabled.set_if_neq(UiInteractionEnabled(enabled));
        }
        let icon = AssetId(entry.icon.0);
        let texture = definitions.texture_image(icon);
        if bound.is_none() {
            debug!(
                row = row.index,
                ?icon,
                resolved = texture.is_some(),
                "binding catalogue row"
            );
        }
        if let Some(handle) = texture {
            if let Some(mut image) = image {
                if image.image != handle {
                    image.image = handle.clone();
                }
            }
            // Purchase-icon fragments express interaction states as ordinary
            // Bevy visual children. Bind the catalogue texture to those
            // children directly; the authored source rectangles then select
            // normal/highlighted/pressed/disabled quadrants without a custom
            // catalogue renderer or copied row model.
            if let Ok(children) = children.get(entity) {
                for child in children.iter() {
                    if let Ok((visual, image, source_rect)) = visual_images.get_mut(child) {
                        if let Some(mut image) = image {
                            if image.image != handle {
                                image.image = handle.clone();
                            }
                        } else {
                            let mut image = ImageNode::new(handle.clone());
                            image.rect = source_rect.map(|rect| Rect {
                                min: Vec2::new(rect.0[0] as f32, rect.0[1] as f32),
                                max: Vec2::new(
                                    (rect.0[0] + rect.0[2]) as f32,
                                    (rect.0[1] + rect.0[3]) as f32,
                                ),
                            });
                            commands.entity(visual).insert(image);
                        }
                    }
                }
            }
        }
        let definition = AssetId(entry.definition.0);
        let is_selected = selected_definition == Some(definition);
        if let Some(mut selected) = selected {
            selected.set_if_neq(UiSelected(is_selected));
        }
        let name = AssetId(entry.name_key.0);
        let binding = CatalogueRowEntry { definition };
        if let Some(mut bound) = bound {
            bound.set_if_neq(binding);
        } else {
            commands.entity(entity).insert(binding);
        }
        if let Some(mut name_key) = name_key {
            name_key.set_if_neq(UiDisplayNameKey(name));
        } else {
            commands.entity(entity).insert(UiDisplayNameKey(name));
        }
        if let Some(mut tooltip_key) = tooltip_key {
            tooltip_key.set_if_neq(UiShortTooltipKey(name));
        } else {
            commands.entity(entity).insert(UiShortTooltipKey(name));
        }
    }
}
