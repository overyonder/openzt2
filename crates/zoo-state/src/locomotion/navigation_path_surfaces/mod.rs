use bevy::prelude::*;

use super::locomotion_types::NavFlags;
use super::terrain_derived_navigation_graph_types::{
    NavigationPathSurface, TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphNode,
};

impl NavigationPathSurface {
    pub fn new(
        entity: Entity,
        transform: GlobalTransform,
        width_m: f32,
        height_cm: [i16; 5],
    ) -> Option<Self> {
        if !width_m.is_finite() || width_m <= 0.0 || !transform.affine().is_finite() {
            return None;
        }
        let (scale, _, _) = transform.to_scale_rotation_translation();
        if scale.abs().min_element() <= f32::EPSILON {
            return None;
        }
        let mut surface = Self {
            entity,
            transform,
            width_m,
            height_cm,
            node: TerrainDerivedNavigationGraphNode {
                height_centimetres: 0,
                minimum_triangle_normal_y: 1.0,
                clearance_centimetres: (width_m * scale.x.abs().min(scale.z.abs()) * 50.0)
                    .min(f32::from(u16::MAX)) as u16,
                water_depth_centimetres: 0,
                eligibility_flags: NavFlags(NavFlags::ELIGIBILITY_BITS | NavFlags::PATH.0),
            },
        };
        let corners = surface.corners();
        let centre = surface.centre();
        surface.node.height_centimetres = (centre.y * 100.0).round() as i32;
        surface.node.minimum_triangle_normal_y = (0..4)
            .map(|index| {
                (corners[index] - centre)
                    .cross(corners[(index + 1) % 4] - centre)
                    .normalize_or_zero()
                    .y
                    .abs()
            })
            .fold(1.0, f32::min);
        Some(surface)
    }

    pub(super) fn centre(&self) -> Vec3 {
        self.transform
            .transform_point(Vec3::Y * (f32::from(self.height_cm[4]) * 0.01))
    }

    fn corners(&self) -> [Vec3; 4] {
        let half = self.width_m * 0.5;
        let corners = [(-half, -half), (half, -half), (half, half), (-half, half)];
        std::array::from_fn(|index| {
            let (x, z) = corners[index];
            self.transform
                .transform_point(Vec3::new(x, f32::from(self.height_cm[index]) * 0.01, z))
        })
    }

    pub fn height_at(&self, world: Vec2) -> Option<f32> {
        let corners = self.corners();
        let centre = self.centre();
        for index in 0..4 {
            let a = corners[index];
            let b = corners[(index + 1) % 4];
            let ab = b.xz() - a.xz();
            let ac = centre.xz() - a.xz();
            let ap = world - a.xz();
            let determinant = ab.perp_dot(ac);
            if determinant.abs() < f32::EPSILON {
                continue;
            }
            let along_edge = ap.perp_dot(ac) / determinant;
            let toward_centre = ab.perp_dot(ap) / determinant;
            if along_edge >= -0.00001
                && toward_centre >= -0.00001
                && along_edge + toward_centre <= 1.00001
            {
                return Some(a.y + along_edge * (b.y - a.y) + toward_centre * (centre.y - a.y));
            }
        }
        None
    }
}

impl TerrainDerivedNavigationGraph {
    pub fn from_terrain_nodes(
        terrain_cell_size_centimetres: u16,
        terrain_cells_per_tile: u16,
        terrain_tile_grid_dimensions: [u32; 2],
        navigation_nodes: Box<[TerrainDerivedNavigationGraphNode]>,
    ) -> Self {
        Self {
            terrain_cell_size_centimetres,
            terrain_cells_per_tile,
            terrain_tile_size_centimetres: u32::from(terrain_cell_size_centimetres)
                * u32::from(terrain_cells_per_tile),
            terrain_tile_grid_dimensions,
            navigation_nodes,
            path_surfaces: Vec::new(),
            path_surface_edges: Default::default(),
            path_surfaces_by_cell: Default::default(),
            ground_path_nodes: Vec::new(),
        }
    }

