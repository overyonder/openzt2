use super::*;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::CarriedPrefabPresentation;

pub(super) fn bind_carried_prefab_presentation(
    mut commands: Commands,
    memberships: InteractionContainerMemberships,
    mut objects: Query<
        (
            Entity,
            &ContainedObject,
            Option<&mut CarriedPrefabPresentation>,
        ),
        Or<(
            Changed<ContainedObject>,
            Changed<InteractionContainerMember>,
        )>,
    >,
) {
    for (item, contained, pose) in &mut objects {
        let Some(anchor) = contained.joint.or_else(|| {
            memberships
                .container_for_member(item)
                .map(|(holder, _)| holder)
        }) else {
            continue;
        };
        let next = CarriedPrefabPresentation {
            anchor,
            local_transform: contained.relative_transform,
        };
        if let Some(mut pose) = pose {
            pose.set_if_neq(next);
        } else {
            commands.entity(item).insert(next);
        }
    }
}

pub(super) fn unbind_carried_prefab_presentation(
    removed: On<Remove<ContainedObject>>,
    mut items: Query<(&CarriedPrefabPresentation, &mut Transform)>,
    transforms: Query<&GlobalTransform>,
    mut commands: Commands,
) {
    if let Ok((pose, mut transform)) = items.get_mut(removed.entity) {
        if let Ok(anchor) = transforms.get(pose.anchor) {
            transform.set_if_neq(
                anchor
                    .mul_transform(pose.local_transform)
                    .compute_transform(),
            );
        }
    }
    commands
        .entity(removed.entity)
        .try_remove::<CarriedPrefabPresentation>();
}
