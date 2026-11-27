use bevy::ecs::entity::EntityHashSet;
use bevy::prelude::*;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::{
    ModelInstanceAttachments, ModelInstanceJoints, ModelJointAttachment,
};

pub(super) fn project_model_joint_indexes(
    mut commands: Commands,
    attachments: Query<&ModelJointAttachment, Changed<ModelJointAttachment>>,
    changed: Query<Entity, Or<(Changed<Children>, Changed<Name>)>>,
    mut removed_names: RemovedComponents<Name>,
    mut removed_children: RemovedComponents<Children>,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    names: Query<&Name>,
    mut indexes: Query<&mut ModelInstanceJoints>,
) {
    let mut dirty = attachments
        .iter()
        .map(|attachment| attachment.instance)
        .collect::<EntityHashSet>();
    for entity in changed
        .iter()
        .chain(removed_names.read())
        .chain(removed_children.read())
    {
        dirty.extend(
            std::iter::once(entity)
                .chain(parents.iter_ancestors::<ChildOf>(entity))
                .filter(|&entity| indexes.contains(entity)),
        );
    }
    for entity in dirty {
        let index = ModelInstanceJoints::from_depth_first_names(
            children
                .iter_descendants_depth_first::<Children>(entity)
                .filter_map(|child| names.get(child).ok().map(|name| (child, name))),
        );
        if let Ok(mut existing) = indexes.get_mut(entity) {
            existing.set_if_neq(index);
        } else {
            commands.entity(entity).try_insert(index);
        }
    }
}

pub(super) fn bind_model_joint_attachments(
    mut commands: Commands,
    mut attachments: Query<(Entity, Ref<ModelJointAttachment>, Option<&mut Transform>)>,
    indexes: Query<&ModelInstanceJoints>,
    changed_indexes: Query<&ModelInstanceAttachments, Changed<ModelInstanceJoints>>,
    changes: Query<
        Entity,
        (
            With<ModelJointAttachment>,
            Or<(Changed<ModelJointAttachment>, Changed<ChildOf>)>,
        ),
    >,
    mut removed_parents: RemovedComponents<ChildOf>,
    parents: Query<&ChildOf>,
) {
    let affected = changes
        .iter()
        .chain(removed_parents.read())
        .chain(
            changed_indexes
                .iter()
                .flat_map(ModelInstanceAttachments::iter),
        )
        .collect::<EntityHashSet>();
    let mut attachments = attachments.iter_many_mut(affected).matched();
    while let Some((entity, attachment, transform)) = attachments.fetch_next() {
        let Ok(index) = indexes.get(attachment.instance) else {
            continue;
        };
        let joint = attachment.resolve(entity, &index, &parents);
        let target = joint.unwrap_or(attachment.instance);
        if parents
            .get(entity)
            .is_ok_and(|parent| parent.parent() == target)
        {
            continue;
        }
        let mut entity = commands.entity(entity);
        entity.insert(ChildOf(target));
        if joint.is_some() {
            if let Some(mut transform) = transform {
                transform.set_if_neq(Transform::IDENTITY);
            } else {
                entity.insert(Transform::IDENTITY);
            }
        }
    }
}

#[cfg(test)]
mod tests;
