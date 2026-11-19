use std::collections::{HashMap, VecDeque};

use bevy::{
    animation::{
        animated_field, animation_curves::AnimatableKeyframeCurve, AnimatedBy, AnimationTargetId,
    },
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
    tasks::{block_on, poll_once, IoTaskPool, Task},
};
use openzt2_game_data::scene_prefab::PrefabDirectionalLightKey;
use openzt2_game_data::scene_prefab::PrefabRailCameraCommand;
use openzt2_game_data::scene_prefab::PrefabRailCameraKey;
use openzt2_game_data::AssetId;

use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;

use super::authored_ui_selection_state::UiSelected;

const RAIL_RENDER_LAYER: usize = 30;
const DEVELOPER_RAIL_PATH: &str = "openzt2-rail-camera.ron";

/// Strong handle for one authored rail-camera scene. The scene document owns
/// its render tree, camera path, lighting and developer key map.
#[derive(Component, Debug, Clone)]
pub(super) struct UiRailCameraPresentation(pub(super) Handle<ScenePrefabAsset>);

#[derive(Component)]
pub(super) struct UiRailCameraRuntime {
    clip: Handle<AnimationClip>,
    animation: bevy::animation::graph::AnimationNodeIndex,
    target: AnimationTargetId,
    keys: Vec<RailKey>,
    pending: VecDeque<RailDeveloperCommand>,
    file_task: Option<Task<Result<Option<Vec<RailKey>>, String>>>,
    elapsed: f32,
    playing: bool,
    commands: [bool; 14],
}

#[derive(Component, Default, PartialEq)]
pub(super) struct UiRailClipTime(Option<f32>);

#[derive(Component)]
pub(super) struct UiRailEnvironment;

#[derive(Component)]
pub(super) struct UiRailLightTrack {
    id: AssetId,
    keys: Vec<PrefabDirectionalLightKey>,
}

#[derive(Component)]
#[relationship(relationship_target = UiRailCameraSceneEntities)]
pub(super) struct UiRailCameraSceneOwner(Entity);

#[derive(Component)]
#[relationship_target(relationship = UiRailCameraSceneOwner, linked_spawn)]
pub(super) struct UiRailCameraSceneEntities(Vec<Entity>);

type RailKey = (Vec3, Quat);

