use std::io;

use bevy::prelude::*;
use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};
use openzt2_game_data::world_definitions::paths_and_tile_surfaces::GuestPathDefinition;
use openzt2_world_state::world_spawn::persistent_id_types::PersistentId;
use openzt2_zoo_state::{
    construction::construction_tool_and_placement_policy_types::{
        ConstructionPlacementPolicy, ElevatedPathGrade,
    },
    topology::topology_graph_types::TileSurface,
};

use super::topology_construction_preview_types::PathPreview;

const ELEVATION_STEP_CM: i32 = 150;

#[derive(Asset, TypePath)]
pub(super) struct ElevatedPathModeDefinition {
    clearance_cm: i32,
}

#[derive(Default, TypePath)]
pub(super) struct ElevatedPathModeLoader;

impl AssetLoader for ElevatedPathModeLoader {
    type Asset = ElevatedPathModeDefinition;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        (): &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy();
        let document = parse_blue_fang_source_document(AssetPath::new(path), &bytes)
            .map_err(io::Error::other)?;
        fn mode(node: &OrderedSourceDocumentNode) -> Option<&OrderedSourceDocumentNode> {
            if node.name == "ZTElevatedPathMode" {
                Some(node)
            } else {
                node.element_children().find_map(mode)
            }
        }
        let clearance = mode(&document.root)
            .and_then(|node| node.attribute("minPathClearance"))
            .and_then(|value| value.parse::<f32>().ok())
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing elevated path clearance",
                )
            })?;
        Ok(ElevatedPathModeDefinition {
            clearance_cm: (clearance * 100.0).round() as i32,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

#[derive(Resource)]
pub(super) struct ElevatedPathModeAsset(Handle<ElevatedPathModeDefinition>);

impl FromWorld for ElevatedPathModeAsset {
    fn from_world(world: &mut World) -> Self {
        Self(world.resource::<AssetServer>().load("ui/modes/modes.xml"))
    }
}

#[derive(SystemParam)]
pub(super) struct ElevatedPathMode<'w> {
    assets: Res<'w, Assets<ElevatedPathModeDefinition>>,
    source: Res<'w, ElevatedPathModeAsset>,
}

impl ElevatedPathMode<'_> {
    pub(super) fn clearance_cm(&self) -> Option<i32> {
        Some(self.assets.get(&self.source.0)?.clearance_cm)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ElevatedPathProfile {
    reference_cm: i32,
    pub height_cm: i32,
    pub grade: ElevatedPathGrade,
    pub spacing_cm: u16,
}

impl ElevatedPathProfile {
    pub(super) fn new(
        base_m: f32,
        definition: &GuestPathDefinition,
        spacing_cm: u16,
        policy: &ConstructionPlacementPolicy,
    ) -> Self {
        let min = i32::from(definition.support_headroom_cm);
        let reference_cm = (base_m * 100.0).round() as i32;
        let height_cm =
            reference_cm + min + i32::from(policy.elevated_path_height_steps) * ELEVATION_STEP_CM;
        Self {
            reference_cm,
            height_cm,
            grade: policy.elevated_path_grade,
            spacing_cm,
        }
    }

    pub(super) fn reference_m(self) -> f32 {
        self.reference_cm as f32 * 0.01
    }
}

#[derive(Component, Default)]
pub(super) struct ElevatedPathSurfaceEdits(pub Vec<(PersistentId, TileSurface)>);

pub(super) fn elevated_path_tiles(
    preview: &PathPreview,
    definition: &GuestPathDefinition,
) -> Option<Vec<(IVec3, TileSurface)>> {
    let profile = preview.profile?;
    let horizontal_stride =
        (i32::from(definition.width_cm) / i32::from(profile.spacing_cm.max(1))).max(1);
    let delta = preview.to.xy() - preview.from.xy();
    let heading = if delta == IVec2::ZERO {
        0
    } else {
        ((delta.x as f32).atan2(-(delta.y as f32)) / core::f32::consts::FRAC_PI_4).round() as i32
    }
    .rem_euclid(8);
    let step = [
        IVec2::NEG_Y,
        IVec2::new(1, -1),
        IVec2::X,
        IVec2::ONE,
        IVec2::Y,
        IVec2::new(-1, 1),
        IVec2::NEG_X,
        IVec2::NEG_ONE,
    ][heading as usize];
    let span = step * horizontal_stride;
    let slots = (delta.as_vec2().dot(span.as_vec2()) / span.as_vec2().length_squared())
        .round()
        .max(0.0) as i32;
    let diagonal = heading % 2 != 0;
    let climb = match profile.grade {
        ElevatedPathGrade::Flat => 0,
        ElevatedPathGrade::RampUp => ELEVATION_STEP_CM,
        ElevatedPathGrade::RampDown => -ELEVATION_STEP_CM,
    };
    let offsets = if diagonal {
        [0, climb / 2, 0, -climb / 2, 0]
    } else {
        [climb, climb, 0, 0, climb / 2]
    };
    (0..=slots)
        .map(|index| {
            let map = preview.from.xy() + span * index;
            let grade_shift = if diagonal { climb / 2 } else { 0 };
            let baseline = profile.height_cm + index * climb + grade_shift;
            let total = baseline + offsets[4];
            let grid_y = (total as f32 / 100.0).round() as i32;
            let mut samples = [0; 5];
            for (corner, sample) in samples.iter_mut().take(4).enumerate() {
                let rotated = (corner + 4 - heading as usize / 2) % 4;
                *sample = i16::try_from(baseline + offsets[rotated] - 100 * grid_y).ok()?;
            }
            samples[4] = i16::try_from(total - 100 * grid_y).ok()?;
            Some((map.extend(grid_y), TileSurface { height_cm: samples }))
        })
        .collect()
}