    pub fn terrain_cell_at_world_position(&self, position: Vec3) -> Option<[u32; 2]> {
        if !position.is_finite()
            || position.x < 0.0
            || position.z > 0.0
            || self.terrain_cell_size_centimetres == 0
        {
            return None;
        }
        let size = f32::from(self.terrain_cell_size_centimetres) * 0.01;
        let cell = [
            (position.x / size).floor() as u32,
            (-position.z / size).floor() as u32,
        ];
        self.calculate_navigation_node_for_terrain_cell(cell)
            .map(|_| cell)
    }

    pub fn path_surface_at_node(&self, node: u32) -> Option<&NavigationPathSurface> {
        self.path_surfaces
            .get((node as usize).checked_sub(self.navigation_nodes.len())?)
    }

    pub fn node_supports_world_position(&self, node: u32, position: Vec3) -> bool {
        if let Some(surface) = self.path_surface_at_node(node) {
            surface.height_at(position.xz()).is_some()
        } else {
            self.terrain_cell_at_world_position(position)
                == self.calculate_terrain_cell_for_navigation_node(node)
        }
    }

    pub fn navigation_surface_at_world_position(&self, position: Vec3) -> Option<u32> {
        let cell = self.terrain_cell_at_world_position(position)?;
        let terrain_node = self.calculate_navigation_node_for_terrain_cell(cell)?;
        let mut selected = terrain_node;
        let mut separation = (self.find_navigation_node(terrain_node)?.height_centimetres as f32
            * 0.01
            - position.y)
            .abs();
        for &node in self.path_surfaces_by_cell.get(&cell).into_iter().flatten() {
            let surface = self.path_surface_at_node(node)?;
            let Some(height) = surface.height_at(position.xz()) else {
                continue;
            };
            let candidate_separation = (height - position.y).abs();
            if candidate_separation < separation {
                selected = node;
                separation = candidate_separation;
            }
        }
        Some(selected)
    }

    /// Keeps a swept line on its current sheet. It can enter another sheet only
    /// through the same connected boundary that route planning uses.
    pub fn continue_navigation_surface(&self, from: u32, position: Vec3) -> Option<u32> {
        std::iter::once(from)
            .chain(
                self.iterate_outgoing_navigation_graph_edges(from)
                    .flat_map(|edge| {
                        std::iter::once(edge.destination_node).chain(
                            self.iterate_outgoing_navigation_graph_edges(edge.destination_node)
                                .map(|edge| edge.destination_node),
                        )
                    }),
            )
            .filter(|node| self.node_supports_world_position(*node, position))
            .min_by_key(|node| {
                let height = self
                    .path_surface_at_node(*node)
                    .and_then(|surface| surface.height_at(position.xz()))
                    .unwrap_or_else(|| {
                        self.find_navigation_node(*node)
                            .map_or(f32::INFINITY, |node| node.height_centimetres as f32 * 0.01)
                    });
                ((height - position.y).abs().to_bits(), *node != from, *node)
            })
    }

