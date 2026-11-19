use arrayvec::ArrayVec;
use bevy::prelude::*;

use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::SetUiListRowCount;
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::UiListRow;

use openzt2_interface_state::information::entity_selection_types::InformationEntitySource;
use openzt2_interface_state::information::entity_selection_types::Inspectable;

use super::super::information_view_types::EntityEditorDataRootSurface;

const MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS: usize = 256;

#[derive(Resource, Default)]
pub(crate) struct InspectableMembership;

pub(crate) fn inspectable_added(
    _: On<Add<Inspectable>>,
    mut membership: ResMut<InspectableMembership>,
) {
    membership.set_changed();
}

pub(crate) fn inspectable_removed(
    _: On<Remove<Inspectable>>,
    mut membership: ResMut<InspectableMembership>,
) {
    membership.set_changed();
}

/// Projects reusable entity-editor rows directly from live inspectable ECS
/// entities rather than retaining another editor hierarchy.
pub(crate) fn project_inspectable_entities_to_visible_entity_editor_data_roots(
    editor_data_root_surfaces: Query<
        (Entity, &InheritedVisibility),
        With<EntityEditorDataRootSurface>,
    >,
    reusable_rows: Query<(
        Entity,
        &UiListRow,
        &ChildOf,
        Option<&InformationEntitySource>,
    )>,
    inspectable_entities: Query<Entity, With<Inspectable>>,
    membership: Res<InspectableMembership>,
    mut members: Local<Option<ArrayVec<Entity, MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS>>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
    mut commands: Commands,
) {
    if membership.is_changed() {
        *members = None;
    }
    for (surface_entity, inherited_visibility) in &editor_data_root_surfaces {
        if !inherited_visibility.get() {
            continue;
        }
        let members = members.get_or_insert_with(|| {
            let mut members: ArrayVec<_, MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS> = inspectable_entities
                .iter()
                .take(MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS)
                .collect();
            members.sort_unstable_by_key(|entity| entity.to_bits());
            members
        });
        let mut row_count = 0;
        for (row_entity, row, parent, source) in &reusable_rows {
            if parent.parent() != surface_entity {
                continue;
            }
            row_count += 1;
            let subject = members.get(usize::from(row.index)).copied();
            if source.map(|source| source.0) != subject {
                if let Some(subject) = subject {
                    commands
                        .entity(row_entity)
                        .insert(InformationEntitySource(subject));
                } else {
                    commands
                        .entity(row_entity)
                        .remove::<InformationEntitySource>();
                }
            }
        }
        if row_count != members.len() {
            row_count_requests.write(SetUiListRowCount {
                list: surface_entity,
                count: members.len() as u16,
            });
        }
    }
}
