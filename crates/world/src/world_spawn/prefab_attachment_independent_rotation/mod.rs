use bevy::prelude::*;
use bevy::transform::helper::TransformHelper;

#[derive(Component)]
pub(super) struct PrefabAttachmentIndependentRotation;

pub(super) fn preserve_independent_attachment_orientation(
    attachments: Query<(Entity, &ChildOf), With<PrefabAttachmentIndependentRotation>>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (attachment, parent) in &attachments {
        let Ok(parent_transform) = transforms.p0().compute_global_transform(parent.parent()) else {
            continue;
        };
        let (_, parent_rotation, _) = parent_transform.to_scale_rotation_translation();
        if let Ok(mut transform) = transforms.p1().get_mut(attachment) {
            transform.rotation = parent_rotation.inverse();
        }
    }
}

/// Undoes the scale a part picks up from the node it hangs on, so it keeps its
/// owner's scale.
pub(super) fn preserve_attachment_owner_scale(
    attachments: Query<(
        Entity,
        &ChildOf,
        &openzt2_world_state::world_spawn::prefab_presentation_types::PrefabAttachmentKeepsOwnerScale,
    )>,
    mut transforms: ParamSet<(TransformHelper, Query<&mut Transform>)>,
) {
    for (attachment, parent, keeps) in &attachments {
        let (Ok(parent_transform), Ok(owner_transform)) = (
            transforms.p0().compute_global_transform(parent.parent()),
            transforms.p0().compute_global_transform(keeps.owner),
        ) else {
            continue;
        };
        let parent_scale = parent_transform.to_scale_rotation_translation().0;
        let owner_scale = owner_transform.to_scale_rotation_translation().0;
        if parent_scale.cmpeq(Vec3::ZERO).any() {
            continue;
        }
        if let Ok(mut transform) = transforms.p1().get_mut(attachment) {
            transform.scale = owner_scale / parent_scale;
        }
    }
}
