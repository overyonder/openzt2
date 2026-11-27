use std::sync::Arc;

use bevy::{ecs::entity::EntityHashSet, prelude::*};
use openzt2_game_assets::animation::{
    animation_clip_source_queries::{AnimationClipSources, LoadedAnimationClip},
    animation_set_asset_types::{AnimationClipSourceAsset, AnimationSetAsset},
};
use openzt2_game_data::AssetId as AuthoredId;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};

#[derive(Clone)]
pub(super) struct ResolvedGaitClip {
    pub(super) key: Arc<str>,
    pub(super) joint: Option<AuthoredId>,
    motion: Option<(Vec3, f32)>,
}

impl ResolvedGaitClip {
    fn from_clip(key: String, clip: &LoadedAnimationClip<'_>) -> Self {
        let joint = clip
            .metadata
            .and_then(|metadata| metadata.root_motion_joint.as_ref())
            .map(openzt2_game_data::animation::animation_clip_metadata::AuthoredAnimationRootMotionJoint::joint_id);
        let duration =
            std::time::Duration::from_millis(u64::from(clip.duration_milliseconds)).as_secs_f32();
        let motion = clip
            .metadata
            .and_then(|metadata| metadata.root_motion_transform)
            .filter(|_| joint.is_some() && duration > 0.0)
            .map(|motion| {
                (
                    Vec3::from_array(motion.translation_from_first_to_last_frame),
                    clip.authored_playback_rate() / duration,
                )
            });
        Self {
            key: Arc::from(key),
            joint,
            motion,
        }
    }

    pub(super) fn velocity(&self, parent: &GlobalTransform) -> Option<Vec3> {
        let (translation, rate) = self.motion?;
        let velocity = parent.affine().transform_vector3(translation) * rate;
        velocity.is_finite().then_some(velocity)
    }
}

#[derive(Component)]
pub(super) struct GaitClipBinding {
    animation_set: AssetId<AnimationSetAsset>,
    requested: String,
    direct: bool,
    resolved: Option<ResolvedGaitClip>,
}

impl GaitClipBinding {
    pub(super) fn matches(
        &self,
        animation_set: AssetId<AnimationSetAsset>,
        requested: &str,
        direct: bool,
    ) -> bool {
        self.animation_set == animation_set
            && self.requested == requested
            && self.direct == direct
            && self.resolved.is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve(
        commands: &mut Commands,
        controller: Entity,
        current: Option<Mut<Self>>,
        animation_set: AssetId<AnimationSetAsset>,
        requested: &str,
        direct: bool,
        changed: bool,
        set: &AnimationSetAsset,
        sources: &AnimationClipSources,
        select: impl FnOnce() -> Result<String, &'static str>,
    ) -> Result<ResolvedGaitClip, &'static str> {
        if !changed {
            if let Some(resolved) = current
                .as_deref()
                .filter(|current| current.matches(animation_set, requested, direct))
                .and_then(|current| current.resolved.as_ref())
            {
                return Ok(resolved.clone());
            }
        }
        let resolved = select().and_then(|key| {
            let clip = sources
                .get(set, &key)
                .ok_or("its gait clip is unavailable")?;
            Ok(ResolvedGaitClip::from_clip(key, &clip))
        });
        if let Some(mut current) = current {
            if current.animation_set != animation_set
                || current.requested != requested
                || current.direct != direct
            {
                current.animation_set = animation_set;
                current.requested.clear();
                current.requested.push_str(requested);
                current.direct = direct;
            }
            if current.resolved.is_some() || resolved.is_ok() {
                current.resolved = resolved.as_ref().ok().cloned();
            }
        } else if let Ok(clip) = &resolved {
            commands.entity(controller).insert(Self {
                animation_set,
                requested: requested.to_owned(),
                direct,
                resolved: Some(clip.clone()),
            });
        }
        resolved
    }
}

#[derive(Component, Default, PartialEq)]
pub(super) struct GaitRigJoints(Vec<(AuthoredId, Entity)>);

impl GaitRigJoints {
    pub(super) fn root_parent<'a>(
        &self,
        id: AuthoredId,
        parents: &Query<&ChildOf>,
        transforms: &'a Query<&GlobalTransform>,
    ) -> Option<&'a GlobalTransform> {
        let start = self.0.partition_point(|(joint_id, _)| *joint_id < id);
        self.0[start..]
            .iter()
            .take_while(|(joint_id, _)| *joint_id == id)
            .find_map(|(_, joint)| transforms.get(parents.get(*joint).ok()?.parent()).ok())
    }
}

pub(super) fn project_gait_rig_joint_lookup(
    mut commands: Commands,
    changed_rigs: Query<Entity, Changed<AnimationRigJoints>>,
    changed_joints: Query<
        &AnimationJointTarget,
        (
            Changed<AnimationJointTarget>,
            Allow<bevy::ecs::entity_disabling::Disabled>,
        ),
    >,
    joints: Query<&AnimationJointTarget, Allow<bevy::ecs::entity_disabling::Disabled>>,
    mut removed_rigs: RemovedComponents<AnimationRigJoints>,
    mut controllers: Query<(&AnimationRigJoints, Option<&mut GaitRigJoints>)>,
    stale: Query<(), (With<GaitRigJoints>, Without<AnimationRigJoints>)>,
) {
    for controller in removed_rigs.read().filter(|&entity| stale.contains(entity)) {
        commands.entity(controller).remove::<GaitRigJoints>();
    }
    let affected: EntityHashSet = changed_rigs
        .iter()
        .chain(
            changed_joints
                .iter()
                .map(|joint| joint.animation_playback_controller_entity),
        )
        .collect();
    for controller in affected {
        let Ok((rig, current)) = controllers.get_mut(controller) else {
            continue;
        };
        let mut resolved: Vec<(AuthoredId, Entity)> = rig
            .joints()
            .iter()
            .filter_map(|&entity| {
                joints
                    .get(entity)
                    .ok()
                    .map(|joint| (joint.authored_joint_id, entity))
            })
            .collect();
        resolved.sort_by_key(|(id, _)| *id);
        let next = GaitRigJoints(resolved);
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(controller).insert(next);
        }
    }
}

/// Retains the authored clip sources needed to measure one controller's gait.
#[derive(Component)]
pub(super) struct LocomotionClipDependencies {
    animation_set: AssetId<AnimationSetAsset>,
    gait: String,
    sources: Vec<Handle<AnimationClipSourceAsset>>,
}

impl LocomotionClipDependencies {
    pub(super) fn retain(
        commands: &mut Commands,
        controller: Entity,
        current: Option<Mut<Self>>,
        animation_set: AssetId<AnimationSetAsset>,
        gait: &str,
        set: &AnimationSetAsset,
        sources: &AnimationClipSources,
        animation_sets_changed: bool,
    ) {
        if !animation_sets_changed
            && current.as_ref().is_some_and(|current| {
                current.animation_set == animation_set && current.gait == gait
            })
        {
            return;
        }
        let node = set
            .animation_graph_nodes
            .iter()
            .find(|node| node.animation_graph_node_asset_key == gait);
        let retained: Vec<_> = if let Some(node) = node {
            node.animation_clip_asset_keys
                .iter()
                .filter_map(|key| sources.request(set, key))
                .collect()
        } else {
            sources.request(set, gait).into_iter().collect()
        };
        if let Some(mut current) = current {
            current.animation_set = animation_set;
            current.gait.clear();
            current.gait.push_str(gait);
            current.sources = retained;
        } else {
            commands.entity(controller).insert(Self {
                animation_set,
                gait: gait.to_owned(),
                sources: retained,
            });
        }
    }
}
