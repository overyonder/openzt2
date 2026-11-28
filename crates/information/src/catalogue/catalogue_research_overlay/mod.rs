use bevy::prelude::*;
use openzt2_game_data::AssetId;

use openzt2_game_assets::localization::localization_asset_types::LocalizationAsset;
use openzt2_game_assets::localization::localization_precedence_index::LocalizationPrecedenceIndex;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_interface_state::information::catalogue_types::CatalogueRowEntry;
use openzt2_zoo_state::progression::adoption_and_content_availability_types::ScenarioContentAvailability;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_can_be_researched;
use openzt2_zoo_state::progression::catalogue_entry_availability::catalogue_entry_is_available;
use openzt2_zoo_state::progression::research_types::PendingResearchProjectPayment;
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

#[derive(Component, Default)]
pub(crate) struct CatalogueResearchOverlay {
    progress: Vec<Entity>,
    projected_row: Option<Option<AssetId>>,
}

#[expect(clippy::needless_pass_by_value)]
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
        commands.queue(move |world: &mut World| {
            let mut descendants = vec![overlay];
            let mut state = CatalogueResearchOverlay::default();
            while let Some(entity) = descendants.pop() {
                if let Some(children) = world.get::<Children>(entity) {
                    descendants.extend(children.iter());
                }
                world.entity_mut(entity).insert(Pickable::IGNORE);
                world
                    .entity_mut(entity)
                    .remove::<openzt2_interface_state::ui::authored_ui_focus_state::UiFocusable>();
                if let Some(mut picking) =
                    world
                        .get_mut::<openzt2_interface_state::ui::authored_ui_visual_types::UiVisualPicking>(
                            entity,
                        )
                {
                    picking.0 = false;
                }
                if world.get::<UiNodeId>(entity).is_none() {
                    continue;
                }
                let name = world.get::<Name>(entity).map(Name::as_str);
                let (beaker, fullcolor, progress) = (name == Some("beaker"), name == Some("fullcolor"), name == Some("progress"));
                if let Some(mut enabled) = world.get_mut::<UiInteractionEnabled>(entity) {
                    enabled.set_if_neq(UiInteractionEnabled(beaker));
                }
                if entity == overlay {
                    if let Some(mut node) = world.get_mut::<Node>(entity) {
                        node.position_type = PositionType::Absolute;
                        node.left = px(0.);
                        node.top = px(0.);
                    }
                } else if fullcolor {
                    world.entity_mut(entity).insert(Visibility::Hidden);
                } else if progress {
                    state.progress.push(entity);
                    if let Some(mut minimum) = world.get_mut::<UiMinimum>(entity) {
                        minimum.set_if_neq(UiMinimum(0));
                    }
                    if let Some(mut maximum) = world.get_mut::<UiMaximum>(entity) {
                        maximum.set_if_neq(UiMaximum(10_000));
                    }
                }
            }
            world.entity_mut(overlay).insert(state);
        });
    }
}

#[expect(clippy::needless_pass_by_value)]
pub(crate) fn project_catalogue_research_overlays(
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    unlocks: Res<UnlockedCatalogueDefinitionSet>,
    worlds: Query<
        (
            Entity,
            Ref<SelectedWorldIdentity>,
            Option<Ref<ScenarioContentAvailability>>,
        ),
        With<WorldRoot>,
    >,
    availability: Query<Ref<ResearchProjectAvailability>>,
    projects: Query<Ref<ResearchProject>>,
    payments: Query<Ref<PendingResearchProjectPayment>>,
    mut removals: (
        RemovedComponents<ResearchProjectAvailability>,
        RemovedComponents<ResearchProject>,
        RemovedComponents<PendingResearchProjectPayment>,
    ),
    mut overlays: Query<(Entity, &ResearchOverlayOf, &mut CatalogueResearchOverlay)>,
    rows: Query<&CatalogueRowEntry>,
    mut nodes: Query<(&mut Visibility, Option<&mut UiValue>)>,
    mut previous_world: Local<Option<(Entity, bool)>>,
) {
    if overlays.is_empty() {
        return;
    }
    let reindex = active_definitions.is_changed() || definitions.is_changed();
    let definitions = active_definitions.get(&definitions);
    let world = worlds.iter().next();
    let world_key = world
        .as_ref()
        .map(|(entity, _, scenario)| (*entity, scenario.is_some()));
    let inputs_changed = reindex
        || unlocks.is_changed()
        || *previous_world != world_key
        || world.as_ref().is_some_and(|(_, identity, scenario)| {
            identity.is_changed() || scenario.as_ref().is_some_and(Ref::is_changed)
        })
        || availability.iter().any(|value| value.is_changed())
        || projects.iter().any(|value| value.is_changed())
        || payments.iter().any(|value| value.is_changed())
        || !removals.0.is_empty()
        || !removals.1.is_empty()
        || !removals.2.is_empty();
    removals.0.clear();
    removals.1.clear();
    removals.2.clear();
    *previous_world = world_key;
    for (overlay, owner, mut state) in &mut overlays {
        let row = rows.get(owner.0).ok().map(|row| row.definition);
        if !inputs_changed && state.projected_row == Some(row) {
            continue;
        }
        state.projected_row = Some(row);
        let entry = row.and_then(|id| definitions?.catalogue_entry_for_definition(id));
        let research =
            entry.and_then(|(_, entry)| definitions?.research_for_definition(entry.definition));
        let project = research.and_then(|id| projects.iter().find(|p| p.definition == id));
        let pending = research.is_some_and(|id| payments.iter().any(|p| p.definition == id));
        let requires_research = entry.is_some_and(|(order, entry)| {
            let definitions = definitions.expect("entry belongs to loaded definitions");
            !catalogue_entry_is_available(
                definitions,
                order,
                entry,
                world.as_ref().map(|(_, identity, _)| identity.mode),
                world
                    .as_ref()
                    .and_then(|(_, _, scenario)| scenario.as_deref()),
                &unlocks,
            ) && (project.is_some()
                || pending
                || catalogue_entry_can_be_researched(
                    definitions,
                    entry,
                    availability.iter().map(|item| (item.item, item.available)),
                ))
        });
        if let Ok((mut visibility, _)) = nodes.get_mut(overlay) {
            visibility.set_if_neq(if requires_research {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        for &progress in &state.progress {
            if let Ok((mut visibility, value)) = nodes.get_mut(progress) {
                visibility.set_if_neq(if requires_research && (pending || project.is_some()) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                if let Some(mut value) = value {
                    value.set_if_neq(UiValue(project.as_ref().map_or(0, |project| {
                        ((u128::from(project.elapsed_ticks.min(project.required_ticks)) * 10_000)
                            / u128::from(project.required_ticks.max(1)))
                            as i64
                    })));
                }
            }
        }
    }
}
