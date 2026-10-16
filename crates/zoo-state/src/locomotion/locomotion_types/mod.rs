use bevy::platform::collections::HashMap;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct NavFlags(pub u16);

impl NavFlags {
    pub const GUEST: Self = Self(1 << 0);
    pub const STAFF: Self = Self(1 << 1);
    pub const ANIMAL: Self = Self(1 << 2);
    pub const VEHICLE: Self = Self(1 << 3);
    pub const ONE_WAY: Self = Self(1 << 4);
    pub const DISABLED_BY_DEFAULT: Self = Self(1 << 5);
    pub const WATER: Self = Self(1 << 6);
    pub const PATH: Self = Self(1 << 7);
    /// An animal whose authored locomotion permits both land and surface water.
    pub const AMPHIBIOUS: Self = Self(1 << 8);
    pub const ELIGIBILITY_BITS: u16 =
        Self::GUEST.0 | Self::STAFF.0 | Self::ANIMAL.0 | Self::VEHICLE.0;

    #[must_use]
    pub const fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct NavAgent {
    pub radius_m: f32,
    /// Current permitted speed. Zero is a valid stationary animation interval;
    /// route geometry and request ownership remain valid while movement waits.
    pub max_speed_mps: f32,
    pub acceleration_mps2: f32,
    pub capabilities: NavFlags,
}

impl NavAgent {
    pub fn is_valid(self) -> bool {
        self.radius_m.is_finite()
            && self.radius_m >= 0.0
            && self.max_speed_mps.is_finite()
            && self.max_speed_mps >= 0.0
            && self.acceleration_mps2.is_finite()
            && self.acceleration_mps2 > 0.0
            && self.capabilities.0 & NavFlags::ELIGIBILITY_BITS != 0
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct Velocity(pub Vec3);

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Destination {
    pub request_id: u64,
    pub world: Vec3,
    pub arrival_radius_m: f32,
}

#[derive(Component, Debug, Clone, PartialEq)]
pub struct Route {
    pub points: Vec<Vec3>,
    pub cursor: usize,
}

impl Route {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            points: Vec::with_capacity(capacity),
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.points.clear();
        self.cursor = 0;
    }

    pub fn current(&self) -> Option<Vec3> {
        self.points.get(self.cursor).copied()
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct Steering {
    pub desired_velocity: Vec3,
}

/// Entity-local response to Avian's currently touching contact manifolds.
///
/// The direction is derived afresh from Avian each fixed tick. `factor` is the
/// authored deflection strength: sustained obstruction decays it so an agent
/// can escape a deadlock, then it recovers after a one-second clear interval.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ContactSteering {
    pub direction: Vec3,
    pub factor: f32,
    pub recovery_delay_s: f32,
}

impl Default for ContactSteering {
    fn default() -> Self {
        Self {
            direction: Vec3::ZERO,
            factor: 1.0,
            recovery_delay_s: 0.0,
        }
    }
}

impl ContactSteering {
    pub const MINIMUM_FACTOR: f32 = 0.1;
    pub const DECAY_PER_SECOND: f32 = 0.5;
    pub const RECOVERY_PER_SECOND: f32 = 1.0;
    pub const RECOVERY_DELAY_SECONDS: f32 = 1.0;
}

/// Continuous player-authored local movement axes. This bypasses autonomous
/// destination planning while present and contains no camera or input state.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct DirectLocomotion {
    pub local_axes: Vec2,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialCell(pub u32);

impl SpatialCell {
    pub const OUTSIDE: Self = Self(u32::MAX);
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LocomotionMode {
    #[default]
    Ground,
    Swim,
    Vehicle,
    Flight,
}

/// Which authored swim animation layer applies while an entity is swimming.
/// Navigation and aquatic movement own this spatial fact; animation only reads it.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SwimLayer {
    #[default]
    Surface,
    Submerged,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct AnimalTerrainWaterContact {
    pub depth_m: Option<f32>,
}

#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Docking {
    pub request_id: u64,
    pub target: Entity,
    pub point: Vec3,
    pub forward: Vec3,
    pub radius_m: f32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct NavigateTo {
    pub entity: Entity,
    pub request_id: u64,
    pub destination: Vec3,
    pub arrival_radius_m: f32,
}

/// Read-only route probe used by authored target-position predicates. The
/// locomotion owner evaluates its current native graph and overlays without
/// installing a destination or changing agent motion.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct TestReachability {
    pub entity: Entity,
    pub destination: Vec3,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReachabilityTested {
    pub entity: Entity,
    pub result: Result<(), NavigationFailure>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct DockAt {
    pub entity: Entity,
    pub request_id: u64,
    pub target: Entity,
    pub local_point: Vec3,
    pub local_forward: Vec3,
    pub radius_m: f32,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arrived {
    pub entity: Entity,
    pub request_id: u64,
    pub target: Option<Entity>,
}

#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationFailed {
    pub entity: Entity,
    pub request_id: u64,
    pub reason: NavigationFailure,
}

#[derive(Resource, Debug, Default)]
pub struct NavigationRequestSequence(u64);

impl NavigationRequestSequence {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(1).max(1);
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationFailure {
    OutsideWorld,
    NoStartNode,
    NoRoute,
    TargetGone,
    ContainmentBlocked,
}

/// The navigation index derived from the active terrain during world hydration
/// and refreshed from authoritative terrain samples after edits.
#[derive(Resource, Debug)]
pub struct ActiveTerrainDerivedNavigationGraph {
    terrain_derived_navigation_graph:
        crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
}

impl ActiveTerrainDerivedNavigationGraph {
    /// Eligible dry node centres sampled from a habitat's topology cells,
    /// nearest the recipient first. Route reachability remains the planner's job.
    pub fn collect_dry_ground_candidates(
        &self,
        region: &crate::habitat::habitat_types::HabitatRegion,
        grid: crate::topology::topology_graph_types::TopologyGrid,
        agent: &NavAgent,
        recipient: Vec3,
        output: &mut Vec<Vec3>,
    ) {
        output.clear();
        let graph = self.terrain_derived_navigation_graph();
        let cell_size = f32::from(graph.terrain_cell_size_centimetres) * 0.01;
        if !agent.is_valid() || !recipient.is_finite() || cell_size <= 0.0 {
            return;
        }
        for cell in &region.topology_cells {
            let centre =
                grid.cell_translation(cell.extend(0)).xz() + Vec2::splat(grid.spacing_m * 0.5);
            if centre.x < 0.0 || centre.y > 0.0 {
                continue;
            }
            let terrain_cell = [
                (centre.x / cell_size).floor() as u32,
                (-centre.y / cell_size).floor() as u32,
            ];
            let Some(index) = graph.calculate_navigation_node_for_terrain_cell(terrain_cell) else {
                continue;
            };
            let Some(node) = graph.find_navigation_node(index) else {
                continue;
            };
            if node.eligibility_flags.0 & agent.capabilities.0 & NavFlags::ELIGIBILITY_BITS == 0
                || node.eligibility_flags.0 & (NavFlags::WATER.0 | NavFlags::DISABLED_BY_DEFAULT.0)
                    != 0
                || f32::from(node.clearance_centimetres) < agent.radius_m * 100.0
            {
                continue;
            }
            if let Some(position) =
                graph.calculate_navigation_node_world_position_centimetres(index)
            {
                output.push(
                    Vec3::new(position[0] as f32, position[1] as f32, position[2] as f32) * 0.01,
                );
            }
        }
        output.sort_unstable_by(|a, b| {
            a.xz()
                .distance_squared(recipient.xz())
                .total_cmp(&b.xz().distance_squared(recipient.xz()))
                .then_with(|| a.x.total_cmp(&b.x))
                .then_with(|| a.z.total_cmp(&b.z))
        });
        output.dedup();
    }

    pub const fn new(
        terrain_derived_navigation_graph:
            crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph,
    ) -> Self {
        Self {
            terrain_derived_navigation_graph,
        }
    }

    pub const fn terrain_derived_navigation_graph(
        &self,
    ) -> &crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph
    {
        &self.terrain_derived_navigation_graph
    }

    pub const fn terrain_derived_navigation_graph_mut(
        &mut self,
    ) -> &mut crate::locomotion::terrain_derived_navigation_graph_types::TerrainDerivedNavigationGraph
    {
        &mut self.terrain_derived_navigation_graph
    }

    /// Returns eligible node positions in the terrain tile containing the
    /// supplied world-space position.
    pub fn candidate_positions(
        &self,
        position_cm: [i32; 3],
        eligibility: NavFlags,
    ) -> impl Iterator<Item = [i32; 3]> + '_ {
        self.terrain_derived_navigation_graph
            .find_candidate_node_indices_in_containing_terrain_tile(position_cm)
            .filter_map(move |index| {
                self.terrain_derived_navigation_graph
                    .find_navigation_node(index)
                    .filter(|node| node.eligibility_flags.contains(eligibility))
                    .and_then(|_| {
                        self.terrain_derived_navigation_graph
                            .calculate_navigation_node_world_position_centimetres(index)
                    })
            })
    }
}

/// Capacity policy compiled for the active world and installed during
/// hydration. It prevents route and spatial storage growth in settled play.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocomotionCapacity {
    pub max_agents: usize,
    pub max_route_points: usize,
}

impl LocomotionCapacity {
    pub const fn is_valid(self) -> bool {
        self.max_agents > 0
            && self.max_agents <= u32::MAX as usize
            && self.max_route_points > 1
            && self.max_route_points <= u32::MAX as usize
    }
}

#[derive(Resource, Debug)]
pub struct SpatialGrid {
    pub origin: Vec2,
    pub cell_size_m: f32,
    pub width: u32,
    pub height: u32,
    pub cell_offsets: Box<[u32]>,
    pub write_cursors: Box<[u32]>,
    pub entities: Vec<Entity>,
}

impl Default for SpatialGrid {
    fn default() -> Self {
        Self {
            origin: Vec2::ZERO,
            cell_size_m: 1.0,
            width: 0,
            height: 0,
            cell_offsets: Box::new([]),
            write_cursors: Box::new([]),
            entities: Vec::new(),
        }
    }
}

impl SpatialGrid {
    pub fn reserve_layout(
        &mut self,
        origin: Vec2,
        cell_size_m: f32,
        width: u32,
        height: u32,
        entity_capacity: usize,
    ) -> bool {
        let Some(cells) = usize::try_from(u64::from(width) * u64::from(height)).ok() else {
            return false;
        };
        if !origin.is_finite() || !cell_size_m.is_finite() || cell_size_m <= 0.0 || cells == 0 {
            return false;
        }
        self.origin = origin;
        self.cell_size_m = cell_size_m;
        self.width = width;
        self.height = height;
        self.cell_offsets = vec![0; cells + 1].into_boxed_slice();
        self.write_cursors = vec![0; cells].into_boxed_slice();
        self.entities = Vec::with_capacity(entity_capacity);
        true
    }

    pub fn cell_of(&self, world_xz: Vec2) -> Option<u32> {
        if !world_xz.is_finite() || self.width == 0 || self.height == 0 {
            return None;
        }
        let cell = ((world_xz - self.origin) / self.cell_size_m)
            .floor()
            .as_ivec2();
        if cell.x < 0 || cell.y < 0 || cell.x >= self.width as i32 || cell.y >= self.height as i32 {
            return None;
        }
        Some(cell.y as u32 * self.width + cell.x as u32)
    }

    pub fn cell_xy(&self, index: u32) -> Option<UVec2> {
        (index < self.width.saturating_mul(self.height))
            .then(|| UVec2::new(index % self.width, index / self.width))
    }

    pub fn range(&self, cell: u32) -> &[Entity] {
        let index = cell as usize;
        let Some((&start, &end)) = self
            .cell_offsets
            .get(index)
            .zip(self.cell_offsets.get(index + 1))
        else {
            return &[];
        };
        self.entities
            .get(start as usize..end as usize)
            .unwrap_or(&[])
    }
}

/// Sparse references to canonical fence entities crossed by terrain graph edges.
/// Collision policy and gate state remain owned by their definitions and entities.
#[derive(Resource, Debug, Default)]
pub struct NavigationOverlay {
    fence_crossings: HashMap<(u32, u32), Vec<Entity>>,
    pub revision: u64,
}

impl NavigationOverlay {
    pub fn replace_fence_crossings(&mut self, mut crossings: HashMap<(u32, u32), Vec<Entity>>) {
        for fences in crossings.values_mut() {
            fences.sort_unstable();
        }
        if self.fence_crossings != crossings {
            self.fence_crossings = crossings;
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn fence_crossings(&self, from: u32, to: u32) -> &[Entity] {
        self.fence_crossings
            .get(&(from, to))
            .map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Default)]
pub struct RouteScratch {
    pub costs: Vec<u64>,
    pub previous: Vec<u32>,
    pub marks: Vec<u32>,
    pub closed: Vec<bool>,
    pub heap_positions: Vec<u32>,
    pub generation: u32,
    pub open: Vec<OpenNode>,
    pub reverse_path: Vec<u32>,
}

impl RouteScratch {
    pub fn ensure_capacity(&mut self, navigation_node_count: usize, route_capacity: usize) {
        if self.costs.len() != navigation_node_count {
            self.costs = vec![u64::MAX; navigation_node_count];
            self.previous = vec![u32::MAX; navigation_node_count];
            self.marks = vec![0; navigation_node_count];
            self.closed = vec![false; navigation_node_count];
            self.heap_positions = vec![u32::MAX; navigation_node_count];
            self.open = Vec::with_capacity(navigation_node_count);
            self.generation = 0;
        }
        if self.reverse_path.capacity() < route_capacity {
            self.reverse_path = Vec::with_capacity(route_capacity);
        }
    }

    pub fn begin(&mut self) {
        self.open.clear();
        self.reverse_path.clear();
        self.closed.fill(false);
        self.heap_positions.fill(u32::MAX);
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill(0);
            self.generation = 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenNode {
    pub node: u32,
    pub cost: u64,
    pub estimate: u64,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct MotionProgress {
    pub last_position: Vec3,
    pub stationary_ticks: u16,
}

pub const STUCK_TICKS: u16 = 120;
pub const MAX_STEERING_NEIGHBORS: usize = 24;

#[cfg(test)]
mod tests;
