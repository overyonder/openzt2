//! Bind authored attachments once to Bevy's existing animated joint hierarchy.

use bevy::animation::AnimationTargetId;
use bevy::prelude::*;

/// The local presentation pose of an item held at a model joint or actor root.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CarriedPrefabPresentation {
    pub anchor: Entity,
    /// Final pose of the prefab root in the joint's coordinate system.
    pub local_transform: Transform,
}

/// A socket binding persists for the lifetime of the attachment.
#[derive(Component)]
pub struct ModelJointAttachment {
    pub instance: Entity,
    joint: AnimationTargetId,
}

impl ModelJointAttachment {
    pub fn new(instance: Entity, joint: &str) -> Self {
        Self {
            instance,
            joint: AnimationTargetId::from_name(&Name::new(joint.to_owned())),
        }
    }

    pub fn resolve(
        &self,
        attachment: Entity,
        joints: &ModelInstanceJoints,
        parents: &Query<&ChildOf>,
    ) -> Option<Entity> {
        joints
            .0
            .get(&self.joint)?
            .iter()
            .copied()
            .find(|&candidate| {
                candidate != attachment
                    && parents.contains(candidate)
                    && parents
                        .iter_ancestors::<ChildOf>(candidate)
                        .all(|ancestor| ancestor != attachment)
            })
    }
}

/// Name lookup built in depth-first order when the instance hierarchy changes.
#[derive(Component, Debug, Default, PartialEq, Eq)]
pub struct ModelInstanceJoints(
    bevy::platform::collections::HashMap<AnimationTargetId, Vec<Entity>>,
);

impl ModelInstanceJoints {
    pub fn from_depth_first_names<'a>(
        descendants: impl IntoIterator<Item = (Entity, &'a Name)>,
    ) -> Self {
        let mut joints = Self::default();
        for (entity, name) in descendants {
            joints
                .0
                .entry(AnimationTargetId::from_name(name))
                .or_default()
                .push(entity);
        }
        joints
    }
}
