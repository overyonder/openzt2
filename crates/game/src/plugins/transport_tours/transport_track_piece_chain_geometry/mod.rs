use super::transport_fact_validation::authored_transport_track_piece_chain_is_valid;
use crate::assets::source_coordinate_conversion::convert_source_z_up_vector_to_bevy_y_up_coordinates;
use crate::plugins::economy::money_types::Money;
use bevy::prelude::*;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackDefinition;
use openzt2_game_data::world_definitions::transportation_and_tours::TransportationTrackKind;

pub(super) fn build_authored_axis_and_diagonal_transport_track_piece_chain(
    definition: &TransportationTrackDefinition,
    from_position: Vec3,
    to_position: Vec3,
) -> Option<(Vec<Vec3>, Money)> {
    const TRACK_ALIGNMENT_EPSILON_METRES: f32 = 0.001;

    if definition.kind == TransportationTrackKind::Sky {
        let maximum_distance = definition.sky_maximum_connection_distance_metres?;
        let distance = from_position.distance(to_position);
        return (distance.is_finite()
            && distance > TRACK_ALIGNMENT_EPSILON_METRES
            && distance <= maximum_distance
            && definition.purchase_cost_cents >= 0)
            .then(|| {
                (
                    vec![from_position, to_position],
                    Money(definition.purchase_cost_cents),
                )
            });
    }

    let source_endpoint_position = |source_position| {
        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
            source_position,
        ))
    };
    let horizontal_piece_span = |endpoint_offsets: [[f32; 3]; 2]| {
        (source_endpoint_position(endpoint_offsets[1])
            - source_endpoint_position(endpoint_offsets[0]))
        .xz()
        .length()
    };
    let horizontal_displacement = (to_position - from_position).xz();
    let cardinal_piece_span = horizontal_piece_span(definition.cardinal_endpoint_offsets_metres);
    let diagonal_piece_span = horizontal_piece_span(definition.diagonal_endpoint_offsets_metres);
    let diagonal_piece_component = diagonal_piece_span * core::f32::consts::FRAC_1_SQRT_2;
    if !horizontal_displacement.is_finite()
        || !cardinal_piece_span.is_finite()
        || cardinal_piece_span <= TRACK_ALIGNMENT_EPSILON_METRES
        || !diagonal_piece_component.is_finite()
        || diagonal_piece_component <= TRACK_ALIGNMENT_EPSILON_METRES
        || definition.purchase_cost_cents < 0
    {
        return None;
    }
    let absolute_horizontal_displacement = horizontal_displacement.abs();
    let diagonal_piece_count =
        (absolute_horizontal_displacement.min_element() / diagonal_piece_component).round();
    let remaining_horizontal_displacement = absolute_horizontal_displacement
        - Vec2::splat(diagonal_piece_count * diagonal_piece_component);
    let cardinal_piece_counts = (remaining_horizontal_displacement / cardinal_piece_span).round();
    let reconstructed_horizontal_displacement =
        Vec2::splat(diagonal_piece_count * diagonal_piece_component)
            + Vec2::new(
                cardinal_piece_counts.x * cardinal_piece_span,
                cardinal_piece_counts.y * cardinal_piece_span,
            );
    if remaining_horizontal_displacement.min_element() < -TRACK_ALIGNMENT_EPSILON_METRES
        || (reconstructed_horizontal_displacement - absolute_horizontal_displacement)
            .abs()
            .max_element()
            >= TRACK_ALIGNMENT_EPSILON_METRES
    {
        return None;
    }
    let piece_count = diagonal_piece_count + cardinal_piece_counts.x + cardinal_piece_counts.y;
    if piece_count < 1.0 || piece_count > f32::from(u16::MAX) {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let piece_count = piece_count as u16;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let diagonal_piece_count = diagonal_piece_count as u16;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let cardinal_piece_counts = [
        cardinal_piece_counts.x as u16,
        cardinal_piece_counts.y as u16,
    ];
    let total_horizontal_length = f32::from(diagonal_piece_count) * diagonal_piece_span
        + (f32::from(cardinal_piece_counts[0]) + f32::from(cardinal_piece_counts[1]))
            * cardinal_piece_span;
    if !total_horizontal_length.is_finite()
        || total_horizontal_length <= TRACK_ALIGNMENT_EPSILON_METRES
    {
        return None;
    }
    let horizontal_direction_signs = horizontal_displacement.signum();
    let mut path_points = Vec::with_capacity(usize::from(piece_count) + 1);
    path_points.push(from_position);
    let mut current_position = from_position;
    let mut accumulated_horizontal_length = 0.0;
    let mut append_piece = |horizontal_step: Vec2, horizontal_length: f32| {
        accumulated_horizontal_length += horizontal_length;
        current_position.x += horizontal_step.x;
        current_position.z += horizontal_step.y;
        current_position.y = from_position.y
            + (to_position.y - from_position.y)
                * (accumulated_horizontal_length / total_horizontal_length);
        path_points.push(current_position);
    };
    for _ in 0..diagonal_piece_count {
        append_piece(
            horizontal_direction_signs * diagonal_piece_component,
            diagonal_piece_span,
        );
    }
    for _ in 0..cardinal_piece_counts[0] {
        append_piece(
            Vec2::new(horizontal_direction_signs.x * cardinal_piece_span, 0.0),
            cardinal_piece_span,
        );
    }
    for _ in 0..cardinal_piece_counts[1] {
        append_piece(
            Vec2::new(0.0, horizontal_direction_signs.y * cardinal_piece_span),
            cardinal_piece_span,
        );
    }
    *path_points.last_mut()? = to_position;
    if !authored_transport_track_piece_chain_is_valid(definition, &path_points) {
        return None;
    }
    let cost = definition
        .purchase_cost_cents
        .checked_mul(i64::from(piece_count))
        .map(Money)?;
    Some((path_points, cost))
}

