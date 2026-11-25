use bevy::prelude::*;
use openzt2_game_data::AssetId;
use openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualPicking;
use openzt2_progression::progression::ResearchCompletionBasisPoints;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_can_be_researched;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_available;
use openzt2_zoo_state::progression::research_types::{ResearchItemPayments, WorldResearchItems};
use openzt2_zoo_state::progression::research_types::ResearchProject;
use openzt2_zoo_state::progression::research_types::ResearchProjectAvailability;
use openzt2_zoo_state::progression::unlock_types::UnlockedCatalogueDefinitionSet;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMaximum;
use openzt2_interface_state::ui::authored_ui_integer_range_components::UiMinimum;
use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiValue;
use openzt2_ui::projection::authored_document_projection::project_document;
use openzt2_interface_state::ui::ui_document_lifecycle_contracts::ShowUiDocument;
use openzt2_world_state::world_spawn::selected_world_identity::SelectedWorldIdentity;
use openzt2_world_state::world_spawn::world_membership_types::WorldRoot;

// The fragment belongs to the reusable row, never to the current selection.
#[derive(Component)]
#[relationship(relationship_target = RowResearchOverlays)]
pub(crate) struct ResearchOverlayOf(Entity);

#[derive(Component, Default)]
#[relationship_target(relationship = ResearchOverlayOf)]
pub(crate) struct RowResearchOverlays(Vec<Entity>);

#[derive(Component)]
pub(crate) struct HydratedCatalogueResearchOverlay;

#[derive(Component)]
pub(crate) struct ResearchOverlayProgress;

pub(crate) fn attach_authored_catalogue_research_overlays(
    mut commands: Commands,
    rows: Query<Entity, (With<CatalogueRowEntry>, Without<RowResearchOverlays>)>,
    parents: Query<&ChildOf>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    localization_index: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
) {
    if rows.is_empty() {
        return;
    }
    let Some(localization) = localization_index.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    for row in &rows {
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
        commands.entity(overlay).insert(ResearchOverlayOf(row));
    }
}

pub(crate) fn hydrate_authored_catalogue_research_overlay_controls(
    overlays: Query<
        Entity,
        (
            With<ResearchOverlayOf>,
            Without<HydratedCatalogueResearchOverlay>,
        ),
    >,
    children: Query<&Children>,
    mut nodes: Query<(
        Has<UiNodeId>,
        Option<&Name>,
        Option<&mut Pickable>,
        Has<UiFocusable>,
        Option<&mut UiVisualPicking>,
        Option<&mut UiInteractionEnabled>,
        Option<&mut Node>,
        Option<&mut Visibility>,
        Option<&mut UiMinimum>,
        Option<&mut UiMaximum>,
    )>,
    mut commands: Commands,
) {
    for overlay in &overlays {
        for entity in std::iter::once(overlay).chain(children.iter_descendants::<Children>(overlay))
        {
            let Ok((
                authored,
                name,
                picking,
                focusable,
                visual_picking,
                enabled,
                layout,
                visibility,
                minimum,
                maximum,
            )) = nodes.get_mut(entity)
            else {
                continue;
            };
            if let Some(mut picking) = picking {
                picking.set_if_neq(Pickable::IGNORE);
            } else {
                commands.entity(entity).insert(Pickable::IGNORE);
            }
            if focusable {
                commands.entity(entity).remove::<UiFocusable>();
            }
            if let Some(mut visual_picking) = visual_picking {
                visual_picking.set_if_neq(UiVisualPicking(false));
            }
            if !authored {
                continue;
            }
            let name = name.map(Name::as_str);
            if let Some(mut enabled) = enabled {
                enabled.set_if_neq(UiInteractionEnabled(name == Some("beaker")));
            }
            if entity == overlay {
                if let Some(mut layout) = layout {
                    layout.position_type = PositionType::Absolute;
                    layout.left = px(0.);
                    layout.top = px(0.);
                }
            } else if name == Some("fullcolor") {
                if let Some(mut visibility) = visibility {
                    visibility.set_if_neq(Visibility::Hidden);
                } else {
                    commands.entity(entity).insert(Visibility::Hidden);
                }
            } else if name == Some("progress") {
                commands.entity(entity).insert(ResearchOverlayProgress);
                if let Some(mut minimum) = minimum {
                    minimum.set_if_neq(UiMinimum(0));
                }
                if let Some(mut maximum) = maximum {
                    maximum.set_if_neq(UiMaximum(10_000));
                }
            }
        }
        commands
            .entity(overlay)
            .insert(HydratedCatalogueResearchOverlay);
    }
}

