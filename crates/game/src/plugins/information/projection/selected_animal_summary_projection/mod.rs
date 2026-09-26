use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};

use crate::assets::localization::localization_asset_types::LocalizationAsset;
use crate::assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::animal_health::types::Dead;
use crate::plugins::animal_health::types::Disease;
use crate::plugins::animal_health::types::Escaped;
use crate::plugins::animal_health::types::Rampaging;
use crate::plugins::animal_health::types::Tranquilized;
use crate::plugins::animal_health::types::Treatment;
use crate::plugins::animal_lifecycle::types::Animal;
use crate::plugins::ui::authored_ui_focus_state::UiFocusable;
use crate::plugins::ui::authored_ui_interaction_enabled_binding::UiEnabledBinding;
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_node_projection_components::UiVisibleBinding;
use crate::plugins::ui::authored_ui_text_content_binding::UiTextBinding;

use super::super::entity_selection_types::{InfoPanel, Inspectable, SelectedEntity};
use super::text_replacement_operations::replace_projected_ui_text_if_changed;

pub(in crate::plugins::information) fn project_selected_entity_information_section_visibility(
    information_panels: Query<(Entity, &InfoPanel, &InheritedVisibility)>,
    subjects: Query<&Inspectable>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut visibility_nodes: Query<(&UiDocumentOwner, &UiVisibleBinding, &mut Visibility)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (information_panel_entity, information_panel, inherited_visibility) in &information_panels {
        let selected_section = subjects
            .get(information_panel.subject)
            .ok()
            .and_then(|subject| definitions.find_object(subject.definition))
            .map(|definition| definition.information_panel);
        for (owner, binding, mut visibility) in &mut visibility_nodes {
            if owner.0 != information_panel_entity {
                continue;
            }
            let UiBooleanPropertyBindingSource::SelectedInformationPanelSubjectUsesSection {
                section,
            } = &binding.0
            else {
                continue;
            };
            let next = if inherited_visibility.get() && selected_section == Some(*section) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != next {
                *visibility = next;
            }
        }
    }
}

pub(in crate::plugins::information) fn project_selected_animal_identity_health_and_action_summary(
    selected_entity: Res<SelectedEntity>,
    world_definition_assets: Res<Assets<WorldDefinitionAsset>>,
    active_world_definitions: Res<WorldDefinitions>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    animals: Query<
        (
            &Inspectable,
            Option<&Name>,
            Option<&Disease>,
            Option<&Treatment>,
            Has<Tranquilized>,
            Has<Escaped>,
            Has<Rampaging>,
            Has<Dead>,
        ),
        With<Animal>,
    >,
    mut text_nodes: Query<(&UiTextBinding, &mut Text)>,
    mut visibility_nodes: Query<(&UiVisibleBinding, &mut Visibility)>,
    mut interaction_nodes: Query<(
        &UiEnabledBinding,
        &mut UiInteractionEnabled,
        Option<&mut UiFocusable>,
        &mut Pickable,
    )>,
) {
    let active_localization =
        active_localization.borrow_loaded_localization_view(&localization_assets);
    let Some(selected_subject) = selected_entity.0 else {
        return;
    };
    let Ok((inspectable, name, disease, treatment, tranquilized, escaped, rampaging, dead)) =
        animals.get(selected_subject)
    else {
        return;
    };

    let world_definitions = active_world_definitions.get(&world_definition_assets);
    let selected_definition_name = world_definitions.and_then(|world_definitions| {
        let world_definition = world_definitions.find_object(inspectable.definition)?;
        active_localization?.find_plain_localized_text(AssetId(world_definition.name_key.0))
    });
    let localized_definition_name = |definition_id: AssetId| {
        world_definitions.and_then(|world_definitions| {
            let world_definition = world_definitions.find_object(definition_id)?;
            active_localization?.find_plain_localized_text(AssetId(world_definition.name_key.0))
        })
    };

    for (text_binding, mut text) in &mut text_nodes {
        let projected_text = match &text_binding.0 {
            UiTextPropertyBindingSource::SelectedEntityName => {
                name.map(Name::as_str).or(selected_definition_name)
            }
            UiTextPropertyBindingSource::SelectedEntityDefinitionName => selected_definition_name,
            UiTextPropertyBindingSource::AnimalDiseaseName => {
                disease.and_then(|disease| localized_definition_name(disease.definition))
            }
            UiTextPropertyBindingSource::AnimalTreatmentName => {
                treatment.and_then(|treatment| localized_definition_name(treatment.definition))
            }
            _ => None,
        };
        if let Some(projected_text) = projected_text {
            replace_projected_ui_text_if_changed(&mut text.0, projected_text);
        }
    }

    for (visibility_binding, mut visibility) in &mut visibility_nodes {
        let should_be_visible = match &visibility_binding.0 {
            UiBooleanPropertyBindingSource::AnimalDiseased => disease.is_some(),
            UiBooleanPropertyBindingSource::AnimalUnderTreatment => treatment.is_some(),
            UiBooleanPropertyBindingSource::AnimalTranquilized => tranquilized,
            UiBooleanPropertyBindingSource::AnimalEscaped => escaped,
            UiBooleanPropertyBindingSource::AnimalRampaging => rampaging,
            UiBooleanPropertyBindingSource::AnimalDead => dead,
            _ => continue,
        };
        *visibility = if should_be_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for (enabled_binding, mut interaction_enabled, focusable, mut pickable) in
        &mut interaction_nodes
    {
        let should_be_enabled = match &enabled_binding.0 {
            UiBooleanPropertyBindingSource::AnimalDiseased => {
                disease.is_some() && !rampaging && !dead
            }
            UiBooleanPropertyBindingSource::AnimalTranquilized => !tranquilized && !dead,
            UiBooleanPropertyBindingSource::AnimalDead => !dead,
            _ => continue,
        };
        interaction_enabled.0 = should_be_enabled;
        if let Some(mut focusable) = focusable {
            focusable.enabled = should_be_enabled;
        }
        *pickable = if should_be_enabled {
            Pickable::default()
        } else {
            Pickable::IGNORE
        };
    }
}
