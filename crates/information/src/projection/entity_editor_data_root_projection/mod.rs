use bevy::{ecs::relationship::RelationshipTarget, prelude::*};

use openzt2_interface_state::information::entity_selection_types::{
    InformationEntitySource, Inspectable,
};
use openzt2_interface_state::ui::authored_reusable_list_and_table_runtime_types::{
    RequestedUiListRowCount, UiListRow,
};
use openzt2_world_state::world_spawn::world_membership_types::{WorldMember, WorldRoot};

use super::super::information_view_types::EntityEditorDataRootSurface;
use super::information_list_projection::bind_information_list_rows;

const MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS: usize = 256;

#[derive(Component)]
#[relationship(relationship_target = InspectableWorldSubjects)]
pub(crate) struct InspectableInWorld(Entity);

#[derive(Component)]
#[relationship_target(relationship = InspectableInWorld)]
pub(crate) struct InspectableWorldSubjects(Vec<Entity>);

pub(crate) fn link_inspectable_world_subjects(
    subjects: Query<
        (Entity, &WorldMember, Option<&InspectableInWorld>),
        (
            With<Inspectable>,
            Or<(Added<Inspectable>, Changed<WorldMember>)>,
        ),
    >,
    mut commands: Commands,
) {
    for (entity, member, previous) in &subjects {
        if previous.is_none_or(|previous| previous.0 != member.root) {
            commands
                .entity(entity)
                .insert(InspectableInWorld(member.root));
        }
    }
}

pub(crate) fn unlink_retired_inspectable_world_subjects(
    event: On<Remove<(Inspectable, WorldMember)>>,
    subjects: Query<(), With<InspectableInWorld>>,
    mut commands: Commands,
) {
    if subjects.contains(event.entity) {
        commands
            .entity(event.entity)
            .try_remove::<InspectableInWorld>();
    }
}

pub(crate) fn project_inspectable_entities_to_visible_entity_editor_data_roots(
    mut surfaces: Query<
        (
            Entity,
            Ref<InheritedVisibility>,
            Option<Ref<Children>>,
            Option<&mut RequestedUiListRowCount>,
        ),
        With<EntityEditorDataRootSurface>,
    >,
    worlds: Query<Option<Ref<InspectableWorldSubjects>>, With<WorldRoot>>,
    mut removed: RemovedComponents<InspectableWorldSubjects>,
    mut rows: Query<(Ref<UiListRow>, Option<&mut InformationEntitySource>)>,
    changed_rows: Query<&ChildOf, Changed<UiListRow>>,
    mut commands: Commands,
) {
    let subjects = worlds.iter().next().flatten();
    let membership_changed = !removed.is_empty() || subjects.as_ref().is_some_and(Ref::is_changed);
    removed.clear();
    let mut ordered_subjects =
        arrayvec::ArrayVec::<Entity, MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS>::new();
    let mut ordered = false;
    let dirty_lists = changed_rows
        .iter()
        .map(ChildOf::parent)
        .collect::<std::collections::HashSet<_>>();
    for (surface, visibility, children, count) in &mut surfaces {
        if !visibility.get()
            || (!membership_changed
                && !visibility.is_changed()
                && !dirty_lists.contains(&surface)
                && !children.as_ref().is_some_and(Ref::is_changed))
        {
            continue;
        }
        if !ordered {
            ordered_subjects.extend(
                subjects
                    .as_ref()
                    .into_iter()
                    .flat_map(|subjects| RelationshipTarget::iter(&**subjects))
                    .take(MAXIMUM_VISIBLE_ENTITY_EDITOR_ROWS),
            );
            ordered_subjects.sort_unstable_by_key(|entity| entity.to_bits());
            ordered = true;
        }
        if let Some(mut count) = count {
            count.set_if_neq(RequestedUiListRowCount(Some(ordered_subjects.len() as u16)));
        }
        bind_information_list_rows(
            children.as_deref(),
            &ordered_subjects,
            &mut rows,
            &mut commands,
        );
    }
}