pub(super) fn build_projected_authored_axis_or_diagonal_transport_track_piece_chain(
    definition: &TransportationTrackDefinition,
    from_position: Vec3,
    pointer_position: Vec3,
) -> Option<(Vec<Vec3>, Money)> {
    const TRACK_ALIGNMENT_EPSILON_METRES: f32 = 0.001;

    if definition.kind == TransportationTrackKind::Sky {
        let maximum_distance = definition.sky_maximum_connection_distance_metres?;
        let displacement = pointer_position - from_position;
        let distance = displacement.length().min(maximum_distance);
        return (distance.is_finite()
            && distance > TRACK_ALIGNMENT_EPSILON_METRES
            && definition.purchase_cost_cents >= 0)
            .then(|| {
                (
                    vec![
                        from_position,
                        from_position + displacement.normalize() * distance,
                    ],
                    Money(definition.purchase_cost_cents),
                )
            });
    }

    let source_endpoint_position = |source_position| {
        Vec3::from_array(convert_source_z_up_vector_to_bevy_y_up_coordinates(
            source_position,
        ))
    };
    let horizontal_piece_span = |endpoint_offsets: [[f32; 3]; 2]| {
        (source_endpoint_position(endpoint_offsets[1])
            - source_endpoint_position(endpoint_offsets[0]))
        .xz()
        .length()
    };
    let cardinal_piece_span = horizontal_piece_span(definition.cardinal_endpoint_offsets_metres);
    let diagonal_piece_span = horizontal_piece_span(definition.diagonal_endpoint_offsets_metres);
    let horizontal_displacement = (pointer_position - from_position).xz();
    if !horizontal_displacement.is_finite()
        || horizontal_displacement.length() <= TRACK_ALIGNMENT_EPSILON_METRES
        || !cardinal_piece_span.is_finite()
        || cardinal_piece_span <= TRACK_ALIGNMENT_EPSILON_METRES
        || !diagonal_piece_span.is_finite()
        || diagonal_piece_span <= TRACK_ALIGNMENT_EPSILON_METRES
        || definition.purchase_cost_cents < 0
    {
        return None;
    }
    let normalized_displacement = horizontal_displacement.normalize();
    let cardinal_and_diagonal_directions = [
        Vec2::X,
        Vec2::ONE.normalize(),
        Vec2::Y,
        Vec2::new(-1.0, 1.0).normalize(),
        Vec2::NEG_X,
        Vec2::NEG_ONE.normalize(),
        Vec2::NEG_Y,
        Vec2::new(1.0, -1.0).normalize(),
    ];
    let direction = cardinal_and_diagonal_directions
        .into_iter()
        .max_by(|left, right| {
            left.dot(normalized_displacement)
                .total_cmp(&right.dot(normalized_displacement))
        })?;
    let direction_is_diagonal = direction.x != 0.0 && direction.y != 0.0;
    let piece_span = if direction_is_diagonal {
        diagonal_piece_span
    } else {
        cardinal_piece_span
    };
    let projected_distance = horizontal_displacement.dot(direction).max(piece_span);
    let piece_count = (projected_distance / piece_span)
        .round()
        .clamp(1.0, f32::from(u16::MAX));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let piece_count = piece_count as u16;
    let projected_end_horizontal =
        from_position.xz() + direction * piece_span * f32::from(piece_count);
    let projected_end_position = Vec3::new(
        projected_end_horizontal.x,
        pointer_position.y,
        projected_end_horizontal.y,
    );
    let mut path_points = Vec::with_capacity(usize::from(piece_count) + 1);
    path_points.push(from_position);
    for piece_index in 1..=piece_count {
        let interpolation = f32::from(piece_index) / f32::from(piece_count);
        let horizontal = from_position.xz() + direction * piece_span * f32::from(piece_index);
        path_points.push(Vec3::new(
            horizontal.x,
            from_position.y + (projected_end_position.y - from_position.y) * interpolation,
            horizontal.y,
        ));
    }
    if !authored_transport_track_piece_chain_is_valid(definition, &path_points) {
        return None;
    }
    let cost = definition
        .purchase_cost_cents
        .checked_mul(i64::from(piece_count))
        .map(Money)?;
    Some((path_points, cost))
}