#[derive(Message, Clone, Copy)]
pub(super) struct RailDeveloperCommand {
    camera: Entity,
    command: PrefabRailCameraCommand,
    active: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SavedRailCamera {
    keys: Vec<([f32; 3], [f32; 4])>,
}

pub(super) fn hydrate_rail_cameras(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    rails: Query<(Entity, &UiRailCameraPresentation), Without<UiRailCameraSceneEntities>>,
    mut animations: ResMut<Assets<AnimationClip>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
) {
    for (owner, presentation) in &rails {
        let Some(prefab) = prefabs.get(&presentation.0) else {
            continue;
        };
        let Some(rail) = prefab
            .canonical_scene_prefab_document()
            .rail_camera
            .as_ref()
        else {
            continue;
        };
        let [width, height] = rail.render_size;
        let mut target = Image::new_target_texture(
            width.max(1),
            height.max(1),
            TextureFormat::Rgba8UnormSrgb,
            None,
        );
        target.asset_usage = RenderAssetUsages::all();
        target.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING;
        let target = images.add(target);
        let scene = commands
            .spawn((
                Name::new("authored rail-camera scene"),
                Transform::IDENTITY,
                Visibility::Inherited,
                RenderLayers::layer(RAIL_RENDER_LAYER),
                UiRailCameraSceneOwner(owner),
                UiRailEnvironment,
                PrefabAmbientLightContribution {
                    color_linear: Vec3::ZERO,
                    intensity: 0.0,
                },
            ))
            .id();
        let (_, renderables, lights) =
            spawn_prefab_render_tree(&mut commands, prefab, scene, false);
        for renderable in renderables {
            commands
                .entity(renderable)
                .insert(RenderLayers::layer(RAIL_RENDER_LAYER));
        }
        for light in lights {
            commands
                .entity(light)
                .insert(RenderLayers::layer(RAIL_RENDER_LAYER));
        }
        let keys = rail.keys.iter().map(runtime_key).collect::<Vec<_>>();
        let transform = keys
            .first()
            .map(|(translation, rotation)| {
                Transform::from_translation(*translation).with_rotation(*rotation)
            })
            .unwrap_or_default();
        let name = Name::new("authored rail camera");
        let animation_target = AnimationTargetId::from_name(&name);
        let clip = animations.add(rail_clip(animation_target, &keys, rail.seconds_per_segment));
        let (graph, animation) = AnimationGraph::from_clip(clip.clone());
        let camera = commands
            .spawn((
                name,
                Camera3d::default(),
                Camera {
                    is_active: false,
                    clear_color: ClearColorConfig::Custom(Color::BLACK),
                    ..default()
                },
                RenderTarget::Image(target.clone().into()),
                transform,
                RenderLayers::layer(RAIL_RENDER_LAYER),
                UiRailCameraSceneOwner(owner),
                AnimationGraphHandle(graphs.add(graph)),
                AnimationPlayer::default(),
                animation_target,
                UiRailCameraRuntime {
                    clip,
                    animation,
                    target: animation_target,
                    keys,
                    pending: VecDeque::new(),
                    file_task: None,
                    elapsed: 0.0,
                    playing: true,
                    commands: [false; 14],
                },
            ))
            .id();
        commands.entity(camera).insert(AnimatedBy(camera));
        let mut light_tracks = group_light_keys(&rail.directional_light_keys);
        for id in unique_lights(&rail.directional_light_keys) {
            commands.spawn((
                Name::new("authored rail directional light"),
                DirectionalLight::default(),
                Transform::IDENTITY,
                RenderLayers::layer(RAIL_RENDER_LAYER),
                UiRailCameraSceneOwner(owner),
                UiRailLightTrack {
                    id,
                    keys: light_tracks.remove(&id).unwrap_or_default(),
                },
            ));
        }
        commands
            .entity(owner)
            .insert((ImageNode::new(target), UiRailClipTime::default()));
    }
}

fn rail_clip(target: AnimationTargetId, keys: &[RailKey], segment_duration: f32) -> AnimationClip {
    let mut clip = AnimationClip::default();
    let duration = segment_duration.max(f32::EPSILON);
    let timed_keys = || {
        keys.iter()
            .chain(keys.first())
            .enumerate()
            .map(move |(index, key)| (index as f32 * duration, key))
    };
    if let Ok(curve) = AnimatableKeyframeCurve::new(timed_keys().map(|(time, key)| (time, key.0))) {
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(animated_field!(Transform::translation), curve),
        );
    }
    if let Ok(curve) = AnimatableKeyframeCurve::new(timed_keys().map(|(time, key)| (time, key.1))) {
        clip.add_curve_to_target(
            target,
            AnimatableCurve::new(animated_field!(Transform::rotation), curve),
        );
    }
    clip
}

pub(super) fn route_rail_developer_hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    rails: Query<(&UiRailCameraPresentation, &InheritedVisibility, &UiSelected)>,
    cameras: Query<(Entity, &UiRailCameraSceneOwner), With<UiRailCameraRuntime>>,
    mut commands: MessageWriter<RailDeveloperCommand>,
) {
    for (camera, owner) in &cameras {
        let Ok((presentation, visible, active)) = rails.get(owner.0) else {
            continue;
        };
        if !visible.get() || !active.0 {
            continue;
        }
        let Some(rail) = prefabs.get(&presentation.0).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .rail_camera
                .as_ref()
        }) else {
            continue;
        };
        for binding in &rail.hotkeys {
            let Some(key) = key_code(binding.key_code) else {
                continue;
            };
            let edge = if binding.triggered_on_press {
                keys.just_pressed(key)
            } else {
                keys.just_released(key)
            };
            if edge {
                commands.write(RailDeveloperCommand {
                    camera,
                    command: binding.command,
                    active: binding.active,
                });
            }
        }
    }
}

