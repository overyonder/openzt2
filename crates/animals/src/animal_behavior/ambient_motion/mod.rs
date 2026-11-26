use bevy::{prelude::*, transform::helper::TransformHelper};
use openzt2_game_assets::{
    animation::{
        animation_clip_source_queries::AnimationClipSources,
        animation_set_asset_types::AnimationSetAsset,
    },
    application_lifecycle::GamePhase,
    behavior::behavior_asset_types::{BehaviorDocumentAsset, LoadedBehaviorDocumentCollection},
    world_definitions::{
        world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
        world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset,
    },
};
use openzt2_game_data::{
    behavior::{
        action::{
            animation::BehaviorAnimationPlaybackSpeed, wander::BehaviorGroundLocomotionSpeed,
        },
        action_record::BehaviorAction,
    },
    AssetId,
};
use openzt2_world_state::{
    animation_graph::{
        animation_graph_playback_message_types::AnimationClipPlaybackRequest,
        animation_graph_playback_state_types::AnimationGraphPlaybackState,
        animation_presentation_relationship_types::{
            AnimationPresentationOwner, AnimationPresentations,
        },
    },
    animation_playback::{
        animation_joint_target_types::{AnimationJointTarget, AnimationRigJoints},
        animation_playback_controller_types::{
            AnimationPlaybackController, AnimationPlaybackRepetitionPolicy,
        },
        locomotion_root_motion::{AnimationLocomotionMotionSampling, ConsumedLocomotionRootMotion},
    },
    application_schedule::FixedGameSet,
    world_spawn::world_membership_types::{DefinitionId, WorldMember},
};
use openzt2_zoo_state::{
    animal_behavior::{
        behavior_random_stream_state::BehaviorRandomStream,
        behavior_set_start_request_types::AmbientBehaviorSubject,
    },
    behavior_task_execution_types::running_actions::{
        BehaviorStepFixedSet, RunBehaviorAnimationClip, RunBehaviorAnimationSwitch,
        RunBehaviorWander,
    },
    locomotion::locomotion_types::LocomotionMode,
    terrain::dry_surface_eligibility::DrySurfaceEligibility,
};

pub(super) fn register(app: &mut App) {
    app.add_systems(
        FixedUpdate,
        (
            play_ambient_wander_clip
                .after(BehaviorStepFixedSet::Think)
                .in_set(FixedGameSet::Think),
            consume_ambient_root_motion
                .before(AnimationLocomotionMotionSampling)
                .in_set(FixedGameSet::Act),
            apply_ambient_root_motion
                .after(AnimationLocomotionMotionSampling)
                .in_set(FixedGameSet::Act),
        )
            .run_if(in_state(GamePhase::InGame)),
    );
}

fn play_ambient_wander_clip(
    mut actors: Query<
        (
            Entity,
            &DefinitionId,
            &RunBehaviorWander,
            &mut BehaviorRandomStream,
            &mut Transform,
        ),
        (With<AmbientBehaviorSubject>, Added<RunBehaviorWander>),
    >,
    definitions: Res<WorldDefinitions>,
    world_assets: Res<Assets<WorldDefinitionAsset>>,
    documents: Res<LoadedBehaviorDocumentCollection>,
    assets: Res<Assets<BehaviorDocumentAsset>>,
    presentations: Query<&AnimationPresentations>,
    controllers: Query<(&AnimationPlaybackController, &AnimationGraphPlaybackState)>,
    sets: Res<Assets<AnimationSetAsset>>,
    mut requests: MessageWriter<AnimationClipPlaybackRequest>,
) {
    let (Some(definitions), Some(view)) = (
        definitions.get(&world_assets),
        documents.create_declaration_index_view(&assets),
    ) else {
        return;
    };
    for (actor, definition, wander, mut random, mut pose) in &mut actors {
        if wander.0.locomotion_speed == BehaviorGroundLocomotionSpeed::Stop {
            continue;
        }
        let Some(definition) = definitions.find_object(definition.0) else {
            continue;
        };
        let Some((document, index)) = view.find_behavior_set_location(
            AssetId::from_key("idle"),
            std::iter::once(definition.id)
                .chain(definition.behavior_type_identifiers.iter().copied()),
        ) else {
            continue;
        };
        let Some(set) = assets
            .get(&document)
            .and_then(|asset| asset.behavior_set_at_index(index))
        else {
            continue;
        };
        let current_node =
            AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
                .find_map(|controller| controllers.get(controller).ok())
                .and_then(|(controller, graph)| {
                    sets.get(&controller.animation_set_asset)?
                        .animation_graph_nodes
                        .get(graph.current_animation_graph_node_index as usize)
                })
                .map(|node| node.animation_graph_node_asset_key.as_str());
        let Some(clip) = set
            .actions
            .lowered_actions()
            .iter()
            .find_map(|action| match action {
                BehaviorAction::AnimationClip(clip) => Some(clip),
                BehaviorAction::AnimationSwitch(switch) => switch
                    .clips_by_node
                    .iter()
                    .find(|(node, _)| {
                        current_node.is_some_and(|current| node.eq_ignore_ascii_case(current))
                    })
                    .map(|(_, clip)| clip)
                    .or(switch.default_clip.as_ref()),
                _ => None,
            })
        else {
            continue;
        };
        let rate = match clip.playback_speed {
            None => 1.0,
            Some(BehaviorAnimationPlaybackSpeed::Fixed(rate)) => rate,
            Some(BehaviorAnimationPlaybackSpeed::Range { minimum, maximum }) => {
                (maximum - minimum).mul_add(random.0.unit_f32(), minimum)
            }
        };
        let lateral = random.0.unit_f32().mul_add(2.0, -1.0);
        let yaw = lateral.atan();
        pose.rotation = (pose.rotation * Quat::from_rotation_y(yaw)).normalize();
        requests.write(AnimationClipPlaybackRequest {
            animation_subject_entity: actor,
            animation_clip_asset_key: clip.animation_clip_asset_key.clone(),
            blend_duration_milliseconds: 0,
            playback_speed_permille: (rate * 1000.0).round().clamp(1.0, f32::from(i16::MAX)) as i16,
            playback_repetition_policy: AnimationPlaybackRepetitionPolicy::Loop,
            action_settings: None,
        });
    }
}

