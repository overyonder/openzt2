use bevy::prelude::*;

use crate::assets::localization::loaded_localization_queries::LoadedLocalizationView;

use super::graph_presentation_types::UiGeneratedGraphPresentationElement;

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

pub(super) fn despawn_generated_graph_presentation_elements(
    commands: &mut Commands,
    graph: Entity,
    elements: &Query<(Entity, &ChildOf), With<UiGeneratedGraphPresentationElement>>,
) {
    for (entity, parent) in elements {
        if parent.parent() == graph {
            commands.entity(entity).despawn();
        }
    }
}

pub(super) fn spawn_colored_graph_geometry_element(
    commands: &mut Commands,
    parent: Entity,
    rectangle: GeneratedGraphElementRectangle,
    transform: UiTransform,
) {
    commands.spawn((
        UiGeneratedGraphPresentationElement,
        ChildOf(parent),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(rectangle.left),
            bottom: Val::Px(rectangle.bottom),
            width: Val::Px(rectangle.width),
            height: Val::Px(rectangle.height),
            ..default()
        },
        transform,
        BackgroundColor(Color::srgb(0.72, 0.92, 0.31)),
        Pickable::IGNORE,
    ));
}

pub(super) fn spawn_graph_line_segment(
    commands: &mut Commands,
    parent: Entity,
    start_x: f32,
    start_y: f32,
    end_x: f32,
    end_y: f32,
) {
    let delta = Vec2::new(end_x - start_x, end_y - start_y);
    let length = delta.length();
    let midpoint = Vec2::new(start_x, start_y) + delta * 0.5;
    spawn_colored_graph_geometry_element(
        commands,
        parent,
        GeneratedGraphElementRectangle {
            left: midpoint.x - length * 0.5,
            bottom: midpoint.y - LINE_THICKNESS * 0.5,
            width: length,
            height: LINE_THICKNESS,
        },
        UiTransform::from_rotation(Rot2::radians(-delta.y.atan2(delta.x))),
    );
}

// The ratio is deliberately reduced to Bevy's f32 UI coordinate space.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn spawn_numeric_graph_y_axis_labels(
    commands: &mut Commands,
    parent: Entity,
    count: u16,
    minimum_value: f64,
    maximum_value: f64,
    plot_height: f32,
) {
    let intervals = f64::from(count - 1);
    for index in 0..count {
        let ratio = f64::from(index) / intervals;
        let value = minimum_value + (maximum_value - minimum_value) * ratio;
        let label = if value.fract().abs() < f64::EPSILON {
            format!("{value:.0}")
        } else {
            format!("{value:.1}")
        };
        spawn_graph_text_label(
            commands,
            parent,
            &label,
            0.0,
            PLOT_BOTTOM + ratio as f32 * plot_height - 6.0,
            PLOT_LEFT - 4.0,
            Justify::Right,
        );
    }
}

// The ratio is deliberately reduced to Bevy's f32 UI coordinate space.
#[allow(clippy::cast_possible_truncation)]
pub(super) fn spawn_localized_currency_graph_y_axis_labels(
    commands: &mut Commands,
    parent: Entity,
    count: u16,
    minimum_value: f64,
    maximum_value: f64,
    plot_height: f32,
    localization: LoadedLocalizationView<'_>,
) {
    let intervals = f64::from(count - 1);
    for index in 0..count {
        let ratio = f64::from(index) / intervals;
        let cents = minimum_value + (maximum_value - minimum_value) * ratio;
        let mut label = String::new();
        #[allow(clippy::cast_possible_truncation)]
        let cents = cents.round() as i64;
        if localization
            .write_localized_currency_amount(cents, false, &mut label)
            .is_err()
        {
            continue;
        }
        spawn_graph_text_label(
            commands,
            parent,
            &label,
            0.0,
            PLOT_BOTTOM + ratio as f32 * plot_height - 6.0,
            PLOT_LEFT - 4.0,
            Justify::Right,
        );
    }
}

pub(super) fn spawn_graph_text_label(
    commands: &mut Commands,
    parent: Entity,
    label: &str,
    left: f32,
    bottom: f32,
    width: f32,
    justify: Justify,
) {
    commands.spawn((
        UiGeneratedGraphPresentationElement,
        ChildOf(parent),
        Text::new(label),
        TextFont {
            font_size: FontSize::Px(10.0),
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::justify(justify),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(left),
            bottom: Val::Px(bottom),
            width: Val::Px(width),
            ..default()
        },
        Pickable::IGNORE,
    ));
}
