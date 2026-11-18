use bevy::asset::RenderAssetUsages;
use bevy::ecs::relationship::RelationshipTarget;
use bevy::gltf::GltfMaterialName;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use openzt2_game_assets::material::material_asset_types::MaterialAsset;
use openzt2_world_state::model_render::ModelExpanded;
use openzt2_world_state::world_spawn::prefab_presentation_types::{
    PrefabMaterialOverride, PrefabMaterialOverrides,
};

use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::AssetId;
use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankGeometry, TankWallTopHeight};

#[derive(Component)]
#[relationship(relationship_target = TankPresentations)]
pub(super) struct TankPresentationOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = TankPresentationOf, linked_spawn)]
pub(super) struct TankPresentations(Vec<Entity>);

#[derive(Component, Clone, Copy, PartialEq)]
pub(super) struct TankPresentationGeometry {
    floor_height: f32,
    fallback_wall_height: f32,
}

#[derive(Component, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TankPresentationPass {
    Opaque(Box<str>),
    Water,
    Glass,
}

const WATER_COLOUR: Color = Color::srgba(0.12, 0.38, 0.55, 0.55);
const GLASS_COLOUR: Color = Color::srgba(0.75, 0.9, 1.0, 0.25);

/// Water and glass materials shared by every tank presentation.
#[derive(Default)]
pub(super) struct TankPresentationMaterials(Option<[Handle<StandardMaterial>; 2]>);

