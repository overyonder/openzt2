use std::sync::OnceLock;

use bevy::{asset::AssetId, platform::collections::HashMap, prelude::*};
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_game_data::world_definitions::object_placement::{
    FootprintCell, FootprintCellFlags, PlaceableDefinition,
};

#[derive(Resource, Default)]
pub struct ScenePlacementFootprints(HashMap<AssetId<ScenePrefabAsset>, SceneFootprintSource>);

struct SceneFootprintSource {
    bounds: [[f32; 2]; 2],
    footprint: OnceLock<Option<ScenePlacementFootprint>>,
}

struct ScenePlacementFootprint {
    cells: Vec<FootprintCell>,
    pivot_cm: [i16; 2],
}

#[derive(Clone, Copy)]
pub struct ObjectPlacementGeometry<'a> {
    pub definition: &'a PlaceableDefinition,
    automatic: Option<&'a ScenePlacementFootprint>,
}

impl<'a> From<&'a PlaceableDefinition> for ObjectPlacementGeometry<'a> {
    fn from(definition: &'a PlaceableDefinition) -> Self {
        Self {
            definition,
            automatic: None,
        }
    }
}

impl<'a> ObjectPlacementGeometry<'a> {
    pub fn cells(self, eighth_turns: u8) -> &'a [FootprintCell] {
        if let Some(automatic) = self.automatic_for_turns(eighth_turns) {
            return &automatic.cells;
        }
        if eighth_turns & 1 == 0 {
            &self.definition.footprint
        } else {
            &self.definition.diagonal_footprint
        }
    }

    pub fn pivot_cm(self, eighth_turns: u8) -> [i16; 2] {
        if let Some(automatic) = self.automatic_for_turns(eighth_turns) {
            return automatic.pivot_cm;
        }
        if eighth_turns & 1 == 0 {
            self.definition.pivot_cm
        } else {
            self.definition.diagonal_pivot_cm
        }
    }

    fn automatic_for_turns(self, eighth_turns: u8) -> Option<&'a ScenePlacementFootprint> {
        if eighth_turns & 1 != 0 && self.definition.rotation_increment_degrees == 45 {
            return None;
        }
        self.automatic
    }
}

impl ScenePlacementFootprints {
    pub fn resolve<'a>(
        &'a self,
        definition: &'a PlaceableDefinition,
        scene: Option<&Handle<ScenePrefabAsset>>,
    ) -> ObjectPlacementGeometry<'a> {
        let automatic = scene
            .filter(|_| definition.automatic_footprint)
            .and_then(|scene| self.0.get(&scene.id()))
            .and_then(|source| {
                source
                    .footprint
                    .get_or_init(|| ScenePlacementFootprint::from_bounds(source.bounds))
                    .as_ref()
            });
        ObjectPlacementGeometry {
            definition,
            automatic,
        }
    }

    pub fn has_scene_bounds(&self, scene: &Handle<ScenePrefabAsset>) -> bool {
        self.0.contains_key(&scene.id())
    }

    pub fn update_scene_bounds(
        &mut self,
        id: AssetId<ScenePrefabAsset>,
        bounds: Option<[[f32; 2]; 2]>,
    ) -> bool {
        if self.0.get(&id).map(|source| source.bounds) == bounds {
            return false;
        }
        if let Some(bounds) = bounds {
            self.0.insert(
                id,
                SceneFootprintSource {
                    bounds,
                    footprint: OnceLock::new(),
                },
            );
        } else {
            self.0.remove(&id);
        }
        true
    }
}

impl ScenePlacementFootprint {
    fn from_bounds([minimum, maximum]: [[f32; 2]; 2]) -> Option<Self> {
        let dimensions =
            std::array::from_fn::<_, 2, _>(|axis| (maximum[axis] - minimum[axis]).ceil().max(1.0));
        let pivot_cm = minimum.map(|minimum| (-minimum * 100.0).round());
        if dimensions
            .iter()
            .any(|value| !(1.0..=f32::from(i16::MAX)).contains(value))
            || pivot_cm
                .iter()
                .any(|value| !(f32::from(i16::MIN)..=f32::from(i16::MAX)).contains(value))
        {
            return None;
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "validated centimetres and grid dimensions"
        )]
        let (dimensions, pivot_cm) = (
            dimensions.map(|value| value as i16),
            pivot_cm.map(|value| value as i16),
        );
        let cells = (0..dimensions[1])
            .flat_map(|z| {
                (0..dimensions[0]).map(move |x| FootprintCell {
                    offset: [x, z],
                    flags: FootprintCellFlags::OCCUPIED,
                })
            })
            .collect();
        Some(Self { cells, pivot_cm })
    }
}