pub(super) fn update_rail_cameras(
    time: Res<Time>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut rails: Query<(
        &UiRailCameraPresentation,
        &InheritedVisibility,
        &UiSelected,
        &mut UiRailClipTime,
    )>,
    mut cameras: Query<(
        &UiRailCameraSceneOwner,
        &mut Camera,
        &mut Transform,
        &mut AnimationPlayer,
        &mut UiRailCameraRuntime,
    )>,
    mut animations: ResMut<Assets<AnimationClip>>,
    mut developer_commands: MessageReader<RailDeveloperCommand>,
) {
    for command in developer_commands.read() {
        if let Ok((_, _, _, _, mut runtime)) = cameras.get_mut(command.camera) {
            runtime.pending.push_back(*command);
        }
    }
    for (owner, mut camera, mut transform, mut player, mut runtime) in &mut cameras {
        let Ok((presentation, visible, active, mut clip_time)) = rails.get_mut(owner.0) else {
            continue;
        };
        let camera_is_active = visible.get() && active.0;
        if camera.is_active != camera_is_active {
            camera.is_active = camera_is_active;
        }
        if !camera_is_active {
            if player.animation(runtime.animation).is_some() {
                player.stop(runtime.animation);
            }
            clip_time.set_if_neq(UiRailClipTime(None));
            continue;
        }
        let Some(rail) = prefabs.get(&presentation.0).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .rail_camera
                .as_ref()
        }) else {
            continue;
        };
        let changed = apply_ordered_developer_commands(&mut runtime);
        let recorded = apply_developer_commands(time.delta_secs(), &mut runtime, &mut transform);
        if changed || recorded {
            if let Some(mut clip) = animations.get_mut(&runtime.clip) {
                *clip = rail_clip(runtime.target, &runtime.keys, rail.seconds_per_segment);
            }
        }
        let duration =
            rail.seconds_per_segment.max(f32::EPSILON) * runtime.keys.len().max(1) as f32;
        if runtime.playing && runtime.keys.len() > 1 {
            let fast = runtime.commands[command_index(PrefabRailCameraCommand::Fast)];
            runtime.elapsed = (runtime.elapsed + time.delta_secs() * if fast { 4.0 } else { 1.0 })
                .rem_euclid(duration);
            player
                .play(runtime.animation)
                .pause()
                .seek_to(runtime.elapsed);
        } else if player.animation(runtime.animation).is_some() {
            player.stop(runtime.animation);
        }
        clip_time.set_if_neq(UiRailClipTime(Some(
            runtime.elapsed.rem_euclid(duration) / duration,
        )));
    }
}

pub(super) fn project_rail_lighting(
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut asset_events: MessageReader<AssetEvent<ScenePrefabAsset>>,
    rails: Query<(
        &UiRailCameraPresentation,
        Ref<UiRailClipTime>,
        &UiRailCameraSceneEntities,
    )>,
    mut ambient: Query<&mut PrefabAmbientLightContribution, With<UiRailEnvironment>>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform, &mut UiRailLightTrack)>,
) {
    let changed_prefabs: std::collections::HashSet<_> = asset_events
        .read()
        .filter_map(|event| match *event {
            AssetEvent::Modified { id } => Some(id),
            _ => None,
        })
        .collect();
    for (presentation, clip_time, targets) in &rails {
        let asset_changed = changed_prefabs.contains(&presentation.0.id());
        if !clip_time.is_changed() && !asset_changed {
            continue;
        }
        let Some(rail) = prefabs.get(&presentation.0).and_then(|prefab| {
            prefab
                .canonical_scene_prefab_document()
                .rail_camera
                .as_ref()
        }) else {
            continue;
        };
        let refreshed_tracks =
            asset_changed.then(|| group_light_keys(&rail.directional_light_keys));
        let environment = clip_time
            .0
            .and_then(|time| sample_environment(&rail.environment_keys, time));
        for target in targets.iter() {
            if let Some((color, _)) = environment {
                if let Ok(mut current) = ambient.get_mut(target) {
                    current.set_if_neq(PrefabAmbientLightContribution {
                        color_linear: Vec3::from_array(color),
                        intensity: 1.0,
                    });
                }
            }
            let Ok((mut light, mut transform, mut track)) = lights.get_mut(target) else {
                continue;
            };
            if let Some(tracks) = &refreshed_tracks {
                track.keys = tracks.get(&track.id).cloned().unwrap_or_default();
            }
            if let Some((_, shadow)) = environment {
                let enabled = shadow > 0.0;
                if light.shadow_maps_enabled != enabled {
                    light.shadow_maps_enabled = enabled;
                }
            }
            if let Some((color, intensity, direction)) =
                clip_time.0.and_then(|time| sample_light(&track.keys, time))
            {
                let color = Color::srgb(color[0], color[1], color[2]);
                let illuminance = intensity * 100_000.0;
                if light.color != color || light.illuminance != illuminance {
                    light.color = color;
                    light.illuminance = illuminance;
                }
                transform.set_if_neq(
                    Transform::IDENTITY.looking_to(Vec3::from_array(direction), Vec3::Y),
                );
            }
        }
    }
}

