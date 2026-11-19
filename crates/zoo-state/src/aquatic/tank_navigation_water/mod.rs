use bevy::prelude::*;

/// Water depths contributed by tanks, indexed by the active world's navigation
/// node indices. Zero means the node uses its terrain water facts.
#[derive(Component)]
pub struct TankNavigationWater {
    depth_centimetres: Box<[u16]>,
    occupied: bevy::platform::collections::HashSet<u32>,
}

impl TankNavigationWater {
    pub fn new(node_count: usize) -> Self {
        Self {
            depth_centimetres: vec![0; node_count].into_boxed_slice(),
            occupied: default(),
        }
    }

    pub fn depth_centimetres(&self, node: u32) -> Option<u16> {
        self.depth_centimetres
            .get(node as usize)
            .copied()
            .filter(|&depth| depth != 0)
    }

    pub fn replace_cells(&mut self, cells: impl IntoIterator<Item = (u32, u16)>) {
        for node in self.occupied.drain() {
            self.depth_centimetres[node as usize] = 0;
        }
        self.update_cells(cells);
    }

    pub fn update_cells(&mut self, cells: impl IntoIterator<Item = (u32, u16)>) {
        for (node, depth) in cells {
            let Some(cell) = self.depth_centimetres.get_mut(node as usize) else {
                continue;
            };
            *cell = depth;
            if depth == 0 {
                self.occupied.remove(&node);
            } else {
                self.occupied.insert(node);
            }
        }
    }
}
