use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    ui_document::node_property_binding::{
        UiBooleanPropertyBindingSource, UiTextPropertyBindingSource,
    },
    AssetId,
};
use openzt2_interface_state::{
    information::entity_selection_types::{
        InfoPanel, Inspectable, SelectedSubject, SelectionChanged, WorldUiSelection,
    },
    ui::{
        authored_ui_interaction_enabled_state::UiInteractionEnabled,
        authored_ui_node_projection_components::{UiDocumentMembers, UiVisibleBinding},
    },
};
use openzt2_ui::{
    authored_ui_interaction_enabled_binding::UiEnabledBinding,
    authored_ui_text_content_binding::UiTextBinding,
};
use openzt2_world_state::world_spawn::world_entity_crating::{
    AnimalCrate, CratedWorldEntity, ScriptedCrating,
};
use openzt2_zoo_state::{
    animal_health::types::{Dead, Disease, Escaped, Rampaging, Tranquilized, Treatment},
    animal_lifecycle::types::Animal,
};

use super::{
    panel_binding_families::{AnimalSummaryFieldOf, AnimalSummaryFields},
    text_replacement_operations::replace_projected_ui_text_if_changed,
};

pub(crate) fn project_selected_entity_information_section_visibility(
    panels: Query<(&ChildOf, Ref<InheritedVisibility>, Ref<UiDocumentMembers>), With<InfoPanel>>,
    selections: Query<Ref<SelectedSubject>>,
    subjects: Query<(Ref<Inspectable>, Has<Animal>)>,
    crated: Query<(), CratedWorldEntity>,
    added_crating: Query<(), Or<(Added<AnimalCrate>, Added<ScriptedCrating>)>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut removed: (
        RemovedComponents<AnimalCrate>,
        RemovedComponents<ScriptedCrating>,
        RemovedComponents<Inspectable>,
        RemovedComponents<SelectedSubject>,
    ),
    mut visibility_nodes: Query<(&UiVisibleBinding, &mut Visibility)>,
) {
    let refresh = definitions.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let retired: EntityHashSet = removed
        .0
        .read()
        .chain(removed.1.read())
        .chain(removed.2.read())
        .chain(removed.3.read())
        .collect();
    for (parent, visible, members) in &panels {
        let selection = selections.get(parent.parent()).ok();
        let facts = selection
            .as_ref()
            .and_then(|selection| subjects.get(selection.0).ok());
        if !refresh
            && !visible.is_changed()
            && !members.is_changed()
            && !retired.contains(&parent.parent())
            && !selection.as_ref().is_some_and(|selection| {
                selection.is_changed()
                    || added_crating.contains(selection.0)
                    || retired.contains(&selection.0)
            })
            && !facts
                .as_ref()
                .is_some_and(|(subject, _)| subject.is_changed())
        {
            continue;
        }
        let section = selection
            .as_ref()
            .zip(facts)
            .and_then(|(selection, (subject, animal))| {
                if crated.contains(selection.0) {
                    Some(AssetId::from_key(if animal {
                        "Crate Info"
                    } else {
                        "Crated entity Info"
                    }))
                } else {
                    definitions
                        .find_object(subject.definition)
                        .map(|definition| definition.information_panel)
                }
            });
        let mut fields = visibility_nodes.iter_many_mut(members.iter());
        while let Some(field) = fields.fetch_next() {
            let Ok((binding, mut visibility)) = field else {
                continue;
            };
            let UiBooleanPropertyBindingSource::SelectedInformationPanelSubjectUsesSection {
                section: expected,
            } = &binding.0
            else {
                continue;
            };
            visibility.set_if_neq(if visible.get() && section == Some(*expected) {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}

pub(crate) fn project_selected_animal_identity_health_and_action_summary(
    selected_entity: WorldUiSelection,
    local_selections: Query<Ref<SelectedSubject>>,
    panels: Query<(
        Ref<AnimalSummaryFields>,
        Option<&ChildOf>,
        Option<Ref<InheritedVisibility>>,
        Has<InfoPanel>,
    )>,
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
    changed_subjects: Query<
        (),
        (
            With<Animal>,
            Or<(
                Changed<Inspectable>,
                Changed<Name>,
                Changed<Disease>,
                Changed<Treatment>,
                Added<Tranquilized>,
                Added<Escaped>,
                Added<Rampaging>,
                Added<Dead>,
            )>,
        ),
    >,
    mut selection_events: MessageReader<SelectionChanged>,
    mut retired: (
        RemovedComponents<SelectedSubject>,
        RemovedComponents<Inspectable>,
        RemovedComponents<Name>,
        RemovedComponents<Disease>,
        RemovedComponents<Treatment>,
        RemovedComponents<Tranquilized>,
        RemovedComponents<Escaped>,
        RemovedComponents<Rampaging>,
        RemovedComponents<Dead>,
        RemovedComponents<Animal>,
    ),
    mut fields: Query<
        (
            Option<(&UiTextBinding, &mut Text)>,
            Option<(&UiVisibleBinding, &mut Visibility)>,
            Option<(&UiEnabledBinding, &mut UiInteractionEnabled)>,
        ),
        With<AnimalSummaryFieldOf>,
    >,
) {
    let selection_changed = selection_events.read().count() != 0;
    let retired: EntityHashSet = retired
        .0
        .read()
        .chain(retired.1.read())
        .chain(retired.2.read())
        .chain(retired.3.read())
        .chain(retired.4.read())
        .chain(retired.5.read())
        .chain(retired.6.read())
        .chain(retired.7.read())
        .chain(retired.8.read())
        .chain(retired.9.read())
        .collect();
    let refresh = world_definition_assets.is_changed()
        || active_world_definitions.is_changed()
        || active_localization.is_changed()
        || localization_assets.is_changed();
    let definitions = active_world_definitions.get(&world_definition_assets);
    let locale = active_localization.borrow_loaded_localization_view(&localization_assets);
    let localized_definition_name = |id| {
        let definition = definitions?.find_object(id)?;
        locale?.find_plain_localized_text(definition.name_key)
    };
    for (members, parent, visible, panel) in &panels {
        if visible.as_ref().is_some_and(|visible| !visible.get()) {
            continue;
        }
        let local = parent
            .and_then(|parent| local_selections.get(parent.parent()).ok())
            .filter(|_| panel);
        let subject = if panel {
            local.as_ref().map(|selection| selection.0)
        } else {
            selected_entity.subject()
        };
        if !refresh
            && !selection_changed
            && !members.is_changed()
            && !visible.as_ref().is_some_and(Ref::is_changed)
            && !local.as_ref().is_some_and(Ref::is_changed)
            && !parent.is_some_and(|parent| retired.contains(&parent.parent()))
            && !subject.is_some_and(|entity| {
                changed_subjects.contains(entity) || retired.contains(&entity)
            })
        {
            continue;
        }
        let facts = subject.and_then(|subject| animals.get(subject).ok());
        let definition_name =
            facts.and_then(|(subject, ..)| localized_definition_name(subject.definition));
        let mut fields = fields.iter_many_mut(members.iter());
        while let Some(field) = fields.fetch_next() {
            let Ok((text, visible, enabled)) = field else {
                continue;
            };
            if let Some((binding, text)) = text {
                let next = match binding.0 {
                    UiTextPropertyBindingSource::SelectedEntityName if !panel => Some(
                        facts
                            .and_then(|facts| facts.1)
                            .map(Name::as_str)
                            .or(definition_name),
                    ),
                    UiTextPropertyBindingSource::SelectedEntityDefinitionName if !panel => {
                        Some(definition_name)
                    }
                    UiTextPropertyBindingSource::AnimalDiseaseName => Some(
                        facts
                            .and_then(|facts| facts.2)
                            .and_then(|disease| localized_definition_name(disease.definition)),
                    ),
                    UiTextPropertyBindingSource::AnimalTreatmentName => Some(
                        facts
                            .and_then(|facts| facts.3)
                            .and_then(|treatment| localized_definition_name(treatment.definition)),
                    ),
                    _ => None,
                };
                if let Some(next) = next {
                    replace_projected_ui_text_if_changed(text, next.unwrap_or_default());
                }
            }
            if let Some((binding, mut visibility)) = visible.filter(|_| !panel) {
                let show = match binding.0 {
                    UiBooleanPropertyBindingSource::AnimalDiseased => {
                        facts.is_some_and(|facts| facts.2.is_some())
                    }
                    UiBooleanPropertyBindingSource::AnimalUnderTreatment => {
                        facts.is_some_and(|facts| facts.3.is_some())
                    }
                    UiBooleanPropertyBindingSource::AnimalTranquilized => {
                        facts.is_some_and(|facts| facts.4)
                    }
                    UiBooleanPropertyBindingSource::AnimalEscaped => {
                        facts.is_some_and(|facts| facts.5)
                    }
                    UiBooleanPropertyBindingSource::AnimalRampaging => {
                        facts.is_some_and(|facts| facts.6)
                    }
                    UiBooleanPropertyBindingSource::AnimalDead => {
                        facts.is_some_and(|facts| facts.7)
                    }
                    _ => continue,
                };
                visibility.set_if_neq(if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            if let Some((binding, mut enabled)) = enabled {
                let next = match binding.0 {
                    UiBooleanPropertyBindingSource::AnimalDiseased => {
                        facts.is_some_and(|facts| facts.2.is_some() && !facts.6 && !facts.7)
                    }
                    UiBooleanPropertyBindingSource::AnimalTranquilized => {
                        facts.is_some_and(|facts| !facts.4 && !facts.7)
                    }
                    UiBooleanPropertyBindingSource::AnimalDead => {
                        facts.is_some_and(|facts| !facts.7)
                    }
                    _ => continue,
                };
                enabled.set_if_neq(UiInteractionEnabled(next));
            }
        }
    }
}
