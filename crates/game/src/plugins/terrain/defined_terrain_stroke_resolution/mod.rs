use bevy::prelude::*;
use openzt2_game_data::world_definitions::terrain_editing_brushes::{
    TerrainEditingBrushFalloff, TerrainEditingBrushOperation,
};

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::construction::construction_tool_and_placement_policy_types::ConstructionPlacementPolicy;

use super::terrain_brush_types::{
    DefinedTerrainStroke, TerrainBrushFalloff, TerrainBrushKind, TerrainStroke,
};
pub(super) fn resolve_authored_terrain_brush_definitions_into_concrete_strokes(
    mut authored: MessageReader<DefinedTerrainStroke>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut strokes: MessageWriter<TerrainStroke>,
    policy: Res<ConstructionPlacementPolicy>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for request in authored.read() {
        let Some(definition) = definitions
            .brushes()
            .find(|definition| definition.id == request.definition)
        else {
            continue;
        };
        let radius_min = f32::from(definition.radius_cm[0]) * 0.01;
        let radius_max = f32::from(definition.radius_cm[1]) * 0.01;
        let radius_t = f32::from(request.radius_permille.min(1000)) * 0.001;
        let kind = match definition.operation {
            TerrainEditingBrushOperation::Raise => TerrainBrushKind::Raise,
            TerrainEditingBrushOperation::Lower => TerrainBrushKind::Lower,
            TerrainEditingBrushOperation::Flatten => TerrainBrushKind::Flatten {
                height_cm: request.flatten_height_cm,
            },
            TerrainEditingBrushOperation::Smooth => TerrainBrushKind::Smooth,
            TerrainEditingBrushOperation::PaintBiome => {
                let Some(biome) = request.paint_biome else {
                    continue;
                };
                TerrainBrushKind::Paint {
                    biome,
                    ground_cover: None,
                }
            }
            TerrainEditingBrushOperation::AddWater => TerrainBrushKind::AddWater,
            TerrainEditingBrushOperation::RemoveWater => TerrainBrushKind::RemoveWater,
        };
        let falloff = match definition.falloff {
            TerrainEditingBrushFalloff::Constant => TerrainBrushFalloff::Constant,
            TerrainEditingBrushFalloff::Linear => TerrainBrushFalloff::Linear,
            TerrainEditingBrushFalloff::Smooth => TerrainBrushFalloff::Smooth,
        };
        strokes.write(TerrainStroke {
            preview: request.preview,
            center: request.center,
            radius_m: radius_min + (radius_max - radius_min) * radius_t,
            strength_per_s: f32::from(definition.strength_permille)
                * f32::from(request.pressure_permille.min(1000))
                * 0.000_001,
            seconds: request.seconds,
            kind,
            falloff,
            paint_trees: policy.biome_paint_trees && matches!(kind, TerrainBrushKind::Paint { .. }),
            paint_foliage: policy.biome_paint_foliage
                && matches!(kind, TerrainBrushKind::Paint { .. }),
            paint_rocks: policy.biome_paint_rocks && matches!(kind, TerrainBrushKind::Paint { .. }),
            automatic_placement_variation: policy.biome_automatic_placement_variation,
        });
    }
}
