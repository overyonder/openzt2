use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::information::catalogue_types::SelectedCatalogueEntry;
use openzt2_interface_state::ui::authored_ui_action_projection_components::UiResearchActions;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMaximum;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMinimum;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValueBinding;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiVisibleBinding;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_world_state::world_spawn::world_load_completion_marker::WorldLoadCompleted;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

use super::adoption_and_content_availability_types::ScenarioContentAvailability;
use super::catalogue_entry_availability::{
    catalogue_entry_can_be_researched, catalogue_entry_is_available,
};
use super::research_types::{
    ResearchItemPayments, ResearchProject, ResearchProjectAvailability, WorldResearchItems,
};
use super::unlock_types::UnlockedCatalogueDefinitionSet;

pub(super) fn project_selected_catalogue_research_panels(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    selected: Single<&SelectedCatalogueEntry>,
    research: Query<(
        &ResearchProjectAvailability,
        Has<ResearchItemPayments>,
        Has<ResearchProject>,
    )>,
    unlocks: Single<&UnlockedCatalogueDefinitionSet>,
    active_world: Query<Entity, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    roots: Query<
        (
            Entity,
            &SelectedWorldIdentity,
            Option<&ScenarioContentAvailability>,
            Option<&WorldResearchItems>,
        ),
        With<WorldRoot>,
    >,
    mut buttons: Query<(&mut UiSelected, &mut UiInteractionEnabled), With<UiResearchActions>>,
    mut panels: Query<(&UiVisibleBinding, &mut Visibility)>,
) {
    let selected = selected.into_inner();
    let unlocks = unlocks.into_inner();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let world = active_world
        .single()
        .ok()
        .and_then(|world| roots.get(world).ok());
    let entry = selected
        .0
        .and_then(|selected| definitions.catalogue_entry_for_definition(selected));
    let available = entry.is_some_and(|(index, entry)| {
        catalogue_entry_is_available(
            definitions,
            index,
            entry,
            world.map(|(_, identity, _, _)| identity.mode),
            world.and_then(|(_, _, scenario, _)| scenario),
            &unlocks,
        )
    });
    let items = world.and_then(|(_, _, _, items)| items);
    let research_required = !available
        && entry.is_some_and(|(_, entry)| {
            catalogue_entry_can_be_researched(
                definitions,
                entry,
                items
                    .into_iter()
                    .flat_map(|items| items.unlocking(entry.definition))
                    .filter_map(|entity| research.get(entity).ok())
                    .map(|(item, _, _)| (item.item, item.available)),
            )
        });
    let started = selected
        .0
        .and_then(|id| definitions.research_for_definition(id))
        .and_then(|id| items?.definitions.get(&id))
        .and_then(|&entity| research.get(entity).ok())
        .is_some_and(|(_, pending, active)| active || pending);
    for (mut selected, mut enabled) in &mut buttons {
        // Rejected payments and catalogue changes must release the sticky toggle.
        selected.set_if_neq(UiSelected(started));
        enabled.set_if_neq(UiInteractionEnabled(research_required && !started));
    }
    for (binding, mut visibility) in &mut panels {
        let visible = match binding.0 {
            UiBooleanPropertyBindingSource::CatalogueSelectedEntryRequiresResearch => {
                research_required
            }
            UiBooleanPropertyBindingSource::CatalogueSelectedEntryAvailable => available,
            _ => continue,
        };
        visibility.set_if_neq(if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

#[derive(Component, Default, PartialEq, Eq)]
pub struct ResearchPresentationProject(pub Option<Entity>);

pub(super) fn bind_selected_research_presentation_project(
    selected: Query<Ref<SelectedCatalogueEntry>>,
    mut worlds: Query<
        (
            Option<Ref<WorldResearchItems>>,
            Option<Ref<super::research_types::WorldResearchItemMembers>>,
            &mut ResearchPresentationProject,
        ),
        (With<WorldRoot>, With<WorldLoadCompleted>),
    >,
    projects: Query<(), With<ResearchProject>>,
    started: Query<(), Added<ResearchProject>>,
    mut retired: RemovedComponents<ResearchProject>,
) {
    use bevy::ecs::relationship::RelationshipTarget;

    let Ok((definitions, members, mut current)) = worlds.single_mut() else {
        retired.clear();
        return;
    };
    let selected = selected.single().ok();
    let pending = !started.is_empty() || !retired.is_empty();
    retired.clear();
    if !pending
        && !current.is_added()
        && !selected.as_ref().is_some_and(Ref::is_changed)
        && !definitions.as_ref().is_some_and(Ref::is_changed)
        && !members.as_ref().is_some_and(Ref::is_changed)
    {
        return;
    }
    let resolved = selected
        .and_then(|selected| selected.0)
        .and_then(|selected| {
            let definitions = definitions.as_deref()?;
            definitions
                .definitions
                .get(&selected)
                .copied()
                .filter(|&entity| projects.contains(entity))
                .or_else(|| {
                    definitions
                        .unlocking(selected)
                        .find(|&entity| projects.contains(entity))
                })
        })
        .or_else(|| {
            members
                .as_deref()
                .into_iter()
                .flat_map(RelationshipTarget::iter)
                .find(|&entity| projects.contains(entity))
        });
    current.set_if_neq(ResearchPresentationProject(resolved));
}

pub(super) fn project_selected_catalogue_research_progress(
    selection: Query<Ref<ResearchPresentationProject>, (With<WorldRoot>, With<WorldLoadCompleted>)>,
    projects: Query<Ref<super::ResearchCompletionBasisPoints>>,
    mut retired: RemovedComponents<super::ResearchCompletionBasisPoints>,
    mut sliders: Query<(
        Ref<UiValueBinding>,
        &mut UiValue,
        &mut UiMinimum,
        &mut UiMaximum,
        Option<&mut UiInteractionEnabled>,
    )>,
) {
    let selection = selection.single().ok();
    let project_entity = selection.as_ref().and_then(|selection| selection.0);
    let project = project_entity.and_then(|entity| projects.get(entity).ok());
    let refresh = selection.as_ref().is_some_and(Ref::is_changed)
        || project.as_ref().is_some_and(Ref::is_changed)
        || retired.read().any(|entity| Some(entity) == project_entity);
    retired.clear();
    for (binding, mut value, mut minimum, mut maximum, enabled) in &mut sliders {
        if (!refresh && !binding.is_changed() && !value.is_added())
            || !matches!(
                binding.0,
                UiIntegerPropertyBindingSource::SelectedCatalogueResearchProgressBasisPoints
            )
        {
            continue;
        }
        minimum.set_if_neq(UiMinimum(0));
        maximum.set_if_neq(UiMaximum(10_000));
        value.set_if_neq(UiValue(project.as_ref().map_or(0, |project| project.0)));
        if let Some(mut enabled) = enabled {
            enabled.set_if_neq(UiInteractionEnabled(false));
        }
    }
}
