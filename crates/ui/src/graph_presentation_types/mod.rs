use bevy::prelude::*;
use openzt2_game_data::ui_document::action::information::InformationGraphType;

/// Immutable authored graph presentation policy. Series values and labels stay
/// with their canonical economy, progression, and localization owners.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(super) struct UiGraphPresentationPolicy {
    pub(super) graph_type: Option<InformationGraphType>,
    pub(super) forced_minimum_y: f32,
    pub(super) forced_maximum_y: f32,
    pub(super) force_y_values: bool,
    pub(super) x_labels: u16,
    pub(super) y_labels: u16,
}

impl UiGraphPresentationPolicy {
    pub(super) fn from_authored_graph_definition(
        graph_type: &Option<InformationGraphType>,
        forced_minimum_y: f32,
        forced_maximum_y: f32,
        force_y_values: bool,
        x_labels: i32,
        y_labels: i32,
    ) -> Self {
        Self {
            graph_type: *graph_type,
            forced_minimum_y,
            forced_maximum_y,
            force_y_values,
            x_labels: clamp_authored_graph_label_count(x_labels, 1),
            y_labels: clamp_authored_graph_label_count(y_labels, 2),
        }
    }

    pub(super) const fn graph_type(self) -> Option<InformationGraphType> {
        self.graph_type
    }
}

fn clamp_authored_graph_label_count(value: i32, minimum: u16) -> u16 {
    u16::try_from(value.clamp(i32::from(minimum), i32::from(u16::MAX))).unwrap_or(minimum)
}

/// Marks presentation entities generated beneath an authored graph node.
#[derive(Component)]
pub(super) struct UiGeneratedGraphPresentationElement;
