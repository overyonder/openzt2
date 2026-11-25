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
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::construction::UiConstructionAction;
use openzt2_game_data::{
    terrain::TerrainWaterDepth,
    world_definitions::biomes_locations_and_details::BiomeAutomaticPlacementVariationKind,
};
use openzt2_interface_state::ui::{
    authored_ui_action_projection_components::UiConstructionActions,
    authored_ui_activation_contracts::UiNodeActivated,
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
};
use openzt2_zoo_state::{
    construction::construction_tool_and_placement_policy_types::{
        BiomeSurface, ConstructionPlacementPolicy, ConstructionTool,
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

    fn cursor_size_policy(&self, kind: TerrainBrushKind) -> Option<(usize, [f32; 2], f32, f32)> {
        if deformation_slot(kind).is_some() {
            let TerrainBrushMode::Deformation {
                width,
                initial,
                increment,
                ..
            } = self.assets.get(&self.handles.deformation)?
            else {
                return None;
            };
            return Some((0, *width, *initial, *increment));
        }
        let TerrainBrushMode::Paint {
            ground_width,
            water_width,
            initial,
            increment,
            ..
        } = self.assets.get(&self.handles.paint)?
        else {
            return None;
        };
        match kind {
            TerrainBrushKind::Paint { .. } => Some((1, *ground_width, *initial, *increment)),
            TerrainBrushKind::PaintWater { .. } => Some((2, *water_width, *initial, *increment)),
            _ => None,
        }
    }

    pub(super) fn brush_dimensions_and_speed(
        &self,
        kind: TerrainBrushKind,
        controls: &TerrainBrushControls,
    ) -> Option<(f32, f32)> {
        if matches!(
            kind,
            TerrainBrushKind::AddWater | TerrainBrushKind::RemoveWater
        ) {
            let TerrainBrushMode::Paint {
                automatic_water_radius,
                ..
            } = self.assets.get(&self.handles.paint)?
            else {
                return None;
            };
            return Some((*automatic_water_radius, 1.0));
        }
        let (slot, bounds, initial, _) = self.cursor_size_policy(kind)?;
        let radius = controls.radius[slot]
            .unwrap_or(initial)
            .clamp(bounds[0], bounds[1]);
        let Some(index) = deformation_slot(kind) else {
            return Some((radius, 1.0));
        };
        let TerrainBrushMode::Deformation { speeds, .. } =
            self.assets.get(&self.handles.deformation)?
        else {
            return None;
        };
        let speed = if matches!(kind, TerrainBrushKind::Flatten { .. }) {
            controls.flatten_speed.unwrap_or(speeds[index])
        } else {
            speeds[index]
        };
        Some((radius, speed))
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

#[derive(Resource, Default)]
pub(super) struct TerrainBrushControls {
    pub(super) radius: [Option<f32>; 3],
    pub(super) flatten_speed: Option<f32>,
}

pub(super) fn apply_authored_terrain_brush_controls(
    mut activations: MessageReader<UiNodeActivated>,
    documents: Res<Assets<UiDocumentAsset>>,
    action_nodes: Query<(&UiConstructionActions, &UiDocumentOwner)>,
    roots: Query<&UiDocumentRoot>,
    tool: Res<ConstructionTool>,
    modes: AuthoredTerrainBrushModes,
    mut controls: ResMut<TerrainBrushControls>,
) {
    for activation in activations.read() {
        let Ok((actions, owner)) = action_nodes.get(activation.node) else {
            continue;
        };
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
        else {
            continue;
        };
        for record in actions
            .authored_action_records(document)
            .filter(|record| record.trigger == activation.trigger)
        {
            match &record.action {
                UiConstructionAction::SetTerrainCursorSize { direction } => {
                    let ConstructionTool::Terrain(kind) = *tool else {
                        continue;
                    };
                    let Some((slot, bounds, initial, increment)) = modes.cursor_size_policy(kind)
                    else {
                        continue;
                    };
                    let radius = controls.radius[slot]
                        .unwrap_or(initial)
                        .clamp(bounds[0], bounds[1]);
                    let radius = (radius + direction.signum() as f32 * increment)
                        .clamp(bounds[0], bounds[1]);
                    controls
                        .reborrow()
                        .map_unchanged(|controls| &mut controls.radius[slot])
                        .set_if_neq(Some(radius));
                }
                UiConstructionAction::SetTerrainFlattenSpeed {
                    strength_tenths_per_second,
                } => {
                    let value = strength_tenths_per_second
                        .unsigned_abs()
                        .min(u32::from(u16::MAX)) as f32
                        * 0.1;
                    controls
                        .reborrow()
                        .map_unchanged(|controls| &mut controls.flatten_speed)
                        .set_if_neq(Some(value));
                }
                _ => {}
            }
        }
    }
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
        if increment < 0.0 {
            return Err(invalid("cursorSizeIncrement"));
        }
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
