use super::conversion_error::ConversionError;

pub(super) fn normalize_blue_fang_rotation_quaternion(
    mut rotation_quaternion: [f32; 4],
) -> Result<[f32; 4], ConversionError> {
    let quaternion_length = rotation_quaternion
        .iter()
        .map(|component| component * component)
        .sum::<f32>()
        .sqrt();
    if !quaternion_length.is_finite() {
        return Err(ConversionError::InvalidValue("non-finite BF quaternion"));
    }
    if quaternion_length < 0.0001 {
        return Ok([0.0, 0.0, 0.0, 1.0]);
    }
    rotation_quaternion
        .iter_mut()
        .for_each(|component| *component /= quaternion_length);
    Ok(rotation_quaternion)
}

pub(super) fn convert_blue_fang_xyz_euler_angles_to_quaternion(
    x_axis_radians: f32,
    y_axis_radians: f32,
    z_axis_radians: f32,
) -> [f32; 4] {
    let (x_half_angle_sine, x_half_angle_cosine) = (x_axis_radians * 0.5).sin_cos();
    let (y_half_angle_sine, y_half_angle_cosine) = (y_axis_radians * 0.5).sin_cos();
    let (z_half_angle_sine, z_half_angle_cosine) = (z_axis_radians * 0.5).sin_cos();
    [
        x_half_angle_sine * y_half_angle_cosine * z_half_angle_cosine
            + x_half_angle_cosine * y_half_angle_sine * z_half_angle_sine,
        x_half_angle_cosine * y_half_angle_sine * z_half_angle_cosine
            - x_half_angle_sine * y_half_angle_cosine * z_half_angle_sine,
        x_half_angle_cosine * y_half_angle_cosine * z_half_angle_sine
            + x_half_angle_sine * y_half_angle_sine * z_half_angle_cosine,
        x_half_angle_cosine * y_half_angle_cosine * z_half_angle_cosine
            - x_half_angle_sine * y_half_angle_sine * z_half_angle_sine,
    ]
}