fn sample_environment(
    keys: &[openzt2_game_data::scene_prefab::PrefabEnvironmentKey],
    time: f32,
) -> Option<([f32; 3], f32)> {
    let (first, second, t) = bracket(keys.iter(), time, |key| key.time)?;
    Some((
        std::array::from_fn(|index| {
            first.ambient_srgb[index] + (second.ambient_srgb[index] - first.ambient_srgb[index]) * t
        }),
        first.shadow_strength + (second.shadow_strength - first.shadow_strength) * t,
    ))
}

fn runtime_key(key: &PrefabRailCameraKey) -> (Vec3, Quat) {
    let [x, y, z] = key.rotation_xyz;
    (
        Vec3::from_array(key.translation_m),
        Quat::from_euler(EulerRot::XYZ, -x, -z, -y),
    )
}

fn unique_lights(keys: &[PrefabDirectionalLightKey]) -> Vec<AssetId> {
    keys.iter().fold(Vec::new(), |mut ids, key| {
        if !ids.contains(&key.light) {
            ids.push(key.light);
        }
        ids
    })
}

fn group_light_keys(
    keys: &[PrefabDirectionalLightKey],
) -> HashMap<AssetId, Vec<PrefabDirectionalLightKey>> {
    let mut tracks = HashMap::<AssetId, Vec<PrefabDirectionalLightKey>>::new();
    for key in keys {
        tracks.entry(key.light).or_default().push(*key);
    }
    tracks
}

fn sample_light(
    keys: &[PrefabDirectionalLightKey],
    time: f32,
) -> Option<([f32; 3], f32, [f32; 3])> {
    let (first, second, t) = bracket(keys.iter(), time, |key| key.time)?;
    Some((
        std::array::from_fn(|index| {
            first.diffuse_srgb[index] + (second.diffuse_srgb[index] - first.diffuse_srgb[index]) * t
        }),
        first.intensity + (second.intensity - first.intensity) * t,
        std::array::from_fn(|index| {
            first.direction[index] + (second.direction[index] - first.direction[index]) * t
        }),
    ))
}

fn bracket<'a, T: 'a>(
    mut keys: impl Iterator<Item = &'a T>,
    time: f32,
    key_time: impl Fn(&T) -> f32,
) -> Option<(&'a T, &'a T, f32)> {
    let first = keys.next()?;
    if time <= key_time(first) {
        return Some((first, first, 0.0));
    }
    let mut lower = first;
    for upper in keys {
        if time <= key_time(upper) {
            let span = (key_time(upper) - key_time(lower)).max(f32::EPSILON);
            return Some((
                lower,
                upper,
                ((time - key_time(lower)) / span).clamp(0.0, 1.0),
            ));
        }
        lower = upper;
    }
    Some((lower, lower, 0.0))
}

