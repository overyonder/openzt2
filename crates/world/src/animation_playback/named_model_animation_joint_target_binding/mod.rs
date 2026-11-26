use super::player_clock_lanes::PrefabRealTimeAnimation;
use bevy::animation::AnimatedBy;
use bevy::animation::AnimationTargetId;
use bevy::ecs::system::SystemParam;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;
use openzt2_game_assets::scene_prefab::transform_animation_lowering::PrefabTransformClocks;

use openzt2_world_state::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

use super::standard_bevy_animation_player_projection_types::StandardBevyAnimationJointBlendPose;
use crate::animation_playback::animation_playback_controller_ancestor_lookup::find_animation_playback_controller_ancestor;
use openzt2_world_state::animation_playback::animation_joint_target_types::{
    AnimationJointTarget, AnimationRigJoints,
};
use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackController;

type NewlyAvailableAnimationController = (
    With<AnimationPlaybackController>,
    Or<(Added<AnimationPlaybackController>, Added<AnimationPlayer>)>,
);

#[derive(SystemParam)]
pub(super) struct AnimationJointBindingChanges<'w, 's> {
    renamed_entities: Query<'w, 's, Entity, Changed<Name>>,
    reparented_entities: Query<'w, 's, Entity, Changed<ChildOf>>,
    detached_entities: RemovedComponents<'w, 's, ChildOf>,
    removed_controllers: RemovedComponents<'w, 's, AnimationPlaybackController>,
    removed_players: RemovedComponents<'w, 's, AnimationPlayer>,
    new_controllers: Query<'w, 's, Entity, NewlyAvailableAnimationController>,
}

