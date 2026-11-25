use bevy::prelude::*;

pub(super) const PLOT_LEFT: f32 = 44.0;
pub(super) const PLOT_RIGHT: f32 = 8.0;
pub(super) const PLOT_TOP: f32 = 8.0;
pub(super) const PLOT_BOTTOM: f32 = 22.0;

const LINE_THICKNESS: f32 = 2.0;

#[derive(Clone, Copy)]
pub(super) struct GeneratedGraphElementRectangle {
    pub(super) left: f32,
    pub(super) bottom: f32,
    pub(super) width: f32,
    pub(super) height: f32,
}

#[derive(Default)]
pub(super) struct GraphDrawings {
    pub(super) plot: Option<Vec<(GeneratedGraphElementRectangle, UiTransform)>>,
    pub(super) labels: Option<Vec<GraphLabel>>,
    pub(super) ticks: Vec<GraphTick>,
}

pub(super) struct GraphLabel {
    pub(super) text: String,
    pub(super) left: f32,
    pub(super) bottom: f32,
    pub(super) width: f32,
    pub(super) justify: Justify,
}

pub(super) fn record_colored_graph_geometry_element(
    drawings: &mut GraphDrawings,
    rectangle: GeneratedGraphElementRectangle,
    transform: UiTransform,
) {
    if let Some(plot) = &mut drawings.plot {
        plot.push((rectangle, transform));
    }
}

pub(super) fn record_graph_text_label(
    drawings: &mut GraphDrawings,
    label: &str,
    left: f32,
    bottom: f32,
    width: f32,
    justify: Justify,
) {
    if let Some(labels) = &mut drawings.labels {
        labels.push(GraphLabel {
            text: label.to_owned(),
            left,
            bottom,
            width,
            justify,
        });
    }
}

pub(super) fn record_graph_line_segment(
    drawings: &mut GraphDrawings,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
) {
    let delta = Vec2::new(end_x - start_x, end_y - start_y);
    let length = delta.length();
    let midpoint = Vec2::new(start_x, start_y) + delta * 0.5;
    record_colored_graph_geometry_element(
        drawings,
        GeneratedGraphElementRectangle {
            left: midpoint.x - length * 0.5,
            bottom: midpoint.y - LINE_THICKNESS * 0.5,
            width: length,
            height: LINE_THICKNESS,
        },
        UiTransform::from_rotation(Rot2::radians(-delta.y.atan2(delta.x))),
    );
}

pub(super) struct GraphTick {
    pub(super) value: f64,
    pub(super) bottom: f32,
}
