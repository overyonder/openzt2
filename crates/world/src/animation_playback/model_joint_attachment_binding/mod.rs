use bevy::prelude::*;
use openzt2_world_state::animation_playback::model_joint_attachment_binding::PendingModelJointAttachment;

pub(super) fn bind_model_joint_attachments(
    mut commands: Commands,
    pending: Query<(Entity, &PendingModelJointAttachment)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
) {
    for (attachment, binding) in &pending {
        let joint = binding.find_joint(attachment, &children, &names, &parents);
        let Some(joint) = joint else {
            continue;
        };
        // Identity under the joint follows its animated world transform.
        commands
            .entity(attachment)
            .insert((ChildOf(joint), Transform::IDENTITY))
            .remove::<PendingModelJointAttachment>();
    }
}

#[cfg(test)]
mod tests;
