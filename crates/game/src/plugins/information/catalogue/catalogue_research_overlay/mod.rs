use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::plugins::information::catalogue_types::CatalogueRowEntry;
use crate::plugins::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use crate::plugins::progression::catalogue_entry_availability::{
    catalogue_entry_can_be_researched, catalogue_entry_is_available,
};
use crate::plugins::progression::research_types::{
    PendingResearchProjectPayment, ResearchProject, ResearchProjectAvailability,
};
use crate::plugins::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use crate::plugins::ui::authored_ui_integer_range_components::{UiMaximum, UiMinimum};
use crate::plugins::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use crate::plugins::ui::authored_ui_node_projection_components::{
    UiDocumentRoot, UiNodeId, UiValue,
};
use crate::plugins::ui::projection::authored_document_projection::project_document;
use crate::plugins::ui::ui_document_lifecycle_contracts::ShowUiDocument;
use crate::plugins::world_spawn::selected_world_identity::SelectedWorldIdentity;
use crate::plugins::world_spawn::world_membership_types::WorldRoot;

// The fragment belongs to the reusable row, never to the current selection.
#[derive(Component)]
pub(in crate::plugins::information) struct CatalogueResearchOverlay;

pub(in crate::plugins::information) fn attach_authored_catalogue_research_overlays(
    mut commands: Commands,
    rows: Query<Entity, With<CatalogueRowEntry>>,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    overlays: Query<(), With<CatalogueResearchOverlay>>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    localization_index: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    let Some(localization) = localization_index.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for row in &rows {
        if children
            .get(row)
            .is_ok_and(|children| children.iter().any(|child| overlays.contains(child)))
        {
            continue;
        }
        let Some(handle) = std::iter::successors(Some(row), |entity| {
            parents.get(*entity).ok().map(ChildOf::parent)
        })
        .filter_map(|entity| roots.get(entity).ok())
        .filter_map(|root| documents.get(&root.document))
        .find_map(|document| {
            document.nested_ui_document_handle(AssetId::from_virtual_path(
                "ui/fragment/ui/layout/researchbutton.xml",
            ))
        }) else {
            continue;
        };
        let Some(fragment) = documents.get(handle) else {
            continue;
        };
        let overlay = project_document(
            &mut commands,
            &ShowUiDocument {
                document: handle.clone(),
                owner: row,
            },
            fragment,
            localization,
            None,
            false,
        );
        commands.entity(overlay).insert(CatalogueResearchOverlay);
        commands.queue(move |world: &mut World| {
            let mut descendants = vec![overlay];
            while let Some(entity) = descendants.pop() {
                if let Some(children) = world.get::<Children>(entity) {
                    descendants.extend(children.iter());
                }
                world.entity_mut(entity).insert(Pickable::IGNORE);
                world
                    .entity_mut(entity)
                    .remove::<crate::plugins::ui::authored_ui_focus_state::UiFocusable>();
                if let Some(mut picking) =
                    world.get_mut::<crate::plugins::ui::authored_ui_visual_types::UiVisualPicking>(
                        entity,
                    )
                {
                    picking.0 = false;
                }
            }
        });
    }
}

pub(in crate::plugins::information) fn project_catalogue_research_overlays(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<(&SelectedWorldIdentity, Option<&ScenarioContentAvailability>), With<WorldRoot>>,
    availability: Query<&ResearchProjectAvailability>,
    projects: Query<&ResearchProject>,
    payments: Query<&PendingResearchProjectPayment>,
    overlays: Query<(Entity, &ChildOf), With<CatalogueResearchOverlay>>,
    children: Query<&Children>,
    rows: Query<&CatalogueRowEntry>,
    mut nodes: Query<(
        &UiNodeId,
        &Name,
        &mut Node,
        &mut Visibility,
        &mut Pickable,
        Option<&mut UiInteractionEnabled>,
        Option<&mut UiValue>,
        Option<&mut UiMinimum>,
        Option<&mut UiMaximum>,
    )>,
) {
    if overlays.is_empty() {
        return;
    }
    let definitions = active_definitions.get(&definitions);
    let world = worlds.iter().next();
    // One catalogue pass per run; each row takes its definition's first entry.
    let mut catalogue_by_definition = std::collections::HashMap::new();
    for (index, entry) in definitions
        .into_iter()
        .flat_map(|definitions| definitions.catalogue().enumerate())
    {
        catalogue_by_definition
            .entry(entry.definition)
            .or_insert((index, entry));
    }
    // The first research, in catalogue order, that is or unlocks each definition.
    let mut research_by_definition = std::collections::HashMap::new();
    for research in definitions
        .into_iter()
        .flat_map(|definitions| definitions.research())
    {
        for definition in std::iter::once(&research.id).chain(&research.unlocks) {
            research_by_definition
                .entry(*definition)
                .or_insert(research);
        }
    }
    for (overlay, parent) in &overlays {
        let entry = rows
            .get(parent.parent())
            .ok()
            .and_then(|row| catalogue_by_definition.get(&row.definition).copied());
        let research =
            entry.and_then(|(_, entry)| research_by_definition.get(&entry.definition).copied());
        let project = research.and_then(|research| {
            projects
                .iter()
                .find(|project| project.definition == research.id)
        });
        let pending = research.is_some_and(|research| {
            payments
                .iter()
                .any(|payment| payment.definition == research.id)
        });
        let requires_research = entry.is_some_and(|(index, entry)| {
            let definitions = definitions.expect("catalogue entry belongs to loaded definitions");
            !catalogue_entry_is_available(
                definitions,
                index,
                entry,
                world.map(|(identity, _)| identity.mode),
                world.and_then(|(_, scenario)| scenario),
                &unlocks,
            ) && (project.is_some()
                || pending
                || catalogue_entry_can_be_researched(
                    definitions,
                    entry,
                    availability.iter().map(|item| (item.item, item.available)),
                ))
        });
        for entity in std::iter::once(overlay).chain(children.iter_descendants(overlay)) {
            let Ok((
                id,
                name,
                mut node,
                mut visibility,
                mut pickable,
                enabled,
                value,
                minimum,
                maximum,
            )) = nodes.get_mut(entity)
            else {
                continue;
            };
            *pickable = Pickable::IGNORE;
            if let Some(mut enabled) = enabled {
                enabled.0 = name.as_str() == "beaker";
            }
            let visible = if id.index == 0 {
                node.position_type = PositionType::Absolute;
                node.left = Val::Px(0.0);
                node.top = Val::Px(0.0);
                requires_research
            } else if name.as_str() == "fullcolor" {
                // The purchase row already owns the catalogue texture and its states.
                false
            } else if name.as_str() == "progress" {
                if let Some(mut value) = value {
                    value.0 = project.map_or(0, |project| {
                        ((u128::from(project.elapsed_ticks.min(project.required_ticks)) * 10_000)
                            / u128::from(project.required_ticks.max(1)))
                            as i64
                    });
                }
                if let Some(mut minimum) = minimum {
                    minimum.0 = 0;
                }
                if let Some(mut maximum) = maximum {
                    maximum.0 = 10_000;
                }
                requires_research && (pending || project.is_some())
            } else {
                continue;
            };
            visibility.set_if_neq(if visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
}
