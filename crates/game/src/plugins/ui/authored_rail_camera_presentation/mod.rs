use bevy::{
    asset::RenderAssetUsages,
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget},
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
};
use openzt2_game_data::scene_prefab::{
    PrefabDirectionalLightKey, PrefabRailCameraCommand, PrefabRailCameraKey,
};

use crate::{
    assets::scene_prefab::ScenePrefabAsset,
    plugins::world_spawn::{
        prefab_ambient_light_contribution::PrefabAmbientLightContribution,
        prefab_presentation_render_tree::spawn_prefab_render_tree,
    },
};

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
    environment: Entity,
    lights: Vec<(openzt2_game_data::AssetId, Entity)>,
    keys: Vec<(Vec3, Quat)>,
    elapsed: f32,
    playing: bool,
    commands: [bool; 14],
}

#[derive(Component)]
pub(super) struct UiRailCameraSceneOwner(Entity);

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
        let lights = unique_lights(&rail.directional_light_keys)
            .into_iter()
            .map(|id| {
                let entity = commands
                    .spawn((
                        Name::new("authored rail directional light"),
                        DirectionalLight::default(),
                        Transform::IDENTITY,
                        RenderLayers::layer(RAIL_RENDER_LAYER),
                        UiRailCameraSceneOwner(owner),
                    ))
                    .id();
                (id, entity)
            })
            .collect();
        commands.entity(owner).insert((
            ImageNode::new(target),
            UiRailCameraRuntime {
                source: presentation.0.clone(),
                camera,
                environment: scene,
                lights,
                keys,
                elapsed: 0.0,
                playing: true,
                commands: [false; 14],
            },
        ));
    }
}

pub(super) fn cleanup_rail_camera_scenes(
    mut commands: Commands,
    owners: Query<(), With<UiRailCameraPresentation>>,
    spawned: Query<(Entity, &UiRailCameraSceneOwner)>,
) {
    for (entity, owner) in &spawned {
        if owners.get(owner.0).is_err() {
            commands.entity(entity).despawn();
        }
    }
}

pub(super) fn update_rail_cameras(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut rails: Query<(&InheritedVisibility, &UiSelected, &mut UiRailCameraRuntime)>,
    mut cameras: Query<(&mut Camera, &mut Transform), With<Camera3d>>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), Without<Camera3d>>,
    mut ambient: Query<&mut PrefabAmbientLightContribution>,
) {
    for (visible, active, mut runtime) in &mut rails {
        let camera_is_active = visible.get() && active.0;
        let Ok((mut camera, mut camera_transform)) = cameras.get_mut(runtime.camera) else {
            continue;
        };
        if camera.is_active != camera_is_active {
            camera.is_active = camera_is_active;
        }
        if !camera_is_active {
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
        let normalized_time = runtime.elapsed.rem_euclid(loop_duration) / loop_duration;
        if let Some((color, shadow)) = sample_environment(&rail.environment_keys, normalized_time) {
            let contribution = PrefabAmbientLightContribution {
                color_linear: Vec3::from_array(color),
                intensity: 1.0,
            };
            if let Ok(mut current) = ambient.get_mut(runtime.environment) {
                *current = contribution;
            }
            for (_, entity) in &runtime.lights {
                if let Ok((mut light, _)) = lights.get_mut(*entity) {
                    light.shadow_maps_enabled = shadow > 0.0;
                }
            }
        }
        for (id, entity) in &runtime.lights {
            let Ok((mut light, mut transform)) = lights.get_mut(*entity) else {
                continue;
            };
            if let Some((color, intensity, direction)) =
                sample_light(&rail.directional_light_keys, *id, normalized_time)
            {
                light.color = Color::srgb(color[0], color[1], color[2]);
                light.illuminance = intensity * 100_000.0;
                *transform = Transform::IDENTITY.looking_to(Vec3::from_array(direction), Vec3::Y);
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

fn unique_lights(keys: &[PrefabDirectionalLightKey]) -> Vec<openzt2_game_data::AssetId> {
    keys.iter().fold(Vec::new(), |mut ids, key| {
        if !ids.contains(&key.light) {
            ids.push(key.light);
        }
        ids
    })
}

fn sample_light(
    keys: &[PrefabDirectionalLightKey],
    id: openzt2_game_data::AssetId,
    time: f32,
) -> Option<([f32; 3], f32, [f32; 3])> {
    let (first, second, t) = bracket(keys.iter().filter(|key| key.light == id), time, |key| {
        key.time
    })?;
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
