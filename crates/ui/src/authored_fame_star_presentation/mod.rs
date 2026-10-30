use bevy::{
    camera::primitives::MeshAabb,
    camera::{visibility::RenderLayers, ClearColorConfig, RenderTarget, ScalingMode},
    prelude::*,
    render::render_resource::TextureFormat,
};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiNodeId;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use openzt2_world_state::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy;
use openzt2_zoo_state::progression::fame_types::Fame;

const STAR_RENDER_LAYER: usize = 28;

#[derive(Component)]
pub(super) struct FameStarScene {
    owner: Entity,
    full: Handle<ScenePrefabAsset>,
    half: Handle<ScenePrefabAsset>,
    hydrated: bool,
    framing_root: Option<Entity>,
    framed: bool,
}
#[derive(Component)]
pub(super) struct FameStar {
    index: u8,
    half: bool,
}

pub(super) fn create_fame_star_scenes(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    nodes: Query<(Entity, &Name), (With<UiNodeId>, Without<FameStarTarget>)>,
) {
    for (owner, name) in &nodes {
        if name.as_str() != "famestarwells" {
            continue;
        }
        debug!(?owner, "creating authored fame-star render target");
        let target = images.add(Image::new_target_texture(
            320,
            64,
            TextureFormat::Rgba8UnormSrgb,
            None,
        ));
        commands.entity(owner).insert(FameStarTarget);
        commands.spawn((
            ImageNode::new(target.clone()),
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Pickable::IGNORE,
            ZIndex(1),
            ChildOf(owner),
        ));
        let scene = commands
            .spawn((
                Transform::IDENTITY,
                Visibility::Inherited,
                PrefabFixedFunctionWorldLightingPolicy {
                    enabled: false,
                    preserve_authored_material_lighting: true,
                },
                FameStarScene {
                    owner,
                    full: server.load("ui/fx/zoofame/star_full.nif#Scene"),
                    half: server.load("ui/fx/zoofame/star_half_full.nif#Scene"),
                    hydrated: false,
                    framing_root: None,
                    framed: false,
                },
            ))
            .id();
        commands.spawn((
            Camera3d::default(),
            Camera {
                order: -20,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::Fixed {
                    width: 10.0,
                    height: 2.0,
                },
                ..OrthographicProjection::default_3d()
            }),
            Transform::from_xyz(0.0, 0.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
            RenderTarget::Image(target.into()),
            RenderLayers::layer(STAR_RENDER_LAYER),
            ChildOf(scene),
        ));
    }
}
#[derive(Component)]
pub(super) struct FameStarTarget;

pub(super) fn hydrate_fame_stars(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    mut scenes: Query<(Entity, &mut FameStarScene)>,
) {
    for (scene, mut state) in &mut scenes {
        if state.hydrated {
            continue;
        }
        let (Some(full), Some(half)) = (prefabs.get(&state.full), prefabs.get(&state.half)) else {
            continue;
        };
        for index in 0..5 {
            for (is_half, prefab) in [(false, full), (true, half)] {
                let position = Vec3::new(f32::from(index) * 2.0 - 4.0, 0.0, 0.0);
                let parent = commands
                    .spawn((
                        FameStar {
                            index,
                            half: is_half,
                        },
                        Transform {
                            translation: position,
                            ..default()
                        },
                        Visibility::Inherited,
                        ChildOf(scene),
                    ))
                    .id();
                let (_, renderables, lights) =
                    spawn_prefab_render_tree(&mut commands, prefab, parent, false);
                if index == 0 && !is_half {
                    state.framing_root = Some(parent);
                }
                for entity in renderables.into_iter().chain(lights) {
                    commands
                        .entity(entity)
                        .insert(RenderLayers::layer(STAR_RENDER_LAYER));
                }
            }
        }
        state.hydrated = true;
    }
}

pub(super) fn present_fame_stars(
    mut commands: Commands,
    fame: Res<Fame>,
    owners: Query<&InheritedVisibility, With<FameStarTarget>>,
    scenes: Query<(Entity, &FameStarScene)>,
    mut stars: Query<(&FameStar, &mut Visibility, &ChildOf)>,
) {
    for (scene, state) in &scenes {
        if owners.get(state.owner).is_err() {
            commands.entity(scene).despawn();
        }
    }
    for (star, mut visibility, parent) in &mut stars {
        let visible = scenes
            .get(parent.parent())
            .ok()
            .and_then(|(_, state)| owners.get(state.owner).ok())
            .is_some_and(|visible| visible.get());
        let full_count = fame.half_stars.min(10) / 2;
        let framing = scenes
            .get(parent.parent())
            .is_ok_and(|(_, state)| !state.framed)
            && star.index == 0
            && !star.half;
        let filled = if star.half {
            star.index == full_count && fame.half_stars % 2 == 1
        } else {
            star.index < full_count
        };
        *visibility = if visible && (filled || framing) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub(super) fn frame_fame_star_models(
    meshes: Res<Assets<Mesh>>,
    mesh_nodes: Query<(&Mesh3d, &GlobalTransform)>,
    children: Query<&Children>,
    transforms: Query<&GlobalTransform>,
    mut scenes: Query<(Entity, &mut FameStarScene)>,
    mut stars: Query<(&FameStar, &mut Transform, &ChildOf)>,
) {
    for (scene, mut state) in &mut scenes {
        if state.framed {
            continue;
        }
        let Some(root) = state.framing_root else {
            continue;
        };
        let Ok(parent) = transforms.get(root) else {
            continue;
        };
        let inverse = parent.affine().inverse();
        let bounds = children
            .iter_descendants(root)
            .filter_map(|entity| {
                let (mesh, transform) = mesh_nodes.get(entity).ok()?;
                let aabb = meshes.get(&mesh.0)?.compute_aabb()?;
                Some((
                    inverse * transform.affine(),
                    Vec3::from(aabb.center),
                    Vec3::from(aabb.half_extents),
                ))
            })
            .flat_map(|(transform, center, half)| {
                (0..8).map(move |corner| {
                    let sign = Vec3::new(
                        if corner & 1 == 0 { -1.0 } else { 1.0 },
                        if corner & 2 == 0 { -1.0 } else { 1.0 },
                        if corner & 4 == 0 { -1.0 } else { 1.0 },
                    );
                    transform.transform_point3(center + half * sign)
                })
            })
            .fold(None, |bounds: Option<(Vec3, Vec3)>, point| {
                Some(bounds.map_or((point, point), |(minimum, maximum)| {
                    (minimum.min(point), maximum.max(point))
                }))
            });
        let Some((minimum, maximum)) = bounds else {
            continue;
        };
        let center = (minimum + maximum) * 0.5;
        let extent = maximum - minimum;
        let scale = 1.8 / extent.x.max(extent.y).max(f32::EPSILON);
        for (star, mut transform, parent) in &mut stars {
            if parent.parent() == scene {
                transform.translation =
                    Vec3::new(f32::from(star.index) * 2.0 - 4.0, 0.0, 0.0) - center * scale;
                transform.scale = Vec3::splat(scale);
            }
        }
        debug!(?minimum, ?maximum, "framed authored fame-star models");
        state.framed = true;
    }
}
