use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::world_entity_model_suppression::ModelRenderingSuppressed;
use openzt2_world_state::world_spawn::prefab_ambient_light_contribution::PrefabAmbientLightContribution;

pub(super) fn project_prefab_ambient_light_contributions(
    contributions: Query<
        (
            &PrefabAmbientLightContribution,
            Option<&RenderLayers>,
            Option<&InheritedVisibility>,
        ),
        Without<ModelRenderingSuppressed>,
    >,
    changed: Query<
        (),
        (
            With<PrefabAmbientLightContribution>,
            Or<(
                Changed<PrefabAmbientLightContribution>,
                Changed<RenderLayers>,
                Changed<InheritedVisibility>,
            )>,
        ),
    >,
    mut removed_contributions: RemovedComponents<PrefabAmbientLightContribution>,
    mut removed_layers: RemovedComponents<RenderLayers>,
    mut removed_visibility: RemovedComponents<InheritedVisibility>,
    changed_suppression: Query<
        (),
        (
            With<PrefabAmbientLightContribution>,
            Added<ModelRenderingSuppressed>,
        ),
    >,
    mut removed_suppression: RemovedComponents<ModelRenderingSuppressed>,
    mut ambient: ResMut<GlobalAmbientLight>,
) {
    let removed = (removed_contributions.read().count() != 0)
        | removed_layers
            .read()
            .any(|entity| contributions.contains(entity))
        | removed_visibility
            .read()
            .any(|entity| contributions.contains(entity))
        | removed_suppression
            .read()
            .any(|entity| contributions.contains(entity));
    if !ambient.is_changed() && !removed && changed.is_empty() && changed_suppression.is_empty() {
        return;
    }
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
