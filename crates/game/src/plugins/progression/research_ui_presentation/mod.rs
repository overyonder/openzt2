use bevy::prelude::*;
use openzt2_game_data::ui_document::node_property_binding::{
    UiBooleanPropertyBindingSource, UiIntegerPropertyBindingSource,
};

use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::plugins::information::catalogue_types::SelectedCatalogueEntry;
use crate::plugins::ui::authored_ui_action_projection_components::UiResearchActions;
use crate::plugins::ui::authored_ui_change_activation_dispatch::UiPreviousSelection;
use crate::plugins::ui::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::{
    UiValue, UiValueBinding, UiVisibleBinding,
};
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

use super::adoption_and_content_availability_types::ScenarioContentAvailability;
use super::catalogue_entry_availability::{
    catalogue_entry_can_be_researched, catalogue_entry_is_available,
};
use super::research_types::{PendingResearchProjectPayment, ResearchProject};
use super::unlock_types::UnlockedCatalogueDefinitionSet;

pub(super) fn project_selected_catalogue_research_panels(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    selected: Res<SelectedCatalogueEntry>,
    research_availability: Query<
        &crate::plugins::progression::research_types::ResearchProjectAvailability,
    >,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    roots: Query<(&SelectedWorldIdentity, Option<&ScenarioContentAvailability>), With<WorldRoot>>,
    projects: Query<&ResearchProject>,
    payments: Query<&PendingResearchProjectPayment>,
    mut buttons: Query<
        (
            &mut UiSelected,
            &mut UiPreviousSelection,
            &mut UiInteractionEnabled,
        ),
        With<UiResearchActions>,
    >,
    mut panels: Query<(&UiVisibleBinding, &mut Visibility)>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let world = roots.iter().next();
    let entry = selected.0.and_then(|selected| {
        definitions
            .catalogue()
            .enumerate()
            .find(|(_, entry)| entry.definition == selected)
    });
    let available = entry.is_some_and(|(index, entry)| {
        catalogue_entry_is_available(
            definitions,
            index,
            entry,
            world.map(|(identity, _)| identity.mode),
            world.and_then(|(_, scenario)| scenario),
            &unlocks,
        )
    });
    let research_required = !available
        && entry.is_some_and(|(_, entry)| {
            catalogue_entry_can_be_researched(
                definitions,
                entry,
                research_availability
                    .iter()
                    .map(|item| (item.item, item.available)),
            )
        });
    let research_definition = selected.0.and_then(|selected| {
        definitions
            .research()
            .find(|research| research.id == selected || research.unlocks.contains(&selected))
            .map(|research| research.id)
    });
    let started = research_definition.is_some_and(|definition| {
        projects
            .iter()
            .any(|project| project.definition == definition)
            || payments
                .iter()
                .any(|payment| payment.definition == definition)
    });
    for (mut selected, mut previous, mut enabled) in &mut buttons {
        // Rejected payments and catalogue changes must release the sticky toggle.
        selected.0 = started;
        previous.synchronize_with_non_authored_selection_change(started);
        enabled.0 = research_required && !started;
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

pub(super) fn project_selected_catalogue_research_progress(
    selected: Res<SelectedCatalogueEntry>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    projects: Query<&ResearchProject>,
    mut sliders: Query<(
        &UiValueBinding,
        &mut UiValue,
        &mut UiMinimum,
        &mut UiMaximum,
        Option<&mut UiInteractionEnabled>,
    )>,
) {
    let definitions = active_definitions.get(&definitions);
    let project = projects.iter().find(|project| {
        selected.0.is_some_and(|selected| {
            project.definition == selected
                || definitions
                    .and_then(|definitions| definitions.find_research(project.definition))
                    .is_some_and(|research| research.unlocks.contains(&selected))
        })
    });
    for (binding, mut value, mut minimum, mut maximum, enabled) in &mut sliders {
        if !matches!(
            binding.0,
            UiIntegerPropertyBindingSource::SelectedCatalogueResearchProgressBasisPoints
        ) {
            continue;
        }
        minimum.0 = 0;
        maximum.0 = 10_000;
        value.0 = project.map_or(0, |project| {
            ((u128::from(project.elapsed_ticks.min(project.required_ticks)) * 10_000)
                / u128::from(project.required_ticks.max(1))) as i64
        });
        if let Some(mut enabled) = enabled {
            enabled.0 = false;
        }
    }
}
