use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
};
use openzt2_game_data::{
    terrain::TerrainWaterDepth,
    world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind,
};
use openzt2_zoo_state::{
    construction::construction_tool_and_placement_policy_types::{
        BiomeSurface, ConstructionPlacementPolicy,
    },
    terrain::terrain_brush_types::TerrainBrushKind,
};

#[derive(Asset, TypePath)]
pub(super) enum TerrainBrushMode {
    Deformation {
        speeds: [f32; 4],
        width: [f32; 2],
        initial: f32,
        increment: f32,
        smooth_samples: u16,
        cursors: [Option<Handle<Image>>; 4],
    },
    Paint {
        ground_width: [f32; 2],
        water_width: [f32; 2],
        initial: f32,
        increment: f32,
        automatic_water_radius: f32,
        cursors: [Option<Handle<Image>>; 7],
    },
}

#[derive(Resource)]
pub(super) struct TerrainBrushModeHandles {
    pub(super) deformation: Handle<TerrainBrushMode>,
    pub(super) paint: Handle<TerrainBrushMode>,
}

#[derive(SystemParam)]
pub(super) struct AuthoredTerrainBrushModes<'w> {
    handles: Res<'w, TerrainBrushModeHandles>,
    assets: Res<'w, Assets<TerrainBrushMode>>,
}

impl AuthoredTerrainBrushModes<'_> {
    pub(super) fn is_changed(&self) -> bool {
        self.handles.is_changed() || self.assets.is_changed()
    }

    pub(super) fn smooth_samples(&self) -> Option<u16> {
        match self.assets.get(&self.handles.deformation)? {
            TerrainBrushMode::Deformation { smooth_samples, .. } => Some(*smooth_samples),
            TerrainBrushMode::Paint { .. } => None,
        }
    }

    pub(super) fn brush_dimensions_and_speed(
        &self,
        kind: TerrainBrushKind,
        policy: &ConstructionPlacementPolicy,
    ) -> Option<(f32, f32)> {
        if let Some(index) = deformation_slot(kind) {
            let TerrainBrushMode::Deformation {
                speeds,
                width,
                initial,
                increment,
                ..
            } = self.assets.get(&self.handles.deformation)?
            else {
                return None;
            };
            let radius = brush_width(
                *width,
                *initial,
                *increment,
                policy.terrain_deformation_brush_radius_half_metre_steps,
            );
            let speed = if index == 2
                && policy.terrain_flatten_strength_tenths_per_second
                    != ConstructionPlacementPolicy::default()
                        .terrain_flatten_strength_tenths_per_second
            {
                f32::from(policy.terrain_flatten_strength_tenths_per_second) * 0.1
            } else {
                speeds[index]
            };
            return Some((radius, speed));
        }
        let TerrainBrushMode::Paint {
            ground_width,
            water_width,
            initial,
            increment,
            automatic_water_radius,
            ..
        } = self.assets.get(&self.handles.paint)?
        else {
            return None;
        };
        let radius = match kind {
            TerrainBrushKind::Paint { .. } => brush_width(
                *ground_width,
                *initial,
                *increment,
                policy.biome_texture_brush_radius_half_metre_steps,
            ),
            TerrainBrushKind::PaintWater { .. } => brush_width(
                *water_width,
                *initial,
                *increment,
                policy.biome_water_brush_radius_half_metre_steps,
            ),
            TerrainBrushKind::AddWater | TerrainBrushKind::RemoveWater => *automatic_water_radius,
            _ => return None,
        };
        Some((radius, 1.0))
    }

    pub(super) fn cursor(
        &self,
        kind: TerrainBrushKind,
        policy: &ConstructionPlacementPolicy,
    ) -> Option<&Handle<Image>> {
        if let Some(index) = deformation_slot(kind) {
            let TerrainBrushMode::Deformation { cursors, .. } =
                self.assets.get(&self.handles.deformation)?
            else {
                return None;
            };
            return cursors[index].as_ref();
        }
        let TerrainBrushMode::Paint { cursors, .. } = self.assets.get(&self.handles.paint)? else {
            return None;
        };
        let index = match kind {
            TerrainBrushKind::PaintWater {
                depth: TerrainWaterDepth::Deep,
                ..
            } => 0,
            TerrainBrushKind::PaintWater {
                depth: TerrainWaterDepth::Shallow,
                ..
            } => 1,
            TerrainBrushKind::Paint { .. } => match policy.biome_surface {
                BiomeSurface::Ground => 2,
                BiomeSurface::GroundCover => 3,
                BiomeSurface::MixedGround => 4,
                BiomeSurface::FoliageMix
                    if policy.biome_automatic_placement_variation
                        == BiomeAutomaticPlacementVariationKind::FoliageMixTerrain =>
                {
                    6
                }
                BiomeSurface::FoliageMix => 5,
                BiomeSurface::DeepWater | BiomeSurface::ShallowWater => return None,
            },
            _ => return None,
        };
        cursors[index].as_ref()
    }
}

