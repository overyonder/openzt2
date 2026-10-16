use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use openzt2_world_state::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;

pub(super) fn project_prefab_ambient_light_contributions(
    contributions: Query<(
        &PrefabAmbientLightContribution,
        Option<&RenderLayers>,
        Option<&InheritedVisibility>,
    )>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let linear = contributions
        .iter()
        .filter(|(_, layers, visibility)| {
            !layers.is_some_and(|layers| *layers == RenderLayers::none())
                && visibility.is_none_or(|visibility| visibility.get())
        })
        .fold(Vec3::ZERO, |sum, (value, _, _)| {
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