fn apply_ordered_developer_commands(runtime: &mut UiRailCameraRuntime) -> bool {
    let mut changed = false;
    if let Some(task) = &mut runtime.file_task {
        let Some(result) = block_on(poll_once(task)) else {
            return false;
        };
        runtime.file_task = None;
        match result {
            Ok(Some(keys)) => {
                runtime.keys = keys;
                changed = true;
            }
            Ok(None) => {}
            Err(error) => warn!("developer rail camera {DEVELOPER_RAIL_PATH}: {error}"),
        }
    }
    while let Some(command) = runtime.pending.pop_front() {
        runtime.commands[command_index(command.command)] = command.active;
        match (command.command, command.active) {
            (PrefabRailCameraCommand::Play, active) => runtime.playing = active,
            (PrefabRailCameraCommand::Clear, true) => {
                runtime.keys.clear();
                changed = true;
            }
            (PrefabRailCameraCommand::Save, true) => {
                let keys = runtime.keys.clone();
                runtime.file_task =
                    Some(IoTaskPool::get().spawn(async move { save_rail(keys).map(|()| None) }));
                break;
            }
            (PrefabRailCameraCommand::Load, true) => {
                runtime.file_task = Some(IoTaskPool::get().spawn(async { load_rail().map(Some) }));
                break;
            }
            _ => {}
        }
    }
    changed
}

fn apply_developer_commands(
    delta: f32,
    runtime: &mut UiRailCameraRuntime,
    camera: &mut Transform,
) -> bool {
    let fast = if runtime.commands[command_index(PrefabRailCameraCommand::Fast)] {
        4.0
    } else {
        1.0
    };
    let movement = Vec3::new(
        axis(
            runtime,
            PrefabRailCameraCommand::Right,
            PrefabRailCameraCommand::Left,
        ),
        axis(
            runtime,
            PrefabRailCameraCommand::Up,
            PrefabRailCameraCommand::Down,
        ),
        axis(
            runtime,
            PrefabRailCameraCommand::Out,
            PrefabRailCameraCommand::In,
        ),
    ) * delta
        * fast;
    camera.translation += camera.rotation * movement;
    let roll = axis(
        runtime,
        PrefabRailCameraCommand::RollRight,
        PrefabRailCameraCommand::RollLeft,
    );
    camera.rotate_local_z(roll * delta * fast);
    let record = runtime.file_task.is_none()
        && runtime.commands[command_index(PrefabRailCameraCommand::Record)];
    if record {
        runtime.keys.push((camera.translation, camera.rotation));
        runtime.commands[command_index(PrefabRailCameraCommand::Record)] = false;
    }
    record
}

fn axis(
    runtime: &UiRailCameraRuntime,
    positive: PrefabRailCameraCommand,
    negative: PrefabRailCameraCommand,
) -> f32 {
    u8::from(runtime.commands[command_index(positive)]) as f32
        - u8::from(runtime.commands[command_index(negative)]) as f32
}

const fn command_index(command: PrefabRailCameraCommand) -> usize {
    command as usize
}

fn save_rail(keys: Vec<RailKey>) -> Result<(), String> {
    let value = SavedRailCamera {
        keys: keys
            .into_iter()
            .map(|(translation, rotation)| (translation.to_array(), rotation.to_array()))
            .collect(),
    };
    let bytes = ron::ser::to_string_pretty(&value, ron::ser::PrettyConfig::default())
        .map_err(|error| error.to_string())?;
    std::fs::write(DEVELOPER_RAIL_PATH, bytes).map_err(|error| error.to_string())
}

fn load_rail() -> Result<Vec<RailKey>, String> {
    let bytes = std::fs::read(DEVELOPER_RAIL_PATH).map_err(|error| error.to_string())?;
    let saved: SavedRailCamera = ron::de::from_bytes(&bytes).map_err(|error| error.to_string())?;
    Ok(saved
        .keys
        .into_iter()
        .map(|(translation, rotation)| (Vec3::from_array(translation), Quat::from_array(rotation)))
        .collect())
}

fn key_code(code: u16) -> Option<KeyCode> {
    Some(match code {
        16 => KeyCode::ShiftLeft,
        32 => KeyCode::Space,
        65 => KeyCode::KeyA,
        67 => KeyCode::KeyC,
        68 => KeyCode::KeyD,
        80 => KeyCode::KeyP,
        82 => KeyCode::KeyR,
        83 => KeyCode::KeyS,
        87 => KeyCode::KeyW,
        116 => KeyCode::F5,
        117 => KeyCode::F6,
        187 => KeyCode::Equal,
        188 => KeyCode::Comma,
        189 => KeyCode::Minus,
        190 => KeyCode::Period,
        _ => return None,
    })
}
