use bevy::prelude::*;

use crate::plugins::construction::construction_interaction_types::ConstructionPreview;

use super::{
    terrain_brush_interaction_state::{TerrainBrushDab, TerrainBrushPath},
    terrain_brush_types::{TerrainBrushPreview, TerrainStroke},
};
pub(super) fn accumulate_terrain_stroke_dabs_into_brush_preview(
    mut commands: Commands,
    mut strokes: MessageReader<TerrainStroke>,
    mut previews: Query<
        (
            Entity,
            &mut TerrainBrushPreview,
            Option<&mut TerrainBrushPath>,
        ),
        With<ConstructionPreview>,
    >,
) {
    for stroke in strokes.read() {
        let Ok((entity, mut preview, path)) = previews.get_mut(stroke.preview) else {
            continue;
        };
        if stroke.center.is_finite()
            && stroke.radius_m.is_finite()
            && stroke.radius_m > 0.0
            && stroke.strength_per_s.is_finite()
            && stroke.seconds.is_finite()
            && stroke.seconds >= 0.0
        {
            let dab = TerrainBrushDab {
                center: stroke.center,
                seconds: stroke.seconds,
            };
            if let Some(mut path) = path {
                if let Some(previous) = path
                    .0
                    .last_mut()
                    .filter(|previous| previous.center == dab.center)
                {
                    previous.seconds += dab.seconds;
                } else {
                    path.0.push(dab);
                }
            } else {
                commands.entity(entity).insert(TerrainBrushPath(vec![dab]));
            }
            let elapsed = preview.seconds + stroke.seconds;
            *preview = TerrainBrushPreview {
                center: stroke.center,
                radius_m: stroke.radius_m,
                strength_per_s: stroke.strength_per_s,
                seconds: elapsed,
                kind: stroke.kind,
                falloff: stroke.falloff,
                paint_trees: stroke.paint_trees,
                paint_foliage: stroke.paint_foliage,
                paint_rocks: stroke.paint_rocks,
                automatic_placement_variation: stroke.automatic_placement_variation,
            };
        }
    }
}