pub(crate) fn project_catalogue_research_overlays(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Single<Ref<UnlockedCatalogueDefinitionSet>>,
    worlds: Query<
        (
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
            Option<Ref<WorldResearchItems>>,
        ),
        With<WorldRoot>,
    >,
    research_items: Query<(
        Ref<ResearchProjectAvailability>,
        Option<Ref<ResearchItemPayments>>,
        Option<Ref<ResearchCompletionBasisPoints>>,
    )>,
    mut removals: (
        RemovedComponents<ResearchProject>,
        RemovedComponents<ScenarioContentAvailability>,
        RemovedComponents<SelectedWorldIdentity>,
        RemovedComponents<WorldResearchItems>,
        RemovedComponents<ResearchItemPayments>,
    ),
    mut removed_bindings: RemovedComponents<CatalogueRowEntry>,
    overlays: Query<(
        Entity,
        &ResearchOverlayOf,
        Ref<HydratedCatalogueResearchOverlay>,
    )>,
    rows: Query<Ref<CatalogueRowEntry>>,
    children: Query<&Children>,
    progress_bars: Query<(), With<ResearchOverlayProgress>>,
    mut nodes: Query<(&mut Visibility, Option<&mut UiValue>)>,
) {
    let unlocks = unlocks.into_inner();
    if overlays.is_empty() {
        return;
    }
    let reindex = active_definitions.is_changed() || definitions.is_changed();
    let definitions = active_definitions.get(&definitions);
    let world = worlds.iter().next();
    let inputs_changed = reindex
        || unlocks.is_changed()
        || world.as_ref().is_some_and(|(identity, scenario, items)| {
            identity.is_changed()
                || scenario.as_ref().is_some_and(Ref::is_changed)
                || items.as_ref().is_some_and(Ref::is_changed)
        })
        || !removals.1.is_empty()
        || !removals.2.is_empty()
        || !removals.3.is_empty();
    let finished_projects = removals.0.read().collect::<std::collections::BTreeSet<_>>();
    let settled_payments = removals.4.read().collect::<std::collections::BTreeSet<_>>();
    removals.1.clear();
    removals.2.clear();
    removals.3.clear();
    let removed_rows = removed_bindings
        .read()
        .collect::<std::collections::BTreeSet<_>>();
    let items = world.as_ref().and_then(|(_, _, items)| items.as_deref());
    for (overlay, owner, hydrated) in &overlays {
        let row = rows.get(owner.0).ok();
        let entry = row
            .as_ref()
            .and_then(|row| definitions?.catalogue_entry_for_definition(row.definition));
        let research = entry.and_then(|(_, entry)| {
            let id = definitions?.research_for_definition(entry.definition)?;
            items?.definitions.get(&id).copied()
        });
        let selected_item = research.and_then(|entity| research_items.get(entity).ok());
        let other_items_changed = entry.is_some_and(|(_, entry)| {
            items.is_some_and(|items| {
                items.unlocking(entry.definition).any(|entity| {
                    research_items
                        .get(entity)
                        .is_ok_and(|(item, _, _)| item.is_changed())
                })
            })
        });
        if !inputs_changed
            && !hydrated.is_added()
            && !removed_rows.contains(&owner.0)
            && !row.as_ref().is_some_and(Ref::is_changed)
            && !selected_item
                .as_ref()
                .is_some_and(|(item, purchase, project)| {
                    item.is_changed()
                        || purchase.as_ref().is_some_and(Ref::is_changed)
                        || project.as_ref().is_some_and(Ref::is_changed)
                })
            && !other_items_changed
            && !research.is_some_and(|entity| finished_projects.contains(&entity))
            && !research.is_some_and(|entity| settled_payments.contains(&entity))
        {
            continue;
        }
        let pending = selected_item
            .as_ref()
            .is_some_and(|(_, purchase, _)| purchase.is_some());
        let project = selected_item.and_then(|(_, _, project)| project);
        let requires_research = entry.is_some_and(|(order, entry)| {
            let definitions = definitions.expect("entry belongs to loaded definitions");
            !catalogue_entry_is_available(
                definitions,
                order,
                entry,
                world.as_ref().map(|(identity, _, _)| identity.mode),
                world
                    .as_ref()
                    .and_then(|(_, scenario, _)| scenario.as_deref()),
                &unlocks,
            ) && (project.is_some()
                || pending
                || catalogue_entry_can_be_researched(
                    definitions,
                    entry,
                    items
                        .into_iter()
                        .flat_map(|items| items.unlocking(entry.definition))
                        .filter_map(|entity| research_items.get(entity).ok())
                        .map(|(item, _, _)| (item.item, item.available)),
                ))
        });
        if let Ok((mut visibility, _)) = nodes.get_mut(overlay) {
            visibility.set_if_neq(if requires_research {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        for progress in children
            .iter_descendants::<Children>(overlay)
            .filter(|&entity| progress_bars.contains(entity))
        {
            if let Ok((mut visibility, value)) = nodes.get_mut(progress) {
                visibility.set_if_neq(if requires_research && (pending || project.is_some()) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if let Some(mut value) = value {
                    value.set_if_neq(UiValue(project.as_ref().map_or(0, |project| project.0)));
                }
            }
        }
    }
}
