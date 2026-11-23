use bevy::{
    ecs::{entity::EntityHashSet, relationship::RelationshipTarget, system::SystemParam},
    prelude::*,
};
use openzt2_game_data::{
    AssetId,
    species::ConservationStatus,
    ui_document::{node_property_binding::*, widget::UiWidgetRecord},
};

use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    species::species_asset_types::{SpeciesAsset, SpeciesAssets},
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_ui::authored_multi_icon_presentation::UiMultiIconPolicy;
use openzt2_ui::authored_ui_interaction_enabled_binding::UiEnabledBinding;
use openzt2_ui::authored_ui_text_content_binding::UiTextBinding;
use openzt2_world_state::world_spawn::world_entity_crating::{AnimalCrate, WorldEntityIsCrated};
use openzt2_zoo_state::animal_health::types::Dead;
use openzt2_zoo_state::animal_health::types::Rampaging;
use openzt2_zoo_state::animal_lifecycle::types::Adoptable;
use openzt2_zoo_state::animal_lifecycle::types::Animal;
use openzt2_zoo_state::animal_lifecycle::types::CratingRestricted;
use openzt2_zoo_state::animal_lifecycle::types::ReleaseRestricted;
use openzt2_zoo_state::animal_lifecycle::types::SpeciesHandle;
use openzt2_zoo_state::placement::placement_preview_types::RelocatingPlacedObject;

use super::panel_binding_families::{AnimalHandlingFieldOf, AnimalHandlingFields};
use super::text_replacement_operations::replace_projected_ui_text_if_changed;
use openzt2_interface_state::information::entity_selection_types::{InfoPanel, SelectedSubject};

pub(crate) fn authored_multi_icon_key_for_conservation_status(
    conservation_status: ConservationStatus,
) -> Option<AssetId> {
    match conservation_status {
        ConservationStatus::Unspecified => None,
        ConservationStatus::LowRisk => Some(AssetId::from_key("LowRisk")),
        ConservationStatus::Vulnerable => Some(AssetId::from_key("Vulnerable")),
        ConservationStatus::Endangered => Some(AssetId::from_key("Endangered")),
        ConservationStatus::Critical => Some(AssetId::from_key("Critical")),
        ConservationStatus::Extinct => Some(AssetId::from_key("Extinct")),
    }
}

#[derive(SystemParam)]
pub(crate) struct AnimalHandlingChanges<'w, 's> {
    changed: Query<
        'w,
        's,
        (),
        Or<(
            Changed<SpeciesHandle>,
            Added<Animal>,
            Added<Adoptable>,
            Added<ReleaseRestricted>,
            Added<CratingRestricted>,
            Added<WorldEntityIsCrated>,
            Added<Rampaging>,
            Added<Dead>,
            Changed<AnimalCrate>,
            Added<RelocatingPlacedObject>,
        )>,
    >,
    animals: RemovedComponents<'w, 's, Animal>,
    species: RemovedComponents<'w, 's, SpeciesHandle>,
    adoptable: RemovedComponents<'w, 's, Adoptable>,
    release: RemovedComponents<'w, 's, ReleaseRestricted>,
    crating: RemovedComponents<'w, 's, CratingRestricted>,
    crated: RemovedComponents<'w, 's, WorldEntityIsCrated>,
    rampaging: RemovedComponents<'w, 's, Rampaging>,
    dead: RemovedComponents<'w, 's, Dead>,
    crates: RemovedComponents<'w, 's, AnimalCrate>,
    relocating: RemovedComponents<'w, 's, RelocatingPlacedObject>,
}

