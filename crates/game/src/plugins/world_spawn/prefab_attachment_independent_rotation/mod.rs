use bevy::{prelude::*, transform::helper::TransformHelper};

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