/// Rebuilds water and wall panels; opaque panels use their authored materials.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn present_changed_tank_water_and_glass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shared_materials: Local<TankPresentationMaterials>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    grid: Res<TopologyGrid>,
    asset_server: Res<AssetServer>,
    tanks: Query<
        (
            Entity,
            Ref<TankGeometry>,
            Ref<HabitatRegion>,
            &WorldMember,
            Option<&TankPresentations>,
        ),
        With<Tank>,
    >,
    fences: Query<(&FenceEdge, Option<&TankWallTopHeight>)>,
    changed_fences: Query<(), Or<(Changed<FenceEdge>, Changed<TankWallTopHeight>)>>,
    nodes: Query<&TopologyNode>,
    mut presentations: Query<(&mut TankPresentationGeometry, Option<&Children>)>,
    mut passes: Query<(
        &TankPresentationPass,
        &Mesh3d,
        &mut Transform,
        &mut Visibility,
    )>,
) {
    let definitions_changed = definitions.is_changed() || active_definitions.is_changed();
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let [water_material, glass_material] = shared_materials
        .0
        .get_or_insert_with(|| {
            [WATER_COLOUR, GLASS_COLOUR].map(|colour| {
                materials.add(StandardMaterial {
                    base_color: colour,
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })
            })
        })
        .clone();
    for (tank, geometry, region, member, roots) in &tanks {
        let previous_root = roots.and_then(|roots| roots.iter().next());
        let previous = previous_root.and_then(|root| presentations.get(root).ok());
        let next = TankPresentationGeometry {
            floor_height: geometry.floor_height,
            fallback_wall_height: geometry.wall_height,
        };
        let outline_changed = previous.is_none() || region.is_changed() || grid.is_changed();
        let walls_changed = outline_changed
            || definitions_changed
            || previous.is_some_and(|(current, _)| *current != next)
            || region
                .boundary_fence_entities
                .iter()
                .any(|fence| changed_fences.contains(*fence));
        if !walls_changed && !geometry.is_changed() {
            continue;
        }
        let mut existing = std::collections::BTreeMap::new();
        if let Some((_, Some(children))) = previous {
            for child in children.iter() {
                if let Ok((pass, ..)) = passes.get(child) {
                    existing.insert(pass.clone(), child);
                }
            }
        }
        let water = existing.remove(&TankPresentationPass::Water);
        if !walls_changed {
            if let Some((_, _, mut transform, mut visibility)) =
                water.and_then(|entity| passes.get_mut(entity).ok())
            {
                update_tank_water_pose(&geometry, &mut transform, &mut visibility);
            }
            continue;
        }
        let Some(walls) = region
            .boundary_fence_entities
            .iter()
            .map(|fence| {
                let (edge, top) = fences.get(*fence).ok()?;
                let [a, b] = [edge.a, edge.b].map(|node| nodes.get(node).map(|node| node.cell));
                let wall_material = definitions
                    .find_tank(edge.definition)?
                    .opaque_wall_material
                    .as_deref();
                Some((
                    [
                        grid.cell_translation(a.ok()?).xz(),
                        grid.cell_translation(b.ok()?).xz(),
                    ],
                    wall_material,
                    top.map_or(geometry.wall_height, |top| top.0),
                ))
            })
            .collect::<Option<Vec<_>>>()
        else {
            if let Some(root) = previous_root {
                commands.entity(root).try_despawn();
            }
            continue;
        };
        let outline = walls.iter().map(|(wall, ..)| *wall).collect::<Vec<_>>();
        let glass_walls = walls
            .iter()
            .filter(|(_, material, _)| material.is_none())
            .map(|(wall, _, top)| (*wall, *top))
            .collect::<Vec<_>>();
        let mut opaque_walls = std::collections::BTreeMap::<&str, Vec<([Vec2; 2], f32)>>::new();
        for (wall, material, top) in &walls {
            if let Some(material) = material {
                opaque_walls
                    .entry(material)
                    .or_default()
                    .push((*wall, *top));
            }
        }
        let root = if let Some((root, (mut current, _))) =
            previous_root.and_then(|root| presentations.get_mut(root).ok().map(|data| (root, data)))
        {
            current.set_if_neq(next);
            root
        } else {
            commands
                .spawn((
                    TankPresentationOf(tank),
                    next,
                    *member,
                    Transform::IDENTITY,
                    Visibility::Inherited,
                ))
                .id()
        };
        for (material, walls) in opaque_walls {
            let Some(mesh) = build_tank_wall_panel_mesh(&walls, *geometry) else {
                continue;
            };
            let key = TankPresentationPass::Opaque(material.into());
            if let Some((_, handle, ..)) = existing
                .remove(&key)
                .and_then(|entity| passes.get(entity).ok())
            {
                meshes
                    .insert(handle.id(), mesh)
                    .expect("live tank wall mesh");
                continue;
            }
            let material_id = AssetId::from_key(material);
            let handle = asset_server.load::<MaterialAsset>(material.to_owned());
            commands.spawn((
                key,
                Mesh3d(meshes.add(mesh)),
                GltfMaterialName(material_id.to_lowercase_hexadecimal_string()),
                ModelExpanded,
                PrefabMaterialOverrides(
                    vec![PrefabMaterialOverride {
                        material: material_id,
                        material_path: material.into(),
                        handle: Some(handle),
                    }]
                    .into_boxed_slice(),
                ),
                Transform::IDENTITY,
                Visibility::Inherited,
                ChildOf(root),
            ));
        }
        if let Some((entity, (_, handle, mut transform, mut visibility))) =
            water.and_then(|entity| passes.get_mut(entity).ok().map(|pass| (entity, pass)))
        {
            if outline_changed {
                if let Some(mesh) = build_water_surface_mesh(*grid, &outline) {
                    meshes
                        .insert(handle.id(), mesh)
                        .expect("live tank water mesh");
                } else {
                    commands.entity(entity).despawn();
                }
            }
            update_tank_water_pose(&geometry, &mut transform, &mut visibility);
        } else if let Some(mesh) = build_water_surface_mesh(*grid, &outline) {
            commands.spawn((
                TankPresentationPass::Water,
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(water_material.clone()),
                Transform::from_xyz(0.0, geometry.water_height, 0.0),
                tank_water_visibility(&geometry),
                ChildOf(root),
            ));
        }
        if let Some(mesh) = build_tank_wall_panel_mesh(&glass_walls, *geometry) {
            if let Some((_, handle, ..)) = existing
                .remove(&TankPresentationPass::Glass)
                .and_then(|entity| passes.get(entity).ok())
            {
                meshes
                    .insert(handle.id(), mesh)
                    .expect("live tank glass mesh");
            } else {
                commands.spawn((
                    TankPresentationPass::Glass,
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(glass_material.clone()),
                    Transform::IDENTITY,
                    ChildOf(root),
                ));
            }
        }
        for entity in existing.into_values() {
            commands.entity(entity).despawn();
        }
    }
}

fn tank_water_visibility(geometry: &TankGeometry) -> Visibility {
    if geometry.depth() > 0.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    }
}

