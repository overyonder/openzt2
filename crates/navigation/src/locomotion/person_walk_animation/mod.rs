//! Projects person navigation onto animation. Staff use their authored gait
//! tables and sampled clip speeds; guests retain their configured walk speed.

use super::locomotion_clip_dependencies::{
    GaitClipBinding, GaitRigJoints, LocomotionClipDependencies,
};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use openzt2_game_assets::animation::animation_clip_source_queries::AnimationClipSources;
use openzt2_game_assets::animation::animation_set_asset_types::{
    AnimationClipSourceAsset, AnimationSetAsset,
};
use openzt2_game_assets::world_definitions::{
    world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
};
use openzt2_game_data::{behavior::action::wander::BehaviorGroundLocomotionSpeed, AssetId};
use openzt2_zoo_state::animal_behavior::behavior_random_stream_state::BehaviorRandomStream;
use openzt2_zoo_state::behavior_task_execution_types::running_actions::BehaviorLocomotionIntent;
use openzt2_zoo_state::locomotion::locomotion_types::{LocomotionMode, SwimLayer};
use openzt2_zoo_state::staff::staff_employment_types::StaffRole;

use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::AnimationClipPlaybackRequest,
        animation_graph_playback_state_types::AnimationGraphEnabled,
        animation_presentation_relationship_types::AnimationPresentationOwner,
    },
    animation_playback::animation_playback_controller_types::{
        AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
    },
};
use openzt2_zoo_state::{
    behavior_task_execution_types::{
        BehaviorHeldAnimationClip, PendingBehaviorAnimationClipCompletion,
    },
    guests::guest_simulation_types::Guest,
    locomotion::locomotion_types::{Destination, DirectLocomotion},
    staff::staff_employment_types::Staff,
};

const WALK_CLIP: &str = "Walk_Ahead";
const STAND_CLIP: &str = "Stand_Idle";

#[derive(Component)]
pub(super) struct PersonGaitPlayback {
    binder: AssetId,
    row: AssetId,
    node: String,
    rate: i16,
    rate_reset: Timer,
    rates: [f32; 2],
}

#[derive(SystemParam)]
pub(super) struct PersonAnimationAssets<'w> {
    sets: Res<'w, Assets<AnimationSetAsset>>,
    clip_assets: Res<'w, Assets<AnimationClipSourceAsset>>,
    sources: AnimationClipSources<'w>,
    definitions: Option<Res<'w, WorldDefinitions>>,
    definition_assets: Option<Res<'w, Assets<WorldDefinitionAsset>>>,
    time: Option<Res<'w, Time<Fixed>>>,
}

use super::sampled_animation_motion_projection::LocomotionAnimationSource;
use openzt2_world_state::animation_playback::locomotion_root_motion::ConsumedLocomotionRootMotion;
use openzt2_zoo_state::locomotion::locomotion_types::{CaffeineFlags, NavAgent};

