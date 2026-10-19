use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;

use openzt2_world_state::world_spawn::world_membership_types::WorldMember;
use openzt2_zoo_state::habitat::habitat_types::HabitatRegion;
use openzt2_zoo_state::topology::topology_graph_types::{FenceEdge, TopologyGrid, TopologyNode};

use super::aquatic_simulation_types::{Tank, TankGeometry};

/// The water surface and glass panes drawn for one tank.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TankPresentationOf(pub Entity);

// ponytail: flat colours; the original's tank water is terrain water with an
// authored effect, which arrives with the terrain-deformation tank tools.
const WATER_COLOUR: Color = Color::srgba(0.12, 0.38, 0.55, 0.55);
const GLASS_COLOUR: Color = Color::srgba(0.75, 0.9, 1.0, 0.25);

#[allow(clippy::type_complexity)]
pub(super) fn present_changed_tank_water_and_glass(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    grid: Res<TopologyGrid>,
    tanks: Query<
        (Entity, &TankGeometry, &HabitatRegion, &WorldMember),
        (
            With<Tank>,
            Or<(Changed<TankGeometry>, Changed<HabitatRegion>)>,
        ),
    >,
    fences: Query<&FenceEdge>,
    nodes: Query<&TopologyNode>,
    presentations: Query<(Entity, &TankPresentationOf)>,
) {
    for (tank, geometry, region, member) in &tanks {
        for (presentation, of) in &presentations {
            if of.0 == tank {
                commands.entity(presentation).despawn();
            }
        }
        let walls = region
            .boundary_fence_entities
            .iter()
            .filter_map(|fence| {
                let edge = fences.get(*fence).ok()?;
                Some([edge.a, edge.b].map(|node| nodes.get(node).map(|node| node.cell)))
            })
            .map(|[a, b]| {
                Some([
                    grid.cell_translation(a.ok()?).xz(),
                    grid.cell_translation(b.ok()?).xz(),
                ])
            })
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default();
        let water = build_water_surface_mesh(*grid, &walls, *geometry);
        let glass = build_glass_mesh(&walls, *geometry);
        let root = commands
            .spawn((
                TankPresentationOf(tank),
                *member,
                Transform::IDENTITY,
                Visibility::Inherited,
            ))
            .id();
        for (mesh, colour) in [(water, WATER_COLOUR), (glass, GLASS_COLOUR)] {
            let Some(mesh) = mesh else {
                continue;
            };
            commands.spawn((
                Mesh3d(meshes.add(mesh)),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: colour,
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::IDENTITY,
                ChildOf(root),
            ));
        }
    }
}

/// Removes presentations whose tank was removed or demoted.
pub(super) fn remove_orphaned_tank_presentations(
    mut commands: Commands,
    presentations: Query<(Entity, &TankPresentationOf)>,
    tanks: Query<(), With<Tank>>,
) {
    for (presentation, of) in &presentations {
        if tanks.get(of.0).is_err() {
            commands.entity(presentation).despawn();
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
fn inside_walls(point: Vec2, walls: &[[Vec2; 2]]) -> bool {
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

fn build_water_surface_mesh(
    grid: TopologyGrid,
    walls: &[[Vec2; 2]],
    geometry: TankGeometry,
) -> Option<Mesh> {
    if geometry.depth() <= 0.0 || walls.is_empty() {
        return None;
    }
    let y = geometry.water_height;
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
                    .then(|| triangle.map(|corner| Vec3::new(corner.x, y, corner.y)))
            })
        })
        .collect();
    triangles_mesh(triangles)
}

fn build_glass_mesh(walls: &[[Vec2; 2]], geometry: TankGeometry) -> Option<Mesh> {
    let (bottom, top) = (geometry.floor_height, geometry.wall_height);
    let triangles = walls
        .iter()
        .flat_map(|[a, b]| {
            let [a0, b0, b1, a1] = [
                Vec3::new(a.x, bottom, a.y),
                Vec3::new(b.x, bottom, b.y),
                Vec3::new(b.x, top, b.y),
                Vec3::new(a.x, top, a.y),
            ];
            [[a0, b0, b1], [a0, b1, a1]]
        })
        .collect();
    triangles_mesh(triangles)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEOMETRY: TankGeometry = TankGeometry {
        floor_height: -3.0,
        wall_height: 6.0,
        water_height: 5.0,
        area: 2.0,
        volume: 16.0,
    };

    fn square_walls(side: f32) -> Vec<[Vec2; 2]> {
        let corners = [
            Vec2::ZERO,
            Vec2::new(side, 0.0),
            Vec2::splat(side),
            Vec2::new(0.0, side),
        ];
        (0..4).map(|i| [corners[i], corners[(i + 1) % 4]]).collect()
    }

    fn vertex_count(mesh: &Mesh) -> usize {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().len()
    }

    #[test]
    fn water_fills_the_area_inside_the_walls() {
        let mesh = build_water_surface_mesh(TopologyGrid::default(), &square_walls(2.0), GEOMETRY)
            .expect("water");
        // Four whole cells, four triangles each.
        assert_eq!(vertex_count(&mesh), 4 * 4 * 3);
    }

    #[test]
    fn a_diagonal_wall_keeps_half_of_the_cell_it_cuts() {
        let walls = vec![
            [Vec2::ZERO, Vec2::new(1.0, 0.0)],
            [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)],
            [Vec2::new(0.0, 1.0), Vec2::ZERO],
        ];
        let mesh =
            build_water_surface_mesh(TopologyGrid::default(), &walls, GEOMETRY).expect("water");
        assert_eq!(vertex_count(&mesh), 2 * 3);
    }

    #[test]
    fn an_empty_tank_has_no_water_and_glass_spans_floor_to_rim() {
        let empty = TankGeometry {
            water_height: -3.0,
            volume: 0.0,
            ..GEOMETRY
        };
        let walls = square_walls(3.0);
        assert!(build_water_surface_mesh(TopologyGrid::default(), &walls, empty).is_none());
        let glass = build_glass_mesh(&walls[..1], GEOMETRY).expect("glass");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            glass.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        assert_eq!(positions[0], [0.0, -3.0, 0.0]);
        assert_eq!(positions[2], [3.0, 6.0, 0.0]);
    }
}
