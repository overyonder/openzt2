use arrayvec::ArrayVec;
use bevy::prelude::*;

use crate::plugins::ui::authored_reusable_list_and_table_runtime_types::{
    SetUiListRowCount, UiListRow,
};

use super::super::{
    entity_selection_types::{InformationEntitySource, Inspectable},
    information_view_types::EntityEditorDataRootSurface,
};

const MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS: usize = 256;

/// Projects reusable entity-editor rows directly from live inspectable ECS
/// entities rather than retaining another editor hierarchy.
pub(in crate::plugins::information) fn project_inspectable_entities_to_visible_entity_editor_data_roots(
    editor_data_root_surfaces: Query<
        (Entity, &InheritedVisibility),
        With<EntityEditorDataRootSurface>,
    >,
    reusable_rows: Query<(Entity, &UiListRow)>,
    inspectable_entities: Query<Entity, With<Inspectable>>,
    mut row_count_requests: MessageWriter<SetUiListRowCount>,
    mut commands: Commands,
) {
    let mut sorted_inspectable_entities =
        ArrayVec::<Entity, MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS>::new();
    inspectable_entities.iter().for_each(|entity| {
        let _ = sorted_inspectable_entities.try_push(entity);
    });
    sorted_inspectable_entities.sort_unstable_by_key(|entity| entity.to_bits());

    for (surface_entity, inherited_visibility) in &editor_data_root_surfaces {
        if !inherited_visibility.get() {
            continue;
        }
        row_count_requests.write(SetUiListRowCount {
            list: surface_entity,
            count: sorted_inspectable_entities.len() as u16,
        });
        for (row_entity, row) in &reusable_rows {
            if row.list != surface_entity {
                continue;
            }
            if let Some(inspectable_entity) =
                sorted_inspectable_entities.get(usize::from(row.index))
            {
                commands
                    .entity(row_entity)
                    .insert(InformationEntitySource(*inspectable_entity));
            } else {
                commands
                    .entity(row_entity)
                    .remove::<InformationEntitySource>();
            }
        }
    }
}
