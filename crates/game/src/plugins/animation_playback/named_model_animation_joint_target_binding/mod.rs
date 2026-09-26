use bevy::{
    animation::{AnimatedBy, AnimationTargetId},
    ecs::system::SystemParam,
    platform::collections::HashSet,
    prelude::*,
};

use crate::plugins::animation_graph::animation_graph_playback_state_types::AnimationGraphPlaybackState;

use super::{
    animation_joint_target_types::AnimationJointTarget,
    animation_playback_controller_ancestor_lookup::find_animation_playback_controller_ancestor,
    animation_playback_controller_types::AnimationPlaybackController,
};

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
    named_entities: Query<(&Name, Option<&AnimationJointTarget>, Option<&AnimatedBy>)>,
    children: Query<&Children>,
    entity_parents: Query<&ChildOf>,
    animation_players: Query<(), With<AnimationPlayer>>,
    animation_playback_controllers: Query<(
        &AnimationPlaybackController,
        Option<&AnimationGraphPlaybackState>,
    )>,
    mut visited_entities: Local<HashSet<Entity>>,
) {
    visited_entities.clear();
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
        let Ok((entity_name, binding, animated_by)) = named_entities.get(named_entity) else {
            continue;
        };
        let Some((animation_playback_controller_entity, _, _)) =
            find_animation_playback_controller_ancestor(
                named_entity,
                &entity_parents,
                &animation_playback_controllers,
            )
        else {
            if binding.is_some() {
                commands
                    .entity(named_entity)
                    .remove::<(AnimationJointTarget, AnimationTargetId, AnimatedBy)>();
            }
            continue;
        };
        let has_player = animation_players.contains(animation_playback_controller_entity);
        if binding.is_some_and(|binding| {
            binding.animation_playback_controller_entity == animation_playback_controller_entity
                && binding.joint_asset_key == entity_name.as_str()
        }) && animated_by.map(|owner| owner.0)
            == has_player.then_some(animation_playback_controller_entity)
        {
            continue;
        }
        insert_animation_joint_target_binding(
            &mut commands,
            named_entity,
            entity_name,
            animation_playback_controller_entity,
            &animation_players,
        );
    }
}

fn insert_animation_joint_target_binding(
    commands: &mut Commands,
    named_entity: Entity,
    entity_name: &Name,
    animation_playback_controller_entity: Entity,
    animation_players: &Query<(), With<AnimationPlayer>>,
) {
    let mut named_entity_commands = commands.entity(named_entity);
    named_entity_commands.insert(AnimationJointTarget {
        animation_playback_controller_entity,
        joint_asset_key: entity_name.as_str().to_owned(),
    });
    if animation_players.contains(animation_playback_controller_entity) {
        named_entity_commands.insert((
            AnimationTargetId::from_name(entity_name),
            AnimatedBy(animation_playback_controller_entity),
        ));
    } else {
        named_entity_commands.remove::<(AnimationTargetId, AnimatedBy)>();
    }
}

#[cfg(test)]
mod tests {
    use super::super::animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackRepetitionPolicy, AnimationPlaybackState,
    };
    use super::*;

    fn controller() -> AnimationPlaybackController {
        AnimationPlaybackController {
            playback_generation: 0,
            explicit_clip_request_id: None,
            animation_set_asset: Handle::default(),
            animation_clip_asset_key: "idle".to_owned(),
            elapsed_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_state: AnimationPlaybackState::default(),
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
            Some(&AnimationTargetId::from_name(&Name::new("Spine")))
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
