use bevy::prelude::*;
use openzt2_game_assets::{
    material::runtime::effect_pass_gpu_data::D3d9ScopedPrefabLight, scene_prefab::ScenePrefabAsset,
};
use openzt2_game_data::scene_prefab::{PrefabLight, PrefabLightKind, ScenePrefabEntityFlags};

use super::world_entity_model_suppression::ModelRenderVisibilityGate;

#[derive(Component)]
#[relationship(relationship_target = ScopedPrefabLights)]
pub(super) struct ScopedPrefabLightOf(Entity);

#[derive(Component)]
#[relationship_target(relationship = ScopedPrefabLightOf, linked_spawn)]
pub(super) struct ScopedPrefabLights(Vec<Entity>);

#[derive(Component)]
pub(super) struct ScopedPrefabLight(PrefabLight);

#[derive(Component, PartialEq)]
pub(super) struct ScopedPrefabLightInputs(pub(super) Vec<D3d9ScopedPrefabLight>);

pub(crate) fn spawn_authored_scene_lights(
    commands: &mut Commands,
    prefab: &ScenePrefabAsset,
    entities: &[Entity],
) -> Vec<Entity> {
    let mut lights = Vec::new();
    for (index, row) in prefab
        .canonical_scene_prefab_document()
        .entities
        .iter()
        .enumerate()
    {
        if row.flags.0 & ScenePrefabEntityFlags::ACTIVE.0 == 0 {
            continue;
        }
        for light in &row.lights {
            let source = entities[index];
            if light.kind == PrefabLightKind::Point
                && light.affected_rows.is_none()
                && light.attenuation.is_none()
            {
                lights.push(
                    commands
                        .spawn((
                            ModelRenderVisibilityGate,
                            ChildOf(source),
                            PointLight {
                                color: Color::srgb(
                                    light.color_srgb[0],
                                    light.color_srgb[1],
                                    light.color_srgb[2],
                                ),
                                intensity: light.intensity,
                                range: light.range_m,
                                shadow_maps_enabled: false,
                                ..default()
                            },
                        ))
                        .id(),
                );
                continue;
            }
            let default_target = [if light.kind == PrefabLightKind::Point {
                0
            } else {
                index as u32
            }];
            let targets = light.affected_rows.as_deref().unwrap_or(&default_target);
            for target in targets
                .iter()
                .filter_map(|&index| entities.get(index as usize))
            {
                lights.push(
                    commands
                        .spawn((
                            ScopedPrefabLight(light.clone()),
                            ScopedPrefabLightOf(*target),
                            ModelRenderVisibilityGate,
                            Transform::IDENTITY,
                            Visibility::Inherited,
                            ChildOf(source),
                        ))
                        .id(),
                );
            }
        }
    }
    lights
}

pub(super) fn project_scoped_prefab_lighting(
    mut commands: Commands,
    changed_lights: Query<
        &ScopedPrefabLightOf,
        Or<(
            Changed<ScopedPrefabLight>,
            Changed<GlobalTransform>,
            Changed<InheritedVisibility>,
        )>,
    >,
    changed_scopes: Query<Entity, Changed<ScopedPrefabLights>>,
    mut removed_scopes: RemovedComponents<ScopedPrefabLights>,
    lights: Query<(&ScopedPrefabLight, &GlobalTransform, &InheritedVisibility)>,
    mut targets: Query<(
        Entity,
        &ScopedPrefabLights,
        Option<&mut ScopedPrefabLightInputs>,
    )>,
) {
    for entity in removed_scopes
        .read()
        .filter(|&entity| !targets.contains(entity))
    {
        commands
            .entity(entity)
            .try_remove::<ScopedPrefabLightInputs>();
    }
    let affected = changed_scopes
        .iter()
        .chain(changed_lights.iter().map(|light| light.0))
        .collect::<bevy::ecs::entity::EntityHashSet>();
    let mut targets = targets.iter_many_mut(affected).matched();
    while let Some((entity, members, current)) = targets.fetch_next() {
        let next = ScopedPrefabLightInputs(
            lights
                .iter_many(&members.0)
                .matched()
                .filter(|(_, _, visible)| visible.get())
                .map(|(light, pose, _)| {
                    let light = &light.0;
                    let [r, g, b] = light.color_srgb;
                    let color = Vec4::from_array(Color::srgb(r, g, b).to_linear().to_f32_array())
                        .truncate()
                        * light.intensity;
                    match light.kind {
                        PrefabLightKind::Ambient => D3d9ScopedPrefabLight::Ambient { color },
                        PrefabLightKind::Directional => D3d9ScopedPrefabLight::Directional {
                            direction_to_light: pose.back().into(),
                            color,
                        },
                        PrefabLightKind::Point => D3d9ScopedPrefabLight::Point {
                            position: pose.translation(),
                            color,
                            range: light.range_m,
                            attenuation: light.attenuation.unwrap_or([1.0, 0.0, 0.0]),
                        },
                    }
                })
                .collect(),
        );
        if let Some(mut current) = current {
            current.set_if_neq(next);
        } else {
            commands.entity(entity).try_insert(next);
        }
    }
}