fn consume_ambient_root_motion(
    mut commands: Commands,
    actors: Query<(), With<AmbientBehaviorSubject>>,
    mut controllers: Query<(
        Entity,
        &AnimationPresentationOwner,
        &AnimationPlaybackController,
        &AnimationRigJoints,
        Option<&mut ConsumedLocomotionRootMotion>,
    )>,
    sets: Res<Assets<AnimationSetAsset>>,
    clips: AnimationClipSources,
    joints: Query<(&AnimationJointTarget, &ChildOf)>,
    transforms: TransformHelper,
) {
    for (entity, owner, controller, rig, motion) in &mut controllers {
        let actor = owner.gameplay_entity;
        if !actors.contains(actor)
            || motion
                .as_ref()
                .is_some_and(|motion| motion.owns_playback(controller))
        {
            continue;
        }
        let parent = (|| {
            let set = sets.get(&controller.animation_set_asset)?;
            let clip = clips.get(set, &controller.animation_clip_asset_key)?;
            let joint = clip.metadata?.root_motion_joint.as_ref()?;
            rig.joints().iter().find_map(|id| {
                let (binding, parent) = joints.get(*id).ok()?;
                binding
                    .joint_asset_key
                    .eq_ignore_ascii_case(&joint.joint_asset_key)
                    .then_some(parent.parent())
            })
        })();
        let Some((request, parent)) = controller.explicit_clip_request_id.zip(parent) else {
            continue;
        };
        let (Ok(actor_pose), Ok(parent_pose)) = (
            transforms.compute_global_transform(actor),
            transforms.compute_global_transform(parent),
        ) else {
            continue;
        };
        let next = ConsumedLocomotionRootMotion::new(
            request,
            actor_pose.affine().inverse() * parent_pose.affine(),
        );
        if let Some(mut motion) = motion {
            *motion = next;
        } else {
            commands.entity(entity).insert(next);
        }
    }
}

fn apply_ambient_root_motion(
    mut actors: Query<
        (
            Entity,
            &WorldMember,
            &LocomotionMode,
            &mut Transform,
            Option<&RunBehaviorWander>,
        ),
        (
            With<AmbientBehaviorSubject>,
            Or<(
                With<RunBehaviorWander>,
                With<RunBehaviorAnimationClip>,
                With<RunBehaviorAnimationSwitch>,
            )>,
        ),
    >,
    presentations: Query<&AnimationPresentations>,
    controllers: Query<(&AnimationPlaybackController, &ConsumedLocomotionRootMotion)>,
    terrain: DrySurfaceEligibility,
) {
    for (actor, member, mode, mut pose, wander) in &mut actors {
        if wander
            .is_some_and(|wander| wander.0.locomotion_speed == BehaviorGroundLocomotionSpeed::Stop)
        {
            continue;
        }
        let Some((_, motion)) =
            AnimationPresentations::for_subject(actor, presentations.get(actor).ok())
                .filter_map(|entity| controllers.get(entity).ok())
                .find(|(controller, motion)| motion.owns_playback(controller))
        else {
            continue;
        };
        let mut point = pose.translation + pose.rotation * (pose.scale * motion.translation());
        let mut rotation = pose.rotation * motion.rotation();
        let floor = terrain.ground_height(member.root, point.xz());
        let surface = terrain.water_surface_height(member.root, point.xz());
        let in_water = floor
            .zip(surface)
            .is_some_and(|(floor, surface)| surface > floor);
        let refused = wander.is_some_and(|wander| {
            (wander.0.avoid_land && !in_water) || (wander.0.avoid_water && in_water)
        });
        if refused || (*mode == LocomotionMode::Swim && !in_water) {
            point = pose.translation;
            rotation = Quat::from_rotation_y(std::f32::consts::PI) * pose.rotation;
        } else if *mode == LocomotionMode::Swim {
            if let (Some(floor), Some(surface)) = (floor, surface) {
                point.y = point.y.clamp(floor, surface);
                if let Some(wander) = wander {
                    let shallow = (surface - wander.0.minimum_depth_m).max(floor);
                    let deep = (surface - wander.0.maximum_depth_m).clamp(floor, shallow);
                    point.y = point.y.clamp(deep, shallow);
                }
            }
        } else if *mode == LocomotionMode::Ground {
            point.y = floor.unwrap_or(point.y);
        }
        if point != pose.translation || rotation != pose.rotation {
            pose.translation = point;
            pose.rotation = rotation;
        }
    }
}