fn deformation_slot(kind: TerrainBrushKind) -> Option<usize> {
    match kind {
        TerrainBrushKind::Raise => Some(0),
        TerrainBrushKind::Lower => Some(1),
        TerrainBrushKind::Flatten { .. } => Some(2),
        TerrainBrushKind::Smooth => Some(3),
        _ => None,
    }
}

fn brush_width(bounds: [f32; 2], initial: f32, increment: f32, control_steps: u8) -> f32 {
    let steps = i16::from(control_steps)
        - i16::from(
            ConstructionPlacementPolicy::default()
                .terrain_deformation_brush_radius_half_metre_steps,
        );
    // A scenario can supply a width directly outside the interactive UI's range.
    if !(2..=16).contains(&control_steps) {
        return f32::from(control_steps) * 0.5;
    }
    (initial + f32::from(steps) * increment).clamp(bounds[0], bounds[1])
}

pub(super) fn load_authored_terrain_brush_modes(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(TerrainBrushModeHandles {
        deformation: server.load("ui/modes/terrdeform.xml"),
        paint: server.load("ui/modes/terrpaint.xml"),
    });
}

#[derive(Default, TypePath)]
pub(super) struct TerrainBrushModeLoader;

impl AssetLoader for TerrainBrushModeLoader {
    type Asset = TerrainBrushMode;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<TerrainBrushMode> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let doc = parse_blue_fang_source_document(path, &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.to_string()))?;
        let invalid = |name: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid terrain brush mode field {name}"),
            )
        };
        let read = |name: &str| {
            doc.root
                .attribute_named_any(&[name])
                .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                .filter(|value| value.is_finite())
                .ok_or_else(|| invalid(name))
        };
        let width = |min, max| {
            let low = read(min)?;
            let high = read(max)?;
            if low > 0.0 && high >= low {
                Ok([low, high])
            } else {
                Err(invalid(min))
            }
        };
        let initial = read("startCursorWidth")?;
        let increment = read("cursorSizeIncrement")?;
        let cursors = doc
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("cursors"));
        let mut texture = |name: &str| {
            let source = cursors?
                .element_children()
                .find(|node| node.name.eq_ignore_ascii_case(name))?
                .first_text()?;
            let path = AssetPath::new(source.trim()).key();
            (!path.is_empty()).then(|| context.load(path))
        };
        if doc.root.name.eq_ignore_ascii_case("TerrDeformationUI") {
            let smooth_samples = doc
                .root
                .attribute_named_any(&["smoothSample"])
                .and_then(parse_blue_fang_source_numeric_lexeme)
                .ok_or_else(|| invalid("smoothSample"))?;
            Ok(TerrainBrushMode::Deformation {
                speeds: [
                    read("hillSpeed")?,
                    read("valleySpeed")?,
                    read("flattenSpeed")?,
                    read("smoothSpeed")?,
                ],
                width: width("minCursorWidth", "maxCursorWidth")?,
                initial,
                increment,
                smooth_samples,
                cursors: ["hill", "valley", "flatten", "smooth"].map(&mut texture),
            })
        } else if doc.root.name.eq_ignore_ascii_case("TerrPaintUI") {
            Ok(TerrainBrushMode::Paint {
                ground_width: width("minTextureCursorWidth", "maxTextureCursorWidth")?,
                water_width: width("minWaterCursorWidth", "maxWaterCursorWidth")?,
                initial,
                increment,
                automatic_water_radius: read("autoWaterRadius")?,
                cursors: [
                    "deepwater",
                    "shallowwater",
                    "ground",
                    "ground-cover",
                    "mix",
                    "foliage-mix",
                    "foliage-mix-terrain",
                ]
                .map(texture),
            })
        } else {
            Err(invalid("root"))
        }
    }
}
