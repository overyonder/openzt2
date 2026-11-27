use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::prelude::*;

#[derive(Component)]
pub(super) struct PrefabAttachmentIndependentRotation;

pub(super) fn preserve_independent_attachment_orientation(
    mut detached: RemovedComponents<ChildOf>,
    children: Query<&Children>,
    attachments: Query<&ChildOf, With<PrefabAttachmentIndependentRotation>>,
    mut poses: ParamSet<(
        (
            Query<
                Entity,
                (
                    Or<(With<Children>, With<PrefabAttachmentIndependentRotation>)>,
                    Or<(
                        Changed<Transform>,
                        Changed<ChildOf>,
                        Added<PrefabAttachmentIndependentRotation>,
                    )>,
                ),
            >,
            Query<(
                &Transform,
                Option<&ChildOf>,
                Has<PrefabAttachmentIndependentRotation>,
            )>,
        ),
        Query<&mut Transform, With<PrefabAttachmentIndependentRotation>>,
    )>,
    mut visited: Local<EntityHashSet>,
    mut parent_poses: Local<EntityHashMap<GlobalTransform>>,
) {
    if attachments.is_empty() {
        detached.clear();
        return;
    }
    visited.clear();
    parent_poses.clear();
    let (changed_branches, local_poses) = poses.p0();
    let mut corrected = Vec::new();
    for root in changed_branches.iter().chain(detached.read()) {
        let mut branch = vec![root];
        while let Some(entity) = branch.pop() {
            if !visited.insert(entity) {
                continue;
            }
            if let Ok(children) = children.get(entity) {
                branch.extend(children.iter());
            }
            let Ok(parent) = attachments.get(entity) else {
                continue;
            };
            let Some(parent_pose) =
                calculate_attachment_parent_pose(parent.parent(), &local_poses, &mut parent_poses)
            else {
                continue;
            };
            let rotation = parent_pose.to_scale_rotation_translation().1.inverse();
            corrected.push((entity, rotation));
        }
    }
    let mut local_poses = poses.p1();
    for (entity, rotation) in corrected {
        if let Ok(pose) = local_poses.get_mut(entity) {
            pose.map_unchanged(|pose| &mut pose.rotation)
                .set_if_neq(rotation);
        }
    }
}

fn calculate_attachment_parent_pose(
    entity: Entity,
    poses: &Query<(
        &Transform,
        Option<&ChildOf>,
        Has<PrefabAttachmentIndependentRotation>,
    )>,
    cache: &mut EntityHashMap<GlobalTransform>,
) -> Option<GlobalTransform> {
    if let Some(&pose) = cache.get(&entity) {
        return Some(pose);
    }
    let (local, parent, independent) = poses.get(entity).ok()?;
    let pose = if let Some(parent) = parent {
        let parent = calculate_attachment_parent_pose(parent.parent(), poses, cache)?;
        let mut local = *local;
        if independent {
            local.rotation = parent.to_scale_rotation_translation().1.inverse();
        }
        parent.mul_transform(local)
    } else {
        GlobalTransform::from(*local)
    };
    cache.insert(entity, pose);
    Some(pose)
}
