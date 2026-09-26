use bevy::prelude::*;

use crate::plugins::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;

pub(super) fn project_prefab_ambient_light_contributions(
    contributions: Query<&PrefabAmbientLightContribution>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let linear = contributions.iter().fold(Vec3::ZERO, |sum, value| {
        sum + value.color_linear * value.intensity
    });
    let brightness = linear.max_element();
    let color = if brightness > f32::EPSILON {
        linear / brightness
    } else {
        Vec3::ZERO
    };
    let color = Color::linear_rgb(color.x, color.y, color.z);
    if ambient.color != color || ambient.brightness != brightness {
        ambient.color = color;
        ambient.brightness = brightness;
    }
}
