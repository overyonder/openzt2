//! Bind authored attachments once to Bevy's existing animated joint hierarchy.

use bevy::{animation::AnimationTargetId, prelude::*};

#[cfg(test)]
mod tests;

/// Pending scene-local lookup; retains only the instance and joint identity.
#[derive(Component)]
pub(crate) struct PendingModelJointAttachment {
    instance: Entity,
    joint: AnimationTargetId,
}

impl PendingModelJointAttachment {
    pub(crate) fn report_unresolved_binding(
        &self,
        attachment: Entity,
        children: &Query<&Children>,
        names: &Query<&Name>,
        parents: &Query<&ChildOf>,
        models: &Query<(
            &crate::plugins::world_spawn::prefab_presentation_types::PrefabModel,
            &InheritedVisibility,
        )>,
        server: &AssetServer,
    ) {
        let mut matching_joints = 0;
        for candidate in children.iter_descendants_depth_first::<Children>(self.instance) {
            if let Ok((model, visibility)) = models.get(candidate) {
                error!(target: "openzt2_map_load_suite", ?attachment, ?candidate,
                    model = model.model_path.as_ref(), visible = visibility.get(),
                    load_state = ?model.handle.as_ref().map(|handle| server.get_load_states(handle.id())),
                    "unresolved joint instance model");
            }
            let Ok(name) = names.get(candidate) else {
                continue;
            };
            if AnimationTargetId::from_name(name) != self.joint {
                continue;
            }
            matching_joints += 1;
            let creates_cycle = candidate == attachment
                || parents
                    .iter_ancestors::<ChildOf>(candidate)
                    .any(|ancestor| ancestor == attachment);
            error!(target: "openzt2_map_load_suite", ?attachment, ?candidate, joint = name.as_str(), creates_cycle,
                "unresolved model joint candidate");
        }
        error!(target: "openzt2_map_load_suite", ?attachment, instance = ?self.instance, joint = ?self.joint,
            matching_joints, "unresolved model joint binding");
    }

    pub(crate) fn new(instance: Entity, joint: &str) -> Self {
        Self {
            instance,
            joint: AnimationTargetId::from_name(&Name::new(joint.to_owned())),
        }
    }
}

pub(super) fn bind_model_joint_attachments(
    mut commands: Commands,
    pending: Query<(Entity, &PendingModelJointAttachment)>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
) {
    for (attachment, binding) in &pending {
        let joint = children
            .iter_descendants_depth_first::<Children>(binding.instance)
            .find(|candidate| {
                *candidate != attachment
                    && names
                        .get(*candidate)
                        .is_ok_and(|name| AnimationTargetId::from_name(name) == binding.joint)
                    && !parents
                        .iter_ancestors::<ChildOf>(*candidate)
                        .any(|ancestor| ancestor == attachment)
            });
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
