use bevy::prelude::*;
use openzt2_game_assets::{
    localization::{
        localization_asset_types::LocalizationAsset,
        localization_precedence_index::LocalizationPrecedenceIndex,
    },
    ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
};
use openzt2_game_data::ui_document::widget_live_collection::UiWidgetLiveCollectionSource;
use openzt2_interface_state::ui::{
    authored_reusable_list_and_table_runtime_types::{
        RequestedUiListRowCount, SetUiListRowCount, UiListPolicy, UiListRow,
        UiScenarioGoalGroupDocument,
    },
    ui_document_lifecycle_contracts::ShowUiDocument,
};

use super::{
    authored_document_projection::{project_document_on_entity, UiDocumentLifetimeAttachment},
    document_asset_readiness::{document_dependencies_resolved, UiDocumentAssetEvents},
};
use crate::authored_image_selection_diagnostic_override::UiAuthoredImageSelectionDiagnosticOverride;

#[derive(Component)]
pub(crate) struct PendingUiListRow(u16);

pub(crate) fn record_requested_ui_list_row_counts(
    mut requests: MessageReader<SetUiListRowCount>,
    mut lists: Query<&mut RequestedUiListRowCount>,
) {
    for request in requests.read() {
        if let Ok(mut count) = lists.get_mut(request.list) {
            count.set_if_neq(RequestedUiListRowCount(Some(request.count)));
        }
    }
}

pub(crate) fn reconcile_changed_ui_list_row_counts(
    mut commands: Commands,
    lists: Query<
        (
            Entity,
            &RequestedUiListRowCount,
            Option<&Children>,
            &UiListPolicy,
        ),
        (
            Without<UiScenarioGoalGroupDocument>,
            Or<(Changed<RequestedUiListRowCount>, Changed<UiListPolicy>)>,
        ),
    >,
    rows: Query<&UiListRow>,
    pending: Query<&PendingUiListRow>,
) {
    for (list, count, children, policy) in &lists {
        let Some(count) = count.0 else {
            continue;
        };
        let mut occupied = vec![false; usize::from(count)];
        for child in children.into_iter().flatten().copied() {
            let index = rows
                .get(child)
                .map(|row| row.index)
                .or_else(|_| pending.get(child).map(|row| row.0));
            if let Ok(index) = index {
                match occupied.get_mut(usize::from(index)) {
                    Some(slot) if !*slot => *slot = true,
                    _ => commands.entity(child).despawn(),
                }
            }
        }
        if policy.row_document.is_some() {
            for index in (0..count).filter(|index| !occupied[usize::from(*index)]) {
                commands.spawn((
                    PendingUiListRow(index),
                    UiDocumentLifetimeAttachment,
                    ChildOf(list),
                ));
            }
        }
    }
}

pub(crate) fn materialize_ready_ui_list_rows(
    mut commands: Commands,
    lists: Query<Ref<UiListPolicy>>,
    server: Res<AssetServer>,
    mut asset_events: UiDocumentAssetEvents,
    pending: Query<(Entity, Ref<PendingUiListRow>, &ChildOf)>,
    documents: Res<Assets<UiDocumentAsset>>,
    active_localization: Res<LocalizationPrecedenceIndex>,
    localizations: Res<Assets<LocalizationAsset>>,
    image_selection: Option<Res<UiAuthoredImageSelectionDiagnosticOverride>>,
) {
    let changes = asset_events.collect_changes();
    let Some(localization) = active_localization.borrow_loaded_localization_view(&localizations)
    else {
        return;
    };
    if changes.is_empty()
        && !pending.iter().any(|(_, row, parent)| {
            row.is_added()
                || lists
                    .get(parent.parent())
                    .is_ok_and(|policy| policy.is_changed())
        })
    {
        return;
    }
    let image_selection_index = image_selection
        .as_deref()
        .map(UiAuthoredImageSelectionDiagnosticOverride::candidate_index);

    for (row, pending, parent) in &pending {
        let Ok(policy) = lists.get(parent.parent()) else {
            continue;
        };
        let Some(handle) = &policy.row_document else {
            continue;
        };
        if (!pending.is_added() && !policy.is_changed() && !changes.affects(handle.id()))
            || !document_dependencies_resolved(&server, handle)
        {
            continue;
        }
        let Some(document) = documents.get(handle) else {
            continue;
        };
        project_document_on_entity(
            &mut commands,
            &ShowUiDocument {
                document: handle.clone(),
                owner: parent.parent(),
            },
            document,
            localization,
            image_selection_index,
            false,
            Some(row),
        );
        let mut row_commands = commands.entity(row);
        match policy.source {
            UiWidgetLiveCollectionSource::ZooMessageLatest
            | UiWidgetLiveCollectionSource::ZooMessageHistory => {
                row_commands
                    .entry::<Node>()
                    .and_modify(|mut node| node.width = percent(100.0));
            }
            _ => {}
        }
        row_commands
            .remove::<PendingUiListRow>()
            .insert(UiListRow { index: pending.0 });
    }
}

#[derive(Component)]
pub(crate) struct UiAwardPanel(pub Option<bool>);

#[derive(Component)]
pub(crate) struct UiAwardGroup {
    program: Entity,
    document: Handle<UiDocumentAsset>,
}

#[derive(Component)]
pub(crate) struct UiAwardMilestone {
    program: Entity,
    index: usize,
    bar: Option<Entity>,
    pending: Option<Entity>,
    received: Option<Entity>,
}