pub(crate) fn project_selected_animal_pickup_release_and_conservation_controls(
    species_assets: Res<Assets<SpeciesAsset>>,
    active_species: Res<SpeciesAssets>,
    ui_documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    panels: Query<
        (
            &ChildOf,
            Ref<InheritedVisibility>,
            Ref<UiDocumentRoot>,
            Ref<AnimalHandlingFields>,
        ),
        With<InfoPanel>,
    >,
    selections: Query<Ref<SelectedSubject>>,
    mut removed_selections: RemovedComponents<SelectedSubject>,
    relocating: Query<(), With<RelocatingPlacedObject>>,
    animals: Query<
        (
            &SpeciesHandle,
            Has<Adoptable>,
            Has<ReleaseRestricted>,
            Has<CratingRestricted>,
            Has<WorldEntityIsCrated>,
            Has<Rampaging>,
            Has<Dead>,
            Option<&AnimalCrate>,
        ),
        With<Animal>,
    >,
    mut changes: AnimalHandlingChanges,
    mut visibility_nodes: Query<(&UiVisibleBinding, &mut Visibility), With<AnimalHandlingFieldOf>>,
    mut interaction_nodes: Query<
        (
            &UiEnabledBinding,
            &mut UiInteractionEnabled,
            Option<&mut UiFocusable>,
            &mut Pickable,
        ),
        With<AnimalHandlingFieldOf>,
    >,
    mut conservation_icons: Query<
        (&UiNodeId, &UiValueBinding, &mut UiValue),
        (With<AnimalHandlingFieldOf>, With<UiMultiIconPolicy>),
    >,
    mut conservation_texts: Query<(&UiTextBinding, &mut Text), With<AnimalHandlingFieldOf>>,
) {
    let data_changed = species_assets.is_changed()
        || active_species.is_changed()
        || ui_documents.is_changed()
        || active_localization.is_changed()
        || localization_assets.is_changed();
    let Some(species) = active_species.get(&species_assets) else {
        return;
    };
    let removed = changes
        .animals
        .read()
        .chain(changes.species.read())
        .chain(changes.adoptable.read())
        .chain(changes.release.read())
        .chain(changes.crating.read())
        .chain(changes.crated.read())
        .chain(changes.rampaging.read())
        .chain(changes.dead.read())
        .chain(changes.crates.read())
        .chain(changes.relocating.read())
        .chain(removed_selections.read())
        .collect::<EntityHashSet>();
    let localization = active_localization.borrow_loaded_localization_view(&localization_assets);
    for (parent, visible, document, fields) in &panels {
        if !visible.get() {
            continue;
        }
        let selection = selections.get(parent.parent()).ok();
        let facts = selection
            .as_ref()
            .and_then(|selection| animals.get(selection.0).ok());
        let crate_entity = facts.and_then(|facts| facts.7.map(|link| link.0));
        let selection_changed = selection.as_ref().is_some_and(|selection| {
            selection.is_changed()
                || changes.changed.contains(selection.0)
                || removed.contains(&selection.0)
        }) || removed.contains(&parent.parent());
        let crate_changed = crate_entity
            .is_some_and(|entity| changes.changed.contains(entity) || removed.contains(&entity));
        if !data_changed
            && !visible.is_changed()
            && !document.is_changed()
            && !fields.is_changed()
            && !selection_changed
            && !crate_changed
        {
            continue;
        }
        let status = facts
            .and_then(|facts| species.find(facts.0.species))
            .map_or(ConservationStatus::Unspecified, |species| {
                species.conservation
            });
        let status_key = authored_multi_icon_key_for_conservation_status(status);
        let source = ui_documents.get(&document.document);
        let mut label_key = None;
        for entity in fields.iter() {
            let Ok((node, binding, mut value)) = conservation_icons.get_mut(entity) else {
                continue;
            };
            if !matches!(
                binding.0,
                UiIntegerPropertyBindingSource::AnimalConservationStatus
            ) {
                continue;
            }
            let icon = source.and_then(|source| {
                let widget = &source
                    .canonical_ui_document()
                    .nodes
                    .get(node.index as usize)?
                    .widget;
                let UiWidgetRecord::MultiIcon { entries } = widget else {
                    return None;
                };
                entries
                    .iter()
                    .enumerate()
                    .find(|(_, entry)| Some(entry.key) == status_key)
            });
            if let Some((index, entry)) = icon {
                value.set_if_neq(UiValue(i64::try_from(index).unwrap_or(i64::MAX)));
                label_key = Some(entry.localization_key);
            } else {
                value.set_if_neq(UiValue(-1));
            }
        }
        let label = localization
            .zip(label_key)
            .and_then(|(localization, key)| localization.find_plain_localized_text(key))
            .unwrap_or_default();
        let pickup_visible = facts.is_some_and(|facts| facts.1 && !facts.6);
        let release_visible = facts.is_some_and(|facts| !facts.2 && !facts.6);
        let pickup_enabled = facts.is_some_and(|facts| pickup_visible && !facts.3 && !facts.5);
        let crated = facts.is_some_and(|facts| facts.4);
        let crate_can_move = crated
            && selection
                .as_ref()
                .is_some_and(|selection| !relocating.contains(crate_entity.unwrap_or(selection.0)));
        for entity in fields.iter() {
            if let Ok((binding, text)) = conservation_texts.get_mut(entity) {
                if matches!(
                    binding.0,
                    UiTextPropertyBindingSource::AnimalConservationStatus
                ) {
                    replace_projected_ui_text_if_changed(text, label);
                }
            }
            if let Ok((binding, mut visibility)) = visibility_nodes.get_mut(entity) {
                let visible = match binding.0 {
                    UiBooleanPropertyBindingSource::AnimalPickupVisible => Some(pickup_visible),
                    UiBooleanPropertyBindingSource::AnimalReleaseVisible => Some(release_visible),
                    UiBooleanPropertyBindingSource::SelectedEntityCrated => Some(crated),
                    _ => None,
                };
                if let Some(visible) = visible {
                    visibility.set_if_neq(if visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    });
                }
            }
            if let Ok((binding, mut interaction, focusable, mut pickable)) =
                interaction_nodes.get_mut(entity)
            {
                let enabled = match binding.0 {
                    UiBooleanPropertyBindingSource::AnimalPickupEnabled => pickup_enabled,
                    UiBooleanPropertyBindingSource::SelectedEntityCrated => crate_can_move,
                    _ => continue,
                };
                interaction.set_if_neq(UiInteractionEnabled(enabled));
                if let Some(mut focusable) = focusable {
                    if focusable.enabled != enabled {
                        focusable.enabled = enabled;
                    }
                }
                pickable.set_if_neq(if enabled {
                    Pickable::default()
                } else {
                    Pickable::IGNORE
                });
            }
        }
    }
}