    pub fn replace_navigation_path_surfaces(
        &mut self,
        mut elevated: Vec<NavigationPathSurface>,
        ground_paths: Vec<NavigationPathSurface>,
        terrain_height: impl Fn(Vec2) -> Option<f32>,
    ) -> bool {
        let previous_ground = self.ground_path_nodes.clone();
        for node in self.ground_path_nodes.drain(..) {
            self.navigation_nodes[node as usize].eligibility_flags.0 &= !NavFlags::PATH.0;
        }
        let size = f32::from(self.terrain_cell_size_centimetres) * 0.01;
        for path in ground_paths {
            let corners = path.corners();
            let minimum = corners
                .iter()
                .map(|point| Vec2::new(point.x, -point.z))
                .reduce(Vec2::min)
                .expect("four corners");
            let maximum = corners
                .iter()
                .map(|point| Vec2::new(point.x, -point.z))
                .reduce(Vec2::max)
                .expect("four corners");
            let minimum = (minimum / size).floor().as_ivec2().max(IVec2::ZERO);
            let maximum = (maximum / size).floor().as_ivec2();
            for z in minimum.y..=maximum.y {
                for x in minimum.x..=maximum.x {
                    let centre = Vec2::new((x as f32 + 0.5) * size, -(z as f32 + 0.5) * size);
                    if path.height_at(centre).is_none() {
                        continue;
                    }
                    if let Some(node) =
                        self.calculate_navigation_node_for_terrain_cell([x as u32, z as u32])
                    {
                        self.navigation_nodes[node as usize].eligibility_flags.0 |=
                            NavFlags::PATH.0;
                        self.ground_path_nodes.push(node);
                    }
                }
            }
        }
        self.ground_path_nodes.sort_unstable();
        self.ground_path_nodes.dedup();
        elevated.sort_unstable_by_key(|surface| surface.entity);
        let previous_surfaces = std::mem::replace(&mut self.path_surfaces, elevated);
        let previous_edges = std::mem::take(&mut self.path_surface_edges);
        self.path_surfaces_by_cell.clear();
        let terrain_count = self.navigation_nodes.len() as u32;
        let mut edge_owners = std::collections::BTreeMap::<([i32; 3], [i32; 3]), Vec<u32>>::new();
        for (index, surface) in self.path_surfaces.iter().enumerate() {
            let node = terrain_count + index as u32;
            let corners = surface.corners();
            let minimum = corners
                .iter()
                .map(|point| Vec2::new(point.x, -point.z))
                .reduce(Vec2::min)
                .expect("four corners");
            let maximum = corners
                .iter()
                .map(|point| Vec2::new(point.x, -point.z))
                .reduce(Vec2::max)
                .expect("four corners");
            let minimum = (minimum / size).floor().as_ivec2().max(IVec2::ZERO);
            let maximum = (maximum / size).floor().as_ivec2();
            for z in minimum.y..=maximum.y {
                for x in minimum.x..=maximum.x {
                    let cell = [x as u32, z as u32];
                    if self
                        .calculate_navigation_node_for_terrain_cell(cell)
                        .is_some()
                    {
                        self.path_surfaces_by_cell
                            .entry(cell)
                            .or_default()
                            .push(node);
                    }
                }
            }
            for side in 0..4 {
                edge_owners
                    .entry(path_edge_key(corners[side], corners[(side + 1) % 4]))
                    .or_default()
                    .push(node);
            }
        }
        let mut connections = Vec::new();
        for (index, surface) in self.path_surfaces.iter().enumerate() {
            let node = terrain_count + index as u32;
            let corners = surface.corners();
            for side in 0..4 {
                let a = corners[side];
                let b = corners[(side + 1) % 4];
                let mut joins_path = false;
                for &other_node in &edge_owners[&path_edge_key(a, b)] {
                    if other_node == node {
                        continue;
                    }
                    connections.push((node, other_node));
                    joins_path = true;
                }
                if joins_path {
                    continue;
                }
                if ![a, (a + b) * 0.5, b].into_iter().all(|point| {
                    terrain_height(point.xz())
                        .is_some_and(|height| (height - point.y).abs() <= 0.01)
                }) {
                    continue;
                }
                let edge = (a + b) * 0.5;
                let outward = (edge.xz() - surface.centre().xz()).normalize_or_zero();
                let count = ((a.xz().distance(b.xz())
                    / (f32::from(self.terrain_cell_size_centimetres) * 0.01))
                    .ceil() as u32)
                    .max(1);
                for part in 0..count {
                    let outside = a.lerp(b, (part as f32 + 0.5) / count as f32)
                        + Vec3::new(outward.x, 0.0, outward.y) * 0.01;
                    if let Some(terrain) = self
                        .terrain_cell_at_world_position(outside)
                        .and_then(|cell| self.calculate_navigation_node_for_terrain_cell(cell))
                    {
                        connections.extend([(node, terrain), (terrain, node)]);
                    }
                }
            }
        }
        connections.sort_unstable();
        connections.dedup();
        for (from, to) in connections {
            if let Some(edge) = self.derive_navigation_graph_edge(from, to) {
                self.path_surface_edges.entry(from).or_default().push(edge);
            }
        }
        previous_surfaces != self.path_surfaces
            || previous_edges != self.path_surface_edges
            || previous_ground != self.ground_path_nodes
    }
}

fn path_edge_key(a: Vec3, b: Vec3) -> ([i32; 3], [i32; 3]) {
    let a = a.to_array().map(|value| (value * 100.0).round() as i32);
    let b = b.to_array().map(|value| (value * 100.0).round() as i32);
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}
