use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use super::locomotion_types::NavFlags;
use super::terrain_derived_navigation_graph_types::{
    NavigationPathGeometry, NavigationPathSurface, NavigationPathSurfaceChanges, PathEdgeKey,
    TerrainDerivedNavigationGraph, TerrainDerivedNavigationGraphEdge,
    TerrainDerivedNavigationGraphNode,
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
        let half = width_m * 0.5;
        let corners = [(-half, -half), (half, -half), (half, half), (-half, half)]
            .map(|(x, z)| Vec3::new(x, 0.0, z));
        let corners = std::array::from_fn(|index| {
            transform.transform_point(corners[index].with_y(f32::from(height_cm[index]) * 0.01))
        });
        let centre = transform.transform_point(Vec3::Y * (f32::from(height_cm[4]) * 0.01));
        let minimum_triangle_normal_y = (0..4)
            .map(|index| {
                (corners[index] - centre)
                    .cross(corners[(index + 1) % 4] - centre)
                    .normalize_or_zero()
                    .y
                    .abs()
            })
            .fold(1.0, f32::min);
        Some(Self {
            entity,
            centre,
            corners,
            node: TerrainDerivedNavigationGraphNode {
                height_centimetres: (centre.y * 100.0).round() as i32,
                minimum_triangle_normal_y,
                clearance_centimetres: (width_m * scale.x.abs().min(scale.z.abs()) * 50.0)
                    .min(f32::from(u16::MAX)) as u16,
                water_depth_centimetres: 0,
                eligibility_flags: NavFlags(NavFlags::ELIGIBILITY_BITS | NavFlags::PATH.0),
            },
        })
    }

    fn edge_keys(&self) -> impl Iterator<Item = PathEdgeKey> + '_ {
        (0..4).map(|side| path_edge_key(self.corners[side], self.corners[(side + 1) % 4]))
    }

    pub fn height_at(&self, world: Vec2) -> Option<f32> {
        let corners = self.corners;
        let centre = self.centre;
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
        let mut graph = Self {
            terrain_neighbors: Box::default(),
            terrain_centres_xz: Box::default(),
            terrain_cell_size_centimetres,
            terrain_cells_per_tile,
            terrain_tile_size_centimetres: u32::from(terrain_cell_size_centimetres)
                * u32::from(terrain_cells_per_tile),
            terrain_tile_grid_dimensions,
            navigation_nodes,
            path_surface_edges: Default::default(),
            path_surfaces: Vec::new(),
            available_path_slots: Default::default(),
            elevated_path_nodes: Default::default(),
            path_surfaces_by_cell: Default::default(),
            path_nodes_by_edge: Default::default(),
            ground_cells_by_path: Default::default(),
            ground_path_count_by_node: Default::default(),
        };
        let mut neighbors = Vec::with_capacity(graph.navigation_nodes.len());
        let mut centres = Vec::with_capacity(graph.navigation_nodes.len());
        let size = i64::from(terrain_cell_size_centimetres);
        for node in 0..graph.navigation_nodes.len() as u32 {
            let [x, z] = graph
                .calculate_terrain_cell_for_navigation_node(node)
                .expect("terrain cell");
            neighbors.push(
                [
                    [x.wrapping_sub(1), z],
                    [x + 1, z],
                    [x, z.wrapping_sub(1)],
                    [x, z + 1],
                ]
                .map(|cell| {
                    graph
                        .calculate_navigation_node_for_terrain_cell(cell)
                        .unwrap_or(u32::MAX)
                }),
            );
            let coordinate =
                |cell| i32::try_from(i64::from(cell) * size + size / 2).unwrap_or(i32::MIN);
            centres.push([coordinate(x), coordinate(z).saturating_neg()]);
        }
        graph.terrain_neighbors = neighbors.into_boxed_slice();
        graph.terrain_centres_xz = centres.into_boxed_slice();
        graph
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
            .get((node as usize).checked_sub(self.navigation_nodes.len())?)?
            .as_ref()
    }

    pub fn node_supports_world_position(&self, node: u32, position: Vec3) -> bool {
        if let Some(surface) = self.path_surface_at_node(node) {
            surface.height_at(position.xz()).is_some()
        } else {
            self.calculate_terrain_cell_for_navigation_node(node)
                .is_some_and(|cell| self.terrain_cell_at_world_position(position) == Some(cell))
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
        for &node in self
            .path_surfaces_by_cell
            .get(&cell)
            .into_iter()
            .flat_map(BTreeMap::values)
        {
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
                self.iterate_outgoing_navigation_graph_edges(from, None)
                    .flat_map(|edge| {
                        std::iter::once(edge.destination_node).chain(
                            self.iterate_outgoing_navigation_graph_edges(
                                edge.destination_node,
                                None,
                            )
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
                (
                    (height - position.y).abs().to_bits(),
                    *node != from,
                    self.path_node_order(*node),
                )
            })
    }
}

fn path_edge_key(a: Vec3, b: Vec3) -> PathEdgeKey {
    let a = a.to_array().map(|value| (value * 100.0).round() as i32);
    let b = b.to_array().map(|value| (value * 100.0).round() as i32);
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

impl TerrainDerivedNavigationGraph {
    /// Apply a batch of changed projections and terrain cells. A missing
    /// projection retires the path. Nodes remain stable while a path remains elevated.
    pub fn update_navigation_path_surfaces<'a>(
        &mut self,
        updates: BTreeMap<Entity, Option<&'a NavigationPathGeometry>>,
        changed_terrain_cells: impl IntoIterator<Item = [u32; 2]>,
        terrain_height: impl Fn(Vec2) -> Option<f32>,
    ) -> NavigationPathSurfaceChanges {
        let mut changes = NavigationPathSurfaceChanges::default();
        let mut ground_candidates = BTreeSet::new();
        let mut elevated_candidates = BTreeSet::new();

        for (entity, geometry) in updates {
            if let Some(nodes) = self.ground_cells_by_path.remove(&entity) {
                for &node in &nodes {
                    ground_candidates.insert(node);
                    if let std::collections::btree_map::Entry::Occupied(mut entry) =
                        self.ground_path_count_by_node.entry(node)
                    {
                        *entry.get_mut() -= 1;
                        if *entry.get() == 0 {
                            entry.remove();
                        }
                    }
                }
            }
            let previous_node = self.elevated_path_nodes.remove(&entity);
            if let Some(node) = previous_node {
                let slot = node as usize - self.navigation_nodes.len();
                if let Some(old) = self.path_surfaces[slot].take() {
                    self.unlink_elevated_path(node, &old, &mut elevated_candidates);
                }
                changes.elevated_nodes.insert(node);
                elevated_candidates.insert(node);
                if !matches!(geometry, Some(NavigationPathGeometry::Elevated(_))) {
                    self.available_path_slots.insert(slot);
                }
            }
            match geometry {
                Some(NavigationPathGeometry::Ground(surface)) => {
                    let nodes = self.ground_nodes_beneath_path(surface);
                    for &node in &nodes {
                        ground_candidates.insert(node);
                        *self.ground_path_count_by_node.entry(node).or_default() += 1;
                    }
                    self.ground_cells_by_path.insert(entity, nodes);
                }
                Some(NavigationPathGeometry::Elevated(surface)) => {
                    let Some(node) = previous_node.or_else(|| self.reserve_path_slot()) else {
                        continue;
                    };
                    self.link_elevated_path(node, surface, &mut elevated_candidates);
                    changes.elevated_nodes.insert(node);
                    elevated_candidates.insert(node);
                }
                None => {}
            }
        }
        for node in ground_candidates {
            let flags = &mut self.navigation_nodes[node as usize].eligibility_flags.0;
            let previous = *flags;
            if self.ground_path_count_by_node.contains_key(&node) {
                *flags |= NavFlags::PATH.0;
            } else {
                *flags &= !NavFlags::PATH.0;
            }
            if *flags != previous {
                changes.ground_nodes.insert(node);
            }
        }

        for [x, z] in changed_terrain_cells {
            // The contact test samples a side and a point just beyond it.
            for cell_x in x.saturating_sub(1)..=x.saturating_add(1) {
                for cell_z in z.saturating_sub(1)..=z.saturating_add(1) {
                    if let Some(paths) = self.path_surfaces_by_cell.get(&[cell_x, cell_z]) {
                        elevated_candidates.extend(paths.values().copied());
                    }
                }
            }
        }
        for node in elevated_candidates {
            if self.refresh_path_edges(node, &terrain_height) {
                changes.elevated_nodes.insert(node);
            }
        }
        changes
    }

    fn reserve_path_slot(&mut self) -> Option<u32> {
        let slot = self
            .available_path_slots
            .first()
            .copied()
            .unwrap_or(self.path_surfaces.len());
        let node = u32::try_from(self.navigation_nodes.len().checked_add(slot)?).ok()?;
        if slot == self.path_surfaces.len() {
            self.path_surfaces.push(None);
        } else {
            self.available_path_slots.remove(&slot);
        }
        Some(node)
    }

    fn path_cells(&self, surface: &NavigationPathSurface) -> impl Iterator<Item = [u32; 2]> {
        let size = f32::from(self.terrain_cell_size_centimetres) * 0.01;
        let corners = surface.corners.map(|point| Vec2::new(point.x, -point.z));
        let lower = corners
            .into_iter()
            .fold(Vec2::splat(f32::INFINITY), Vec2::min);
        let upper = corners
            .into_iter()
            .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max);
        let minimum = (lower / size).floor().as_ivec2().max(IVec2::ZERO);
        let dimensions = UVec2::from_array(self.terrain_tile_grid_dimensions)
            * u32::from(self.terrain_cells_per_tile);
        let maximum = (upper / size)
            .floor()
            .as_ivec2()
            .min(dimensions.as_ivec2() - IVec2::ONE);
        (minimum.y..=maximum.y)
            .flat_map(move |z| (minimum.x..=maximum.x).map(move |x| [x as u32, z as u32]))
    }

    fn ground_nodes_beneath_path(&self, surface: &NavigationPathSurface) -> Box<[u32]> {
        let size = f32::from(self.terrain_cell_size_centimetres) * 0.01;
        self.path_cells(surface)
            .filter_map(|[x, z]| {
                let centre = Vec2::new((x as f32 + 0.5) * size, -(z as f32 + 0.5) * size);
                surface.height_at(centre)?;
                self.calculate_navigation_node_for_terrain_cell([x, z])
            })
            .collect()
    }

    fn unlink_elevated_path(
        &mut self,
        node: u32,
        surface: &NavigationPathSurface,
        affected: &mut BTreeSet<u32>,
    ) {
        for cell in self.path_cells(surface) {
            if let std::collections::btree_map::Entry::Occupied(mut entry) =
                self.path_surfaces_by_cell.entry(cell)
            {
                entry.get_mut().remove(&surface.entity);
                if entry.get().is_empty() {
                    entry.remove();
                }
            }
        }
        for key in surface.edge_keys() {
            if let std::collections::btree_map::Entry::Occupied(mut entry) =
                self.path_nodes_by_edge.entry(key)
            {
                entry.get_mut().remove(&node);
                affected.extend(entry.get().iter().copied());
                if entry.get().is_empty() {
                    entry.remove();
                }
            }
        }
    }

    fn link_elevated_path(
        &mut self,
        node: u32,
        surface: &NavigationPathSurface,
        affected: &mut BTreeSet<u32>,
    ) {
        for cell in self.path_cells(surface) {
            self.path_surfaces_by_cell
                .entry(cell)
                .or_default()
                .insert(surface.entity, node);
        }
        for key in surface.edge_keys() {
            let adjacent = self.path_nodes_by_edge.entry(key).or_default();
            affected.extend(adjacent.iter().copied());
            adjacent.insert(node);
        }
        self.elevated_path_nodes.insert(surface.entity, node);
        self.path_surfaces[node as usize - self.navigation_nodes.len()] = Some(surface.clone());
    }

    fn refresh_path_edges(
        &mut self,
        node: u32,
        terrain_height: &impl Fn(Vec2) -> Option<f32>,
    ) -> bool {
        let previous = self.path_surface_edges.remove(&node).unwrap_or_default();
        let mut next = self.connected_path_edges(node, terrain_height);
        self.order_path_edges(&mut next);
        let changed = previous != next;
        for edge in previous {
            if (edge.destination_node as usize) < self.navigation_nodes.len() {
                if let std::collections::btree_map::Entry::Occupied(mut entry) =
                    self.path_surface_edges.entry(edge.destination_node)
                {
                    entry.get_mut().retain(|edge| edge.destination_node != node);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
            }
        }
        for edge in &next {
            let from = edge.destination_node;
            if let Some(terrain) = self.navigation_nodes.get(from as usize) {
                if let Some(reverse) = self.derive_navigation_graph_edge(from, *terrain, node, None)
                {
                    let mut outgoing = self.path_surface_edges.remove(&from).unwrap_or_default();
                    outgoing.push(reverse);
                    self.order_path_edges(&mut outgoing);
                    self.path_surface_edges.insert(from, outgoing);
                }
            }
        }
        if !next.is_empty() {
            self.path_surface_edges.insert(node, next);
        }
        changed
    }

    fn order_path_edges(&self, edges: &mut [TerrainDerivedNavigationGraphEdge]) {
        edges.sort_unstable_by_key(|edge| self.path_node_order(edge.destination_node));
    }

    fn path_node_order(&self, node: u32) -> (Option<Entity>, u32) {
        match self.path_surface_at_node(node) {
            Some(surface) => (Some(surface.entity), 0),
            None => (None, node),
        }
    }

    fn connected_path_edges(
        &self,
        node: u32,
        terrain_height: &impl Fn(Vec2) -> Option<f32>,
    ) -> Vec<TerrainDerivedNavigationGraphEdge> {
        let Some(surface) = self.path_surface_at_node(node) else {
            return Vec::new();
        };
        let mut destinations = BTreeSet::new();
        for side in 0..4 {
            let a = surface.corners[side];
            let b = surface.corners[(side + 1) % 4];
            let adjoining = &self.path_nodes_by_edge[&path_edge_key(a, b)];
            if adjoining.len() > 1 {
                destinations.extend(adjoining.iter().copied().filter(|other| *other != node));
                continue;
            }
            if ![a, (a + b) * 0.5, b].into_iter().all(|point| {
                terrain_height(point.xz()).is_some_and(|height| (height - point.y).abs() <= 0.01)
            }) {
                continue;
            }
            let edge = (a + b) * 0.5;
            let outward = (edge.xz() - surface.centre.xz()).normalize_or_zero();
            let size = f32::from(self.terrain_cell_size_centimetres) * 0.01;
            let count = ((a.xz().distance(b.xz()) / size).ceil() as u32).max(1);
            for part in 0..count {
                let outside = a.lerp(b, (part as f32 + 0.5) / count as f32)
                    + Vec3::new(outward.x, 0.0, outward.y) * 0.01;
                destinations.extend(
                    self.terrain_cell_at_world_position(outside)
                        .and_then(|cell| self.calculate_navigation_node_for_terrain_cell(cell)),
                );
            }
        }
        destinations
            .into_iter()
            .filter_map(|to| self.derive_navigation_graph_edge(node, surface.node, to, None))
            .collect()
    }
}
