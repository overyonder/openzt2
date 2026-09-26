use bevy::{ecs::entity::EntityHashMap, prelude::*};
use openzt2_game_data::AssetId;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::catalogue_entry_availability::{catalogue_entry_is_available, catalogue_entry_can_be_researched};
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiListRow;
use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::UiTypeListPolicy;
use crate::plugins::ui::authored_tooltip_presentation::UiDisplayNameKey;
use crate::plugins::ui::authored_tooltip_presentation::UiLongTooltipKey;
use crate::plugins::ui::authored_tooltip_presentation::UiShortTooltipKey;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::ui::authored_ui_visual_types::UiSourceRect;
use crate::plugins::ui::authored_ui_visual_types::UiVisualLayer;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::super::catalogue_types::{CatalogueRowEntry, SelectedCatalogueEntry, TypeListFilter};
use super::catalogue_type_list_policy_operations::catalogue_entry_is_included_by_authored_type_list;
use super::catalogue_world_availability_context::{
    catalogue_world_session_or_scenario_availability_changed,
    resolve_catalogue_world_session_mode_and_scenario_availability,
};

pub(crate) fn project_catalogue_entries_to_authored_type_list_rows(
    mut commands: Commands,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    research_availability: Query<
        Ref<crate::plugins::progression::research_types::ResearchProjectAvailability>,
    >,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    lists: Query<(Ref<UiTypeListPolicy>, Option<Ref<TypeListFilter>>)>,
    mut removed_filters: RemovedComponents<TypeListFilter>,
    selected_entry: Res<SelectedCatalogueEntry>,
    children: Query<&Children>,
    mut visual_images: Query<
        (Entity, Option<&mut ImageNode>, Option<&UiSourceRect>),
        (With<UiVisualLayer>, Without<UiListRow>),
    >,
    mut rows: Query<(
        Entity,
        Ref<UiListRow>,
        Option<&CatalogueRowEntry>,
        Option<&mut ImageNode>,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiSelected>,
        Option<&mut UiPreviousSelection>,
        &mut Visibility,
    )>,
    mut previous_catalogue_revision: Local<u64>,
) {
    let definitions_changed = *previous_catalogue_revision
        != active_definitions.catalogue_revision()
        || research_availability
            .iter()
            .any(|availability| availability.is_changed())
        || unlocks.is_changed()
        || catalogue_world_session_or_scenario_availability_changed(&worlds);
    *previous_catalogue_revision = active_definitions.catalogue_revision();
    let details_changed = selected_entry.is_changed();
    let removed_filters = removed_filters.read().collect::<Vec<_>>();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let catalog = definitions;
    let (mode, scenario) = resolve_catalogue_world_session_mode_and_scenario_availability(&worlds);
    let selected_definition = selected_entry.0;
    let mut catalogue_entries_by_type_list = EntityHashMap::default();
    for (
        entity,
        row,
        bound,
        image,
        interaction_enabled,
        selected,
        selection_state,
        mut visibility,
    ) in &mut rows
    {
        // A row's catalogue binding is immutable until either the reusable row
        // is assigned a new index/list or the loaded definitions asset is
        // replaced. Do not rescan the entire catalogue for every row on every
        // rendered frame.
        let Ok((policy, filter)) = lists.get(row.list) else {
            continue;
        };
        let presentation_changed = policy.is_changed()
            || filter.as_ref().is_some_and(Ref::is_changed)
            || removed_filters.contains(&row.list);
        if bound.is_some()
            && !row.is_changed()
            && !definitions_changed
            && !presentation_changed
            && !details_changed
        {
            continue;
        }
        let filter = filter.as_deref().copied().unwrap_or_default();
        let included_entries = catalogue_entries_by_type_list
            .entry(row.list)
            .or_insert_with(|| {
                catalog
                    .catalogue_in_authored_purchase_order()
                    .filter(|(index, entry)| {
                        catalogue_entry_is_included_by_authored_type_list(
                            &policy, filter, *index, entry, catalog, mode, scenario, &unlocks,
                        )
                    })
                    .collect::<Vec<_>>()
            });
        let Some(&(catalogue_index, entry)) = included_entries.get(usize::from(row.index)) else {
            if *visibility != Visibility::Hidden {
                *visibility = Visibility::Hidden;
            }
            continue;
        };
        if *visibility != Visibility::Inherited {
            *visibility = Visibility::Inherited;
        }
        if let Some(mut interaction_enabled) = interaction_enabled {
            interaction_enabled.0 = catalogue_entry_can_be_researched(
                catalog,
                entry,
                research_availability
                    .iter()
                    .map(|item| (item.item, item.available)),
            ) || catalogue_entry_is_available(
                catalog,
                catalogue_index,
                entry,
                mode,
                scenario,
                &unlocks,
            );
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
            selected.0 = is_selected;
        }
        if let Some(mut selection_state) = selection_state {
            selection_state.synchronize_with_non_authored_selection_change(is_selected);
        }
        if definitions_changed || bound.is_none_or(|bound| bound.definition != definition) {
            let name = AssetId(entry.name_key.0);
            commands.entity(entity).insert((
                CatalogueRowEntry { definition },
                UiDisplayNameKey(name),
                UiShortTooltipKey(name),
            ));
            let description = catalog
                .find_object(definition)
                .map(|object| AssetId(object.description_key.0))
                .filter(|key| *key != AssetId::default());
            if let Some(description) = description {
                commands
                    .entity(entity)
                    .insert(UiLongTooltipKey(description));
            } else {
                commands.entity(entity).remove::<UiLongTooltipKey>();
            }
        }
    }
}