pub(crate) fn project_authored_award_group_rows(
    mut commands: Commands,
    layouts: Query<(Entity, Ref<UiAwardPanel>, Option<&Children>)>,
    existing: Query<&UiAwardGroup>,
    worlds: Query<
        Ref<openzt2_interface_state::information::award_presentation_types::WorldAwardPrograms>,
        With<openzt2_world_state::world_spawn::world_membership_types::WorldRoot>,
    >,
    groups: Query<
        Ref<openzt2_interface_state::information::award_presentation_types::AwardGroupPresentation>,
    >,
    server: Res<AssetServer>,
    mut removed_programs: RemovedComponents<
        openzt2_interface_state::information::award_presentation_types::WorldAwardPrograms,
    >,
) {
    let removed_programs = removed_programs.read().count() > 0;
    let world = worlds.single().ok();
    for (list, panel, children) in &layouts {
        let members = world
            .as_ref()
            .into_iter()
            .flat_map(|programs| programs.iter());
        let group_sources = members
            .filter_map(|program| Some((program, groups.get(program).ok()?)))
            .collect::<Vec<_>>();
        if !panel.is_added()
            && !removed_programs
            && !world.as_ref().is_some_and(DetectChanges::is_changed)
            && !group_sources.iter().any(|(_, group)| group.is_changed())
        {
            continue;
        }
        let mut projected = children
            .into_iter()
            .flat_map(|nodes| nodes.iter())
            .filter_map(|child| Some((child, existing.get(child).ok()?)))
            .collect::<Vec<_>>();
        let mut order = Vec::new();
        for (program, group) in group_sources.into_iter().filter(|(_, group)| {
            !group.document.is_empty()
                && panel
                    .0
                    .is_none_or(|persistent| group.persistent == persistent)
        }) {
            let path = group.document.replace('\\', "/").to_ascii_lowercase();
            let handle = server.load::<UiDocumentAsset>(path);
            if let Some(index) = projected.iter().position(|(_, existing)| {
                existing.program == program && existing.document == handle
            }) {
                order.push(projected.swap_remove(index).0);
                continue;
            }
            order.push(
                commands
                    .spawn((
                        UiAwardGroup {
                            program,
                            document: handle,
                        },
                        ChildOf(list),
                        super::authored_document_projection::UiDocumentLifetimeAttachment,
                    ))
                    .id(),
            );
        }
        for (entity, _) in projected {
            commands.entity(entity).try_despawn();
        }
        let current_order = children
            .into_iter()
            .flat_map(|children| children.iter())
            .filter(|&entity| existing.contains(entity));
        if !order.iter().copied().eq(current_order) {
            commands.entity(list).insert_children(0, &order);
        }
    }
}

pub(crate) fn hydrate_authored_award_group_rows(
    mut commands: Commands,
    groups: Query<
        (Entity, Ref<UiAwardGroup>, &ChildOf),
        Without<
            openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentRoot,
        >,
    >,
    documents: Res<Assets<UiDocumentAsset>>,
    server: Res<AssetServer>,
    locales: Res<Assets<LocalizationAsset>>,
    index: Res<LocalizationPrecedenceIndex>,
    mut events: UiDocumentAssetEvents,
) {
    let changes = events.collect_changes();
    let Some(localization) = index.borrow_loaded_localization_view(&locales) else {
        return;
    };
    for (root, group, parent) in &groups {
        if (!group.is_added() && !changes.affects(group.document.id()))
            || !document_dependencies_resolved(&server, &group.document)
        {
            continue;
        }
        let Some(document) = documents.get(&group.document) else {
            continue;
        };
        let (_, entities) =
            super::authored_document_projection::project_document_on_entity_with_nodes(
                &mut commands,
                &ShowUiDocument {
                    document: group.document.clone(),
                    owner: parent.parent(),
                },
                document,
                localization,
                None,
                false,
                Some(root),
            );
        let nodes = &document.canonical_ui_document().nodes;
        let Some(award_list) = nodes
            .iter()
            .find(|node| node.name.eq_ignore_ascii_case("awardslist"))
        else {
            continue;
        };
        for (index, &entry) in award_list.children.iter().enumerate() {
            let find = |name: &str| {
                nodes.iter().enumerate().find_map(|(idx, node)| {
                    let mut parent = idx;
                    while parent != entry as usize && parent != 0 {
                        parent = nodes[parent].parent as usize;
                    }
                    (parent == entry as usize && node.name.eq_ignore_ascii_case(name))
                        .then_some(entities[idx])
                })
            };
            commands
                .entity(entities[entry as usize])
                .insert(UiAwardMilestone {
                    program: group.program,
                    index: index + 1,
                    bar: find("bar"),
                    pending: find("pre"),
                    received: find("post"),
                });
        }
    }
}

pub(crate) fn project_authored_award_milestone_presentation(
    milestones: Query<Ref<UiAwardMilestone>>,
    groups: Query<
        Ref<openzt2_interface_state::information::award_presentation_types::AwardGroupPresentation>,
    >,
    mut nodes: Query<&mut Node>,
    mut visibility: Query<&mut Visibility>,
) {
    for milestone in &milestones {
        let Ok(group) = groups.get(milestone.program) else {
            continue;
        };
        if !milestone.is_added() && !group.is_changed() {
            continue;
        }
        let Some(progress) = group.milestones.get(&milestone.index) else {
            continue;
        };
        if let Some(mut bar) = milestone.bar.and_then(|bar| nodes.get_mut(bar).ok()) {
            bar.map_unchanged(|bar| &mut bar.width)
                .set_if_neq(percent(progress.progress.clamp(0.0, 1.0) * 100.0));
        }
        for (entity, show) in [
            (milestone.pending, !progress.received),
            (milestone.received, progress.received),
        ] {
            if let Some(mut visible) = entity.and_then(|entity| visibility.get_mut(entity).ok()) {
                visible.set_if_neq(if show {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}