pub(super) fn project_person_motion_to_walk_animation(
    mut commands: Commands,
    assets: PersonAnimationAssets,
    mut controllers: Query<(
        Entity,
        &AnimationPlaybackController,
        &AnimationPresentationOwner,
        Option<&AnimationPlayer>,
        Option<&GaitRigJoints>,
        Option<&mut LocomotionClipDependencies>,
        Option<&mut GaitClipBinding>,
        Option<&AnimationGraphEnabled>,
        Option<&mut ConsumedLocomotionRootMotion>,
        Option<&mut PersonGaitPlayback>,
    )>,
    parents: Query<&ChildOf>,
    transforms: Query<&GlobalTransform>,
    mut people: Query<
        (
            Has<Destination>,
            Has<DirectLocomotion>,
            Has<PendingBehaviorAnimationClipCompletion>,
            Has<BehaviorHeldAnimationClip>,
            Option<&mut NavAgent>,
            Option<&mut LocomotionAnimationSource>,
            Option<&StaffRole>,
            Option<&LocomotionMode>,
            Option<&SwimLayer>,
            Option<&BehaviorLocomotionIntent>,
            Option<&mut BehaviorRandomStream>,
            Has<openzt2_zoo_state::staff::staff_employment_types::StaffClimbingTankGate>,
            Option<&openzt2_zoo_state::behavior_task_execution_types::BehaviorLocomotionModifier>,
        ),
        Or<(With<Guest>, With<Staff>)>,
    >,
    mut playback_requests: MessageWriter<AnimationClipPlaybackRequest>,
    caffeine_flags: Query<&CaffeineFlags>,
) {
    for (
        controller_entity,
        controller,
        owner,
        player,
        rig,
        dependencies,
        clip_binding,
        enabled,
        mut consumed,
        mut gait,
    ) in &mut controllers
    {
        let person = owner.gameplay_entity;
        let Ok((
            navigating,
            direct,
            awaited_clip,
            held_clip,
            mut agent,
            mut source,
            role,
            mode,
            layer,
            intent,
            mut random,
            climbing,
            modifier,
        )) = people.get_mut(person)
        else {
            continue;
        };
        if climbing {
            continue;
        }
        let task_animation = awaited_clip || held_clip;
        let moving = navigating || direct;
        if !moving || task_animation || enabled.is_some_and(|enabled| !enabled.0) {
            if source
                .as_ref()
                .is_some_and(|source| source.controller == controller_entity)
            {
                commands
                    .entity(person)
                    .remove::<LocomotionAnimationSource>();
                commands
                    .entity(controller_entity)
                    .remove::<ConsumedLocomotionRootMotion>();
            }
        }
        if task_animation || enabled.is_some_and(|enabled| !enabled.0) {
            continue;
        }
        // Attached heads and props have independent controllers without these clips.
        let catalogue = assets
            .definitions
            .as_ref()
            .zip(assets.definition_assets.as_ref())
            .and_then(|(index, definitions)| index.get(definitions));
        let role_definition =
            role.and_then(|role| catalogue.and_then(|catalogue| catalogue.find_staff(role.0)));
        let speed = intent.map(|intent| intent.0).unwrap_or_default();
        let binder = if mode == Some(&LocomotionMode::Swim) {
            if layer == Some(&SwimLayer::Submerged) {
                "underwater"
            } else {
                "water"
            }
        } else if modifier.is_some_and(|modifier| modifier.modifier == AssetId::from_key("carry")) {
            "groundcarry"
        } else {
            "ground"
        };
        let table = role_definition.and_then(|role| {
            role.locomotion_tables
                .iter()
                .find(|table| table.binder == AssetId::from_key(binder))
                .or_else(|| {
                    role.locomotion_tables
                        .iter()
                        .find(|table| table.binder == AssetId::from_key("ground"))
                })
        });
        let speed_key = if moving {
            speed.row_key()
        } else {
            BehaviorGroundLocomotionSpeed::Stop.row_key()
        };
        let row = table.and_then(|table| {
            table
                .rows
                .iter()
                .find(|row| row.key == speed_key)
                .or_else(|| table.rows.first())
        });
        let node_key =
            row.map(|row| row.node.as_str())
                .unwrap_or(if moving { WALK_CLIP } else { STAND_CLIP });
        let assets_changed = assets.sets.is_changed() || assets.clip_assets.is_changed();
        let resolved_node = !assets_changed
            && moving
            && clip_binding.as_ref().is_some_and(|binding| {
                binding.matches(controller.animation_set_asset.id(), node_key, false)
            });
        if !has_locomotion_node {
            if let Some(agent) = agent.filter(|_| moving && row.is_some()) {
                agent
                    .map_unchanged(|agent| &mut agent.max_speed_mps)
                    .set_if_neq(0.0);
            }
            continue;
        }
        let sampled_rate = if let (Some(table), Some(row)) = (table, row) {
            let mut new_gait = PersonGaitPlayback {
                binder: AssetId::default(),
                row: AssetId::default(),
                node: String::new(),
                rate: 1000,
                rate_reset: Timer::default(),
                rates: [1.0; 2],
            };
            let newly_projected = gait.is_none();
            let current = gait.as_deref_mut().unwrap_or(&mut new_gait);
            let rate_due = current
                .rate_reset
                .tick(
                    assets
                        .time
                        .as_ref()
                        .map_or_else(std::time::Duration::default, |time| time.delta()),
                )
                .is_finished();
            if current.binder != table.binder
                || current.row != row.key
                || current.node != row.node
                || current.rates != row.playback_speed_range
                || (row.reset_seconds_range[1] > 0.0 && rate_due)
            {
                let sample = random
                    .as_deref_mut()
                    .map_or(0.0, |random| random.0.unit_f32());
                let rate = row.playback_speed_range[0]
                    + sample * (row.playback_speed_range[1] - row.playback_speed_range[0]);
                current.rate = (rate * 1000.0).round().clamp(0.0, f32::from(i16::MAX)) as i16;
                let sample = random
                    .as_deref_mut()
                    .map_or(0.0, |random| random.0.unit_f32());
                let reset_seconds = row.reset_seconds_range[0]
                    + sample * (row.reset_seconds_range[1] - row.reset_seconds_range[0]);
                current.rate_reset = super::fixed_interval_timer(reset_seconds);
                current.binder = table.binder;
                current.row = row.key;
                current.node.clone_from(&row.node);
                current.rates = row.playback_speed_range;
            }
            let rate = current.rate;
            if newly_projected {
                commands.entity(controller_entity).insert(new_gait);
            }
            Some(rate)
        } else {
            None
        };
        let Some(set) = assets.sets.get(&controller.animation_set_asset) else {
            continue;
        };
        let stationary_clip = (!moving)
            .then(|| table.and_then(|table| table.stationary_clip.as_deref()))
            .flatten();
        LocomotionClipDependencies::retain(
            &mut commands,
            controller_entity,
            dependencies,
            controller.animation_set_asset.id(),
            stationary_clip.unwrap_or(node_key),
            set,
            &assets.sources,
            assets.sets.is_changed(),
        );
        let resolved = GaitClipBinding::resolve(
            &mut commands,
            controller_entity,
            clip_binding,
            controller.animation_set_asset.id(),
            stationary_clip.unwrap_or(node_key),
            stationary_clip.is_some(),
            assets_changed,
            set,
            &assets.sources,
            || {
                let selected = stationary_clip.map(str::to_owned).or_else(|| {
                    let node = set.animation_graph_nodes.iter().find(|node| node.animation_graph_node_asset_key == node_key)?;
                    openzt2_world_state::animation_graph::animation_graph_transition_operations::select_preferred_animation_clip_from_graph_node(
                        set, |key| assets.sources.get(set, key).is_some(), node,
                    )
                });
                Ok(selected.unwrap_or_else(|| node_key.to_owned()))
            },
        );
        let Ok(clip) = resolved else {
            if let Some(agent) = agent.filter(|_| moving && row.is_some()) {
                agent
                    .map_unchanged(|agent| &mut agent.max_speed_mps)
                    .set_if_neq(0.0);
            }
            continue;
        };
        let wanted = clip.key.as_ref();
        let binding = moving
            .then(|| {
                let parent = rig?.root_parent(clip.joint?, &parents, &transforms)?;
                let actor = transforms.get(person).ok()?;
                let speed = clip.velocity(parent)?.length();
                if speed <= 0.0 {
                    return None;
                }
                // A gait row's rate also sets the agent's speed below, so
                // scaling it speeds the person up or slows them down.
                // People without a gait row move at their
                // NavAgent speed and keep it; scale that too if one appears.
                let rate = if let Some(rate) = sampled_rate {
                    caffeine_flags
                        .get(person)
                        .copied()
                        .unwrap_or_default()
                        .apply_to_rate(f32::from(rate), super::DEFAULT_LOCOMOTION_RATE_FACTORS)
                        .round()
                        .clamp(1.0, f32::from(i16::MAX)) as i16
                } else {
                    (agent.as_deref()?.max_speed_mps / speed * 1000.0)
                        .round()
                        .clamp(1.0, f32::from(i16::MAX)) as i16
                };
                Some((
                    actor.affine().inverse() * parent.affine(),
                    rate,
                    speed * f32::from(rate) / 1000.0,
                ))
            })
            .flatten();
        let rate = binding.map_or(1000, |(_, rate, _)| rate);
        if row.is_some() {
            if let Some(agent) = agent.take() {
                let active = controller.animation_clip_asset_key == wanted
                    && controller.playback_speed_permille == rate
                    && player.is_some_and(|player| {
                        player
                            .playing_animations()
                            .any(|(_, active)| !active.is_paused() && !active.is_finished())
                    })
                    && consumed
                        .as_ref()
                        .is_some_and(|motion| motion.owns_playback(controller));
                let speed = if active {
                    binding.map_or(0.0, |(_, _, speed)| speed)
                } else {
                    0.0
                };
                agent
                    .map_unchanged(|agent| &mut agent.max_speed_mps)
                    .set_if_neq(speed);
            }
        }
        let needs_request = controller.animation_clip_asset_key != wanted
            || (moving
                && binding.is_some()
                && (controller.playback_speed_permille != rate
                    || consumed
                        .as_ref()
                        .is_none_or(|motion| !motion.owns_playback(controller))));
        if needs_request {
            let request = playback_requests.write(AnimationClipPlaybackRequest {
                animation_subject_entity: controller_entity,
                animation_clip_asset_key: wanted.to_owned(),
                blend_duration_milliseconds: 0,
                playback_speed_permille: rate,
                playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
            });
            if let Some((parent, _, mean_speed_mps)) = binding {
                let motion = ConsumedLocomotionRootMotion::new(request.id, parent);
                if let Some(mut current) = consumed {
                    *current = motion;
                } else {
                    commands.entity(controller_entity).insert(motion);
                }
                let next = LocomotionAnimationSource {
                    controller: controller_entity,
                    mean_speed_mps,
                };
                if let Some(mut current) = source {
                    current.set_if_neq(next);
                } else {
                    commands.entity(person).insert(next);
                }
            }
        } else if let (Some((parent, _, mean_speed_mps)), Some(motion)) =
            (binding, consumed.as_deref_mut())
        {
            motion.update_root_parent_transform(parent);
            let next = LocomotionAnimationSource {
                controller: controller_entity,
                mean_speed_mps,
            };
            if let Some(mut current) = source.take() {
                current.set_if_neq(next);
            } else {
                commands.entity(person).insert(next);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openzt2_game_assets::animation::animation_set_asset_types::{
        AnimationClipRecord, AnimationClipSourceAsset,
    };
    use openzt2_world_state::animation_playback::animation_playback_controller_types::AnimationPlaybackClock;

    #[test]
    fn navigating_guest_walks_and_stopped_guest_stands() {
        let mut app = App::new();
        let mut sets = Assets::<AnimationSetAsset>::default();
        let set = sets.add(AnimationSetAsset::create_ready_animation_set_asset(
            String::new(),
            String::new(),
            [WALK_CLIP, STAND_CLIP]
                .map(|key| {
                    AnimationClipRecord::create_embedded_animation_clip_record(
                        key.to_owned(),
                        Handle::default(),
                        1000,
                    )
                })
                .into(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            None,
        ));
        app.add_plugins(AssetPlugin::default())
            .init_asset::<AnimationClipSourceAsset>()
            .insert_resource(sets)
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
                AnimationPresentationOwner {
                    gameplay_entity: guest,
                },
                AnimationPlaybackController {
                    playback_generation: 0,
                    explicit_clip_request_id: None,
                    animation_set_asset: set,
                    animation_clip_asset_key: STAND_CLIP.to_owned(),
                    preserve_seek_on_restart: false,
                    playback_speed_permille: 1000,
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