fn update_tank_water_pose(
    geometry: &TankGeometry,
    transform: &mut Mut<Transform>,
    visibility: &mut Mut<Visibility>,
) {
    transform
        .reborrow()
        .map_unchanged(|transform| &mut transform.translation.y)
        .set_if_neq(geometry.water_height);
    visibility.set_if_neq(tank_water_visibility(geometry));
}

pub(super) fn retire_tank_presentation_on_demotion(
    removed: On<Remove<Tank>>,
    mut commands: Commands,
    roots: Query<&TankPresentations>,
) {
    if let Ok(roots) = roots.get(removed.entity) {
        for root in roots.iter() {
            commands.entity(root).try_despawn();
        }
    }
}

fn triangles_mesh(triangles: Vec<[Vec3; 3]>) -> Option<Mesh> {
    if triangles.is_empty() {
        return None;
    }
    let normals = triangles
        .iter()
        .flat_map(|[a, b, c]| [(*b - *a).cross(*c - *a).normalize_or_zero().to_array(); 3])
        .collect::<Vec<_>>();
    let positions = triangles
        .into_iter()
        .flat_map(|triangle| triangle.map(|corner| corner.to_array()))
        .collect::<Vec<_>>();
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals),
    )
}

/// Even-odd test against the unordered wall edges.
pub(super) fn inside_walls(point: Vec2, walls: &[[Vec2; 2]]) -> bool {
    walls
        .iter()
        .filter(|[a, b]| {
            (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
        })
        .count()
        % 2
        == 1
}

fn build_water_surface_mesh(grid: TopologyGrid, walls: &[[Vec2; 2]]) -> Option<Mesh> {
    if walls.is_empty() {
        return None;
    }
    let (minimum, maximum) = walls.iter().flatten().fold(
        (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)),
        |(minimum, maximum), point| (minimum.min(*point), maximum.max(*point)),
    );
    let first = ((minimum - grid.origin) / grid.spacing_m)
        .round()
        .as_ivec2();
    let last = ((maximum - grid.origin) / grid.spacing_m)
        .round()
        .as_ivec2();
    // Walls run along cell sides or diagonals, so each half of a diagonal
    // cut holds two whole pinwheel triangles, whose centroids are never on a wall.
    let triangles = (first.y..last.y)
        .flat_map(|y| (first.x..last.x).map(move |x| IVec2::new(x, y)))
        .flat_map(|cell| {
            let minimum = grid.cell_translation(cell.extend(0)).xz();
            let corners = [Vec2::ZERO, Vec2::Y, Vec2::ONE, Vec2::X]
                .map(|corner| minimum + corner * grid.spacing_m);
            let centre = minimum + Vec2::splat(grid.spacing_m * 0.5);
            (0..4).filter_map(move |side| {
                let triangle = [corners[side], corners[(side + 1) % 4], centre];
                let centroid = (triangle[0] + triangle[1] + triangle[2]) / 3.0;
                inside_walls(centroid, walls)
                    .then(|| triangle.map(|corner| Vec3::new(corner.x, 0.0, corner.y)))
            })
        })
        .collect();
    triangles_mesh(triangles)
}

fn build_tank_wall_panel_mesh(walls: &[([Vec2; 2], f32)], geometry: TankGeometry) -> Option<Mesh> {
    let bottom = geometry.floor_height;
    let triangles = walls
        .iter()
        .flat_map(|([a, b], top)| {
            let [a0, b0, b1, a1] = [
                Vec3::new(a.x, bottom, a.y),
                Vec3::new(b.x, bottom, b.y),
                Vec3::new(b.x, *top, b.y),
                Vec3::new(a.x, *top, a.y),
            ];
            [[a0, b0, b1], [a0, b1, a1]]
        })
        .collect();
    let uv = walls
        .iter()
        .flat_map(|([a, b], top)| {
            let length = a.distance(*b);
            let height = *top - bottom;
            [
                [0.0, 0.0],
                [length, 0.0],
                [length, height],
                [0.0, 0.0],
                [length, height],
                [0.0, height],
            ]
        })
        .collect::<Vec<_>>();
    triangles_mesh(triangles).map(|mesh| {
        mesh.with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv.clone())
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, uv)
    })
}
