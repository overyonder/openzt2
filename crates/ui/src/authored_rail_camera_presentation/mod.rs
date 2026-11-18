use std::collections::HashMap;

use bevy::{
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
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
    source: Handle<ScenePrefabAsset>,
    camera: Entity,
    keys: Vec<(Vec3, Quat)>,
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
struct UiRailCameraSceneOwner(Entity);

#[derive(Component)]
#[relationship_target(relationship = UiRailCameraSceneOwner, linked_spawn)]
pub(super) struct UiRailCameraSceneEntities(Vec<Entity>);

#[derive(serde::Deserialize, serde::Serialize)]
struct SavedRailCamera {
    keys: Vec<([f32; 3], [f32; 4])>,
}

pub(super) fn hydrate_rail_cameras(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    rails: Query<(Entity, &UiRailCameraPresentation), Without<UiRailCameraRuntime>>,
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
        let camera = commands
            .spawn((
                Name::new("authored rail camera"),
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
            ))
            .id();
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
        commands.entity(owner).insert((
            ImageNode::new(target),
            UiRailClipTime::default(),
            UiRailCameraRuntime {
                source: presentation.0.clone(),
                camera,
                keys,
                elapsed: 0.0,
                playing: true,
                commands: [false; 14],
            },
        ));
    }
}

pub(super) fn update_rail_cameras(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut rails: Query<(
        &InheritedVisibility,
        &UiSelected,
        &mut UiRailCameraRuntime,
        &mut UiRailClipTime,
    )>,
    mut cameras: Query<(&mut Camera, &mut Transform), With<Camera3d>>,
) {
    for (visible, active, mut runtime, mut clip_time) in &mut rails {
        let camera_is_active = visible.get() && active.0;
        let Ok((mut camera, mut camera_transform)) = cameras.get_mut(runtime.camera) else {
            continue;
        };
        if camera.is_active != camera_is_active {
            camera.is_active = camera_is_active;
        }
        if !camera_is_active {
            clip_time.set_if_neq(UiRailClipTime(None));
            continue;
        }
        let Some(prefab) = prefabs.get(&runtime.source) else {
            continue;
        };
        let Some(rail) = prefab
            .canonical_scene_prefab_document()
            .rail_camera
            .as_ref()
        else {
            continue;
        };
        apply_hotkeys(&keys, &rail.hotkeys, &mut runtime);
        apply_developer_commands(time.delta_secs(), &mut runtime, &mut camera_transform);
        let segment_duration = rail.seconds_per_segment.max(f32::EPSILON);
        let loop_duration = segment_duration * runtime.keys.len().max(1) as f32;
        if runtime.playing && runtime.keys.len() > 1 {
            runtime.elapsed += time.delta_secs()
                * if runtime.commands[command_index(PrefabRailCameraCommand::Fast)] {
                    4.0
                } else {
                    1.0
                };
            runtime.elapsed = runtime.elapsed.rem_euclid(loop_duration);
            let segment =
                (runtime.elapsed / segment_duration).floor() as usize % runtime.keys.len();
            let next = (segment + 1) % runtime.keys.len();
            let t = (runtime.elapsed % segment_duration) / segment_duration;
            camera_transform.translation = runtime.keys[segment].0.lerp(runtime.keys[next].0, t);
            camera_transform.rotation = runtime.keys[segment].1.slerp(runtime.keys[next].1, t);
        }
        clip_time.set_if_neq(UiRailClipTime(Some(
            runtime.elapsed.rem_euclid(loop_duration) / loop_duration,
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

fn apply_hotkeys(
    input: &ButtonInput<KeyCode>,
    bindings: &[openzt2_game_data::scene_prefab::PrefabRailCameraHotkey],
    runtime: &mut UiRailCameraRuntime,
) {
    for binding in bindings {
        let Some(key) = key_code(binding.key_code) else {
            continue;
        };
        let changed = if binding.triggered_on_press {
            input.just_pressed(key)
        } else {
            input.just_released(key)
        };
        if changed {
            runtime.commands[command_index(binding.command)] = binding.active;
            if binding.active {
                match binding.command {
                    PrefabRailCameraCommand::Play => runtime.playing = true,
                    PrefabRailCameraCommand::Clear => runtime.keys.clear(),
                    PrefabRailCameraCommand::Save => save_rail(runtime),
                    PrefabRailCameraCommand::Load => load_rail(runtime),
                    _ => {}
                }
            } else if binding.command == PrefabRailCameraCommand::Play {
                runtime.playing = false;
            }
        }
    }
}

fn apply_developer_commands(delta: f32, runtime: &mut UiRailCameraRuntime, camera: &mut Transform) {
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
    if runtime.commands[command_index(PrefabRailCameraCommand::Record)] {
        runtime.keys.push((camera.translation, camera.rotation));
        runtime.commands[command_index(PrefabRailCameraCommand::Record)] = false;
    }
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

fn save_rail(runtime: &UiRailCameraRuntime) {
    let value = SavedRailCamera {
        keys: runtime
            .keys
            .iter()
            .map(|(translation, rotation)| (translation.to_array(), rotation.to_array()))
            .collect(),
    };
    let result = ron::ser::to_string_pretty(&value, ron::ser::PrettyConfig::default())
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            std::fs::write(DEVELOPER_RAIL_PATH, bytes).map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        warn!("failed to save developer rail camera to {DEVELOPER_RAIL_PATH}: {error}");
    }
}

fn load_rail(runtime: &mut UiRailCameraRuntime) {
    let Ok(bytes) = std::fs::read(DEVELOPER_RAIL_PATH).inspect_err(|error| {
        warn!("failed to read developer rail camera from {DEVELOPER_RAIL_PATH}: {error}");
    }) else {
        return;
    };
    let Ok(value) = ron::de::from_bytes::<SavedRailCamera>(&bytes).inspect_err(|error| {
        warn!("failed to parse developer rail camera from {DEVELOPER_RAIL_PATH}: {error}");
    }) else {
        return;
    };
    runtime.keys = value
        .keys
        .into_iter()
        .map(|(translation, rotation)| (Vec3::from_array(translation), Quat::from_array(rotation)))
        .collect();
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
