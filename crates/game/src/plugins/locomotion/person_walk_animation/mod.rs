//! Guests and staff move kinematically at their authored speed, so nothing in
//! their motion selects a clip. Play the walk clip while they navigate and
//! return to standing when they stop, unless a behavior owns the clip.

use bevy::prelude::*;
use openzt2_game_assets::animation::animation_set_asset_types::AnimationSetAsset;

use super::locomotion_types::{Destination, DirectLocomotion};
use crate::plugins::{
    animation_graph::{
        animation_graph_playback_message_types::AnimationClipPlaybackRequest,
        animation_graph_playback_state_types::AnimationGraphEnabled,
    },
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
    },
    behavior_task_execution_types::PendingBehaviorAnimationClipCompletion,
    guests::guest_simulation_types::Guest,
    staff::staff_employment_types::Staff,
};

const WALK_CLIP: &str = "Walk_Ahead";
const STAND_CLIP: &str = "Stand_Idle";

pub(super) fn project_person_motion_to_walk_animation(
    animation_sets: Res<Assets<AnimationSetAsset>>,
    controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        Option<&AnimationGraphEnabled>,
    )>,
    parents: Query<&ChildOf>,
    people: Query<
        (
            Has<Destination>,
            Has<DirectLocomotion>,
            Has<PendingBehaviorAnimationClipCompletion>,
        ),
        Or<(With<Guest>, With<Staff>)>,
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    for (controller_entity, controller, enabled) in &controllers {
        if enabled.is_some_and(|enabled| !enabled.0) {
            continue;
        }
        let Some(person) = parents
            .iter_ancestors(controller_entity)
            .find(|ancestor| people.contains(*ancestor))
        else {
            continue;
        };
        let Ok((navigating, direct, task_animation)) = people.get(person) else {
            continue;
        };
        if task_animation {
            continue;
        }
        let moving = navigating || direct;
        let clip = &controller.animation_clip_asset_key;
        let wanted = match (moving, clip == WALK_CLIP) {
            (true, false) => WALK_CLIP,
            (false, true) => STAND_CLIP,
            _ => continue,
        };
        // Attached heads and props have their own controller without these
        // clips; a clip still loading already has its request deferred.
        let Some(set) = animation_sets.get(&controller.animation_set_asset) else {
            continue;
        };
        if set
            .find_animation_clip_source_reference_by_asset_key(wanted)
            .is_none()
            || set.animation_clip_asset_load_has_failed(wanted)
            || set.animation_clip_asset_load_is_pending(wanted)
        {
            continue;
        }
        playback_requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: controller_entity,
            animation_clip_asset_key: wanted.to_owned(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: 1000,
            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::animation_playback::animation_playback_controller_types::{
        AnimationPlaybackClock, AnimationPlaybackState,
    };
    use openzt2_game_data::animation::animation_set::AuthoredAnimationSetClipReference;

    #[test]
    fn navigating_guest_walks_and_stopped_guest_stands() {
        let mut app = App::new();
        let mut sets = Assets::<AnimationSetAsset>::default();
        let set = sets.add(AnimationSetAsset {
            model_asset_path: String::new(),
            skeleton_asset_path: String::new(),
            animation_clips: Vec::new(),
            animation_graph_nodes: Vec::new(),
            animation_graph_order: Vec::new(),
            animation_graph_edges: Vec::new(),
            animation_graph_metadata: None,
            animation_clip_source_references: [WALK_CLIP, STAND_CLIP]
                .map(|key| AuthoredAnimationSetClipReference {
                    animation_clip_asset_key: key.to_owned(),
                    animation_clip_asset_path: String::new(),
                    playback_policy: default(),
                })
                .into(),
            pending_animation_clip_source_asset_handles: Vec::new(),
            failed_animation_clip_asset_keys: Vec::new(),
        });
        app.insert_resource(sets)
            .add_message::<AnimationClipPlaybackRequest>()
            .add_systems(Update, project_person_motion_to_walk_animation);
        let guest = app
            .world_mut()
            .spawn((
                Guest,
                Destination {
                    request_id: 1,
                    world: Vec3::X,
                    arrival_radius_m: 0.1,
                },
            ))
            .id();
        let controller = app
            .world_mut()
            .spawn((
                ChildOf(guest),
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: None,
                    animation_set_asset: set,
                    animation_clip_asset_key: STAND_CLIP.to_owned(),
                    elapsed_milliseconds: 0,
                    playback_speed_permille: 1000,
                    playback_state: AnimationPlaybackState::Playing,
                    playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
                    playback_clock: AnimationPlaybackClock::Simulation,
                },
            ))
            .id();
        let requested = |app: &mut App| {
            app.update();
            app.world_mut()
                .resource_mut::<Messages<AnimationClipPlaybackRequest>>()
                .drain()
                .map(|request| request.animation_clip_asset_key)
                .collect::<Vec<_>>()
        };
        assert_eq!(requested(&mut app), [WALK_CLIP]);
        app.world_mut()
            .get_mut::<AnimationPlaybackController>(controller)
            .unwrap()
            .animation_clip_asset_key = WALK_CLIP.to_owned();
        assert!(requested(&mut app).is_empty());
        app.world_mut().entity_mut(guest).remove::<Destination>();
        assert_eq!(requested(&mut app), [STAND_CLIP]);
    }
}
