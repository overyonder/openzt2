use bevy::prelude::*;

use openzt2_zoo_state::environment::environment_presentation_types::EnvironmentSpecularLightColor;

#[derive(Component, Default, PartialEq)]
pub(super) struct LinearWorldAmbient(pub(super) Vec4);

#[derive(Component, PartialEq)]
pub(super) struct DirectionalWorldLightInput {
    pub(super) back: Vec3,
    pub(super) color: Vec4,
    pub(super) specular: Vec4,
    pub(super) illuminance: f32,
}

pub(super) fn publish_linear_world_ambient(
    ambient: Res<GlobalAmbientLight>,
    mut input: Single<&mut LinearWorldAmbient>,
) {
    if ambient.is_changed() || input.is_added() {
        let color = Vec4::from_array(ambient.color.to_linear().to_f32_array());
        input.set_if_neq(LinearWorldAmbient(color * ambient.brightness));
    }
}

pub(super) fn publish_directional_world_light_inputs(
    mut commands: Commands,
    mut removed_specular: RemovedComponents<EnvironmentSpecularLightColor>,
    mut removed_lights: RemovedComponents<DirectionalLight>,
    changed: Query<
        Entity,
        (
            With<DirectionalLight>,
            Or<(
                Changed<DirectionalLight>,
                Changed<GlobalTransform>,
                Changed<EnvironmentSpecularLightColor>,
            )>,
        ),
    >,
    mut lights: Query<(
        Entity,
        &DirectionalLight,
        &GlobalTransform,
        Option<&EnvironmentSpecularLightColor>,
        Option<&mut DirectionalWorldLightInput>,
    )>,
) {
    for entity in removed_lights
        .read()
        .filter(|&entity| !lights.contains(entity))
    {
        commands
            .entity(entity)
            .try_remove::<DirectionalWorldLightInput>();
    }
    let affected = changed
        .iter()
        .chain(removed_specular.read())
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let mut affected = lights.iter_many_mut(affected).matched();
    while let Some((entity, light, transform, specular, input)) = affected.fetch_next() {
        let color = Vec4::from_array(light.color.to_linear().to_f32_array());
        let input_value = DirectionalWorldLightInput {
            color,
            specular: specular.map_or(color, |specular| {
                Vec4::from_array(specular.0.to_linear().to_f32_array())
            }),
            back: transform.back().into(),
            illuminance: light.illuminance,
        };
        if let Some(mut input) = input {
            input.set_if_neq(input_value);
        } else {
            commands.entity(entity).try_insert(input_value);
        }
    }
}