pub(super) fn bind_new_named_model_entities_and_new_animation_playback_controller_descendants(
    mut commands: Commands,
    mut changes: AnimationJointBindingChanges,
    mut named_entities: Query<
        (
            &Name,
            Option<&AnimationJointTarget>,
            Option<&mut AnimatedBy>,
            Option<&mut AnimationTargetId>,
            Option<&mut StandardBevyAnimationJointBlendPose>,
        ),
        (
            Without<PrefabTransformClocks>,
            Without<PrefabRealTimeAnimation>,
        ),
    >,
    sets: Option<Res<Assets<AnimationSetAsset>>>,
    children: Query<&Children>,
    entity_parents: Query<&ChildOf>,
    animation_players: Query<(), With<AnimationPlayer>>,
    rig_joints: Query<&AnimationRigJoints>,
    animation_playback_controllers: Query<(
        &AnimationPlaybackController,
        Option<&AnimationGraphPlaybackState>,
    )>,
    mut visited_entities: Local<HashSet<Entity>>,
) {
    visited_entities.clear();
    let mut inherited_blend_poses = HashMap::<
        Entity,
        HashMap<AnimationTargetId, StandardBevyAnimationJointBlendPose>,
    >::default();
    let AnimationJointBindingChanges {
        renamed_entities,
        reparented_entities,
        detached_entities,
        removed_controllers,
        removed_players,
        new_controllers,
    } = &mut changes;
    // Moving a model root changes the controller of its entire skeleton, even
    // though individual joints retain their immediate parent. Visit only
    // changed subtrees, once each, rather than scanning settled rigs.
    for named_entity in new_controllers
        .iter()
        .chain(reparented_entities.iter())
        .chain(detached_entities.read())
        .chain(removed_controllers.read())
        .chain(removed_players.read())
        .flat_map(|root| {
            std::iter::once(root).chain(children.iter_descendants_depth_first::<Children>(root))
        })
        .chain(renamed_entities.iter())
    {
        if !visited_entities.insert(named_entity) {
            continue;
        }
        let Ok((entity_name, binding, animated_by, previous_target, _)) =
            named_entities.get(named_entity)
        else {
            continue;
        };
        let Some((animation_playback_controller_entity, controller, _)) =
            find_animation_playback_controller_ancestor(
                named_entity,
                &entity_parents,
                &animation_playback_controllers,
            )
        else {
            if binding.is_some() {
                commands.entity(named_entity).remove::<(
                    AnimationJointTarget,
                    AnimationTargetId,
                    AnimatedBy,
                    StandardBevyAnimationJointBlendPose,
                )>();
            }
            continue;
        };
        let has_player = animation_players.contains(animation_playback_controller_entity);
        let embedded = sets
            .as_ref()
            .and_then(|sets| sets.get(&controller.animation_set_asset))
            .is_some_and(|set| set.embedded_animation_graph.is_some());
        if binding.is_some_and(|binding| {
            binding.animation_playback_controller_entity == animation_playback_controller_entity
                && binding.joint_asset_key == entity_name.as_str()
        }) && animated_by.map(|owner| owner.0)
            == has_player.then_some(animation_playback_controller_entity)
        {
            continue;
        }
        let joint_target =
            || AnimationTargetId::from_name(&Name::new(entity_name.as_str().to_ascii_lowercase()));
        let target_id = if embedded {
            previous_target.copied().unwrap_or_else(joint_target)
        } else {
            joint_target()
        };
        let blend_poses = inherited_blend_poses
            .entry(animation_playback_controller_entity)
            .or_insert_with(|| {
                rig_joints
                    .get(animation_playback_controller_entity)
                    .into_iter()
                    .flat_map(|rig| named_entities.iter_many(rig.joints()).matched())
                    .filter_map(|(name, _, _, previous_target, snapshot)| {
                        let snapshot = snapshot?;
                        Some((
                            previous_target.copied().unwrap_or_else(|| {
                                AnimationTargetId::from_name(&Name::new(
                                    name.as_str().to_ascii_lowercase(),
                                ))
                            }),
                            *snapshot,
                        ))
                    })
                    .collect::<HashMap<_, _>>()
            });
        let inherited_pose = blend_poses.get(&target_id).copied();
        let target = AnimationJointTarget {
            animation_playback_controller_entity,
            joint_asset_key: entity_name.as_str().to_owned(),
            authored_joint_id: openzt2_game_data::AssetId::from_key(
                &entity_name.as_str().to_ascii_lowercase(),
            ),
        };
        let relation_changed = binding.is_none_or(|binding| {
            binding.animation_playback_controller_entity != animation_playback_controller_entity
                || binding.joint_asset_key != target.joint_asset_key
        });
        let Ok((_, _, driver, active_target, snapshot)) = named_entities.get_mut(named_entity)
        else {
            continue;
        };
        let mut entity = commands.entity(named_entity);
        if relation_changed {
            entity.insert(target);
        }
        match (snapshot, inherited_pose) {
            (Some(mut snapshot), Some(pose)) => {
                snapshot.set_if_neq(pose);
            }
            (None, Some(pose)) => {
                entity.insert(pose);
            }
            (Some(_), None) => {
                entity.remove::<StandardBevyAnimationJointBlendPose>();
            }
            (None, None) => {}
        }
        if has_player {
            if let Some(mut active_target) = active_target {
                active_target.set_if_neq(target_id);
            } else {
                entity.insert(target_id);
            }
            if let Some(driver) = driver {
                driver
                    .map_unchanged(|driver| &mut driver.0)
                    .set_if_neq(animation_playback_controller_entity);
            } else {
                entity.insert(AnimatedBy(animation_playback_controller_entity));
            }
        } else {
            entity.remove::<(AnimationTargetId, AnimatedBy)>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock;
    use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackRepetitionPolicy;

    fn controller() -> AnimationPlaybackController {
        AnimationPlaybackController {
            playback_generation: 0,
            explicit_clip_request_id: None,
            animation_set_asset: Handle::default(),
            animation_clip_asset_key: "idle".to_owned(),
            preserve_seek_on_restart: false,
            playback_speed_permille: 1000,

            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::default(),
            playback_clock: AnimationPlaybackClock::default(),
        }
    }

    #[test]
    fn joint_binding_follows_model_reparenting_renaming_and_detachment() {
        let mut app = App::new();
        app.add_systems(
            Update,
            bind_new_named_model_entities_and_new_animation_playback_controller_descendants,
        );
        let first = app
            .world_mut()
            .spawn((controller(), AnimationPlayer::default()))
            .id();
        let second = app
            .world_mut()
            .spawn((controller(), AnimationPlayer::default()))
            .id();
        let model = app.world_mut().spawn(ChildOf(first)).id();
        let joint = app
            .world_mut()
            .spawn((Name::new("Pelvis"), ChildOf(model)))
            .id();
        app.update();
        assert_eq!(
            app.world().get::<AnimatedBy>(joint).map(|owner| owner.0),
            Some(first)
        );
        app.world_mut().entity_mut(model).insert(ChildOf(second));
        app.update();
        assert_eq!(
            app.world().get::<AnimatedBy>(joint).map(|owner| owner.0),
            Some(second)
        );
        app.world_mut().entity_mut(joint).insert(Name::new("Spine"));
        app.update();
        assert_eq!(
            app.world().get::<AnimationTargetId>(joint),
            Some(&AnimationTargetId::from_name(&Name::new("spine")))
        );
        app.world_mut().entity_mut(model).remove::<ChildOf>();
        app.update();
        assert!(app.world().get::<AnimationJointTarget>(joint).is_none());
        assert!(app.world().get::<AnimatedBy>(joint).is_none());
    }

    #[test]
    fn nested_controller_and_late_player_keep_the_nearest_animation_owner() {
        let mut app = App::new();
        app.add_systems(
            Update,
            bind_new_named_model_entities_and_new_animation_playback_controller_descendants,
        );
        let outer = app
            .world_mut()
            .spawn((controller(), AnimationPlayer::default()))
            .id();
        let inner = app.world_mut().spawn((controller(), ChildOf(outer))).id();
        let joint = app
            .world_mut()
            .spawn((Name::new("Pelvis"), ChildOf(inner)))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<AnimationJointTarget>(joint)
                .map(|binding| binding.animation_playback_controller_entity),
            Some(inner)
        );
        assert!(app.world().get::<AnimatedBy>(joint).is_none());
        app.world_mut()
            .entity_mut(inner)
            .insert(AnimationPlayer::default());
        app.update();
        assert_eq!(
            app.world().get::<AnimatedBy>(joint).map(|owner| owner.0),
            Some(inner)
        );
    }
}
