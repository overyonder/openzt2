pub(crate) fn interpret_authored_animation_graph_edge_blend_duration_milliseconds(
    authored_edge_attributes: &[openzt2_game_data::animation::animation_set::AuthoredAnimationAttribute],
) -> u16 {
    authored_edge_attributes
        .iter()
        .find(|authored_attribute| {
            authored_attribute
                .attribute_name
                .eq_ignore_ascii_case("blend")
        })
        .and_then(|authored_attribute| {
            authored_attribute
                .attribute_value
                .trim()
                .parse::<f32>()
                .ok()
        })
        .map_or(0, |authored_blend_duration_seconds| {
            (authored_blend_duration_seconds * 1000.0)
                .round()
                .clamp(0.0, f32::from(u16::MAX)) as u16
        })
}
