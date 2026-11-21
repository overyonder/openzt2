use bevy::prelude::*;
use openzt2_interface_state::{
    information::entity_selection_types::InformationEntitySource,
    ui::{
        authored_reusable_list_and_table_runtime_types::UiListRow,
        authored_ui_node_projection_components::UiValueBinding,
    },
};
use openzt2_ui::{
    authored_ui_image_content_binding::UiImageBinding,
    authored_ui_text_content_binding::UiTextBinding,
};

#[derive(Component)]
#[relationship(relationship_target = InformationRowFields, allow_self_referential)]
pub(crate) struct InformationRowFieldOf(pub(super) Entity);

#[derive(Component)]
#[relationship_target(relationship = InformationRowFieldOf)]
pub(crate) struct InformationRowFields(Vec<Entity>);

pub(crate) fn bind_authored_information_row_fields(
    mut commands: Commands,
    parents: Query<&ChildOf>,
    children: Query<&Children>,
    rows: Query<(), (With<UiListRow>, With<InformationEntitySource>)>,
    changed_rows: Query<Entity, Or<(Added<UiListRow>, Added<InformationEntitySource>)>>,
    mut removed: (
        RemovedComponents<UiListRow>,
        RemovedComponents<InformationEntitySource>,
        RemovedComponents<UiTextBinding>,
        RemovedComponents<UiImageBinding>,
        RemovedComponents<UiValueBinding>,
    ),
    fields: Query<
        (
            Option<&InformationRowFieldOf>,
            Has<UiTextBinding>,
            Has<UiImageBinding>,
            Has<UiValueBinding>,
        ),
        Or<(
            With<InformationRowFieldOf>,
            With<UiTextBinding>,
            With<UiImageBinding>,
            With<UiValueBinding>,
        )>,
    >,
    changed_fields: Query<
        Entity,
        (
            Or<(
                With<UiTextBinding>,
                With<UiImageBinding>,
                With<UiValueBinding>,
            )>,
            Or<(
                Changed<ChildOf>,
                Changed<UiTextBinding>,
                Changed<UiImageBinding>,
                Changed<UiValueBinding>,
            )>,
        ),
    >,
) {
    let mut dirty = bevy::ecs::entity::EntityHashSet::default();
    dirty.extend(
        changed_fields
            .iter()
            .chain(removed.2.read())
            .chain(removed.3.read())
            .chain(removed.4.read()),
    );
    for row in changed_rows
        .iter()
        .chain(removed.0.read())
        .chain(removed.1.read())
    {
        dirty.extend(std::iter::once(row).chain(children.iter_descendants::<Children>(row)));
    }
    for field in dirty {
        let Ok((previous, text, image, value)) = fields.get(field) else {
            continue;
        };
        let row = if text || image || value {
            std::iter::once(field)
                .chain(parents.iter_ancestors::<ChildOf>(field))
                .find(|&candidate| rows.contains(candidate))
        } else {
            None
        };
        if previous.map(|owner| owner.0) == row {
            continue;
        }
        if let Some(row) = row {
            commands.entity(field).insert(InformationRowFieldOf(row));
        } else {
            commands.entity(field).remove::<InformationRowFieldOf>();
        }
    }
}
