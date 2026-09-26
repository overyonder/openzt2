use bevy::{camera::visibility::RenderLayers, prelude::*};
use openzt2_game_data::world_definitions::environment::EnvironmentLightTarget;

use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::plugins::camera::camera_runtime_state_types::ZooCamera;
use crate::plugins::terrain::terrain_chunk_presentation_types::TERRAIN_RENDER_LAYER;
use crate::plugins::world_spawn::world_membership_types::WorldMember;

use super::{
    daylight_environment_sampling::collect_authored_environment_light_kind_ranks,
    environment_fog_attachment::insert_neutral_environment_fog,
    environment_presentation_types::{
        EnvironmentLight, EnvironmentPresentationPending, EnvironmentSky,
    },
    environment_state_types::WorldEnvironment,
    environment_visual_construction::{
        convert_authored_sky_keyframe_tint_to_bevy_color,
        create_environment_texture_renderer_components,
        hydrate_environment_visual_sample_renderer_entities,
    },
};

pub(super) fn hydrate_bevy_renderer_entities_for_new_world_environment(
    mut commands: Commands,
    environments: Query<
        (Entity, &WorldEnvironment, &WorldMember),
        With<EnvironmentPresentationPending>,
    >,
    cameras: Query<Entity, With<ZooCamera>>,
    lights: Query<(), With<EnvironmentLight>>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    for (environment_entity, environment, world_member) in &environments {
        let Some(definition) = definitions.find_environment(environment.definition) else {
            continue;
        };
        if lights.is_empty() {
            if !definition.light_keyframes.is_empty() {
                let light_entity = commands
                    .spawn((
                        EnvironmentLight {
                            keyframe: 0,
                            target: EnvironmentLightTarget::Object,
                            kind_rank: 0,
                        },
                        DirectionalLight::default(),
                        RenderLayers::from_layers(&[0, TERRAIN_RENDER_LAYER]),
                        Transform::IDENTITY,
                        world_member.clone(),
                    ))
                    .id();
                commands.entity(environment_entity).add_child(light_entity);
            } else {
                for light_target in [
                    EnvironmentLightTarget::Object,
                    EnvironmentLightTarget::Terrain,
                ] {
                    let (authored_light_ranks, light_count) =
                        collect_authored_environment_light_kind_ranks(
                            definitions,
                            definition,
                            light_target,
                        );
                    for light_kind_rank in authored_light_ranks.into_iter().take(light_count) {
                        let render_layers = match light_target {
                            EnvironmentLightTarget::Terrain => {
                                RenderLayers::layer(TERRAIN_RENDER_LAYER)
                            }
                            _ => RenderLayers::layer(0),
                        };
                        let light_entity = commands
                            .spawn((
                                EnvironmentLight {
                                    keyframe: 0,
                                    target: light_target,
                                    kind_rank: light_kind_rank,
                                },
                                DirectionalLight::default(),
                                render_layers,
                                Transform::IDENTITY,
                                world_member.clone(),
                            ))
                            .id();
                        commands.entity(environment_entity).add_child(light_entity);
                    }
                }
            }
        }

        if let Some(sky_keyframe) = definition.sky_keyframes.first() {
            let texture_identifier = sky_keyframe.texture;
            let mut sky_commands = commands.spawn((
                EnvironmentSky {
                    definition: texture_identifier,
                },
                Transform::IDENTITY,
                Visibility::Inherited,
                *world_member,
            ));
            if let Some(image) = definitions.texture_image(texture_identifier) {
                sky_commands.insert(create_environment_texture_renderer_components(
                    image,
                    convert_authored_sky_keyframe_tint_to_bevy_color(sky_keyframe),
                    &mut meshes,
                    &mut materials,
                ));
            }
            let sky_entity = sky_commands.id();
            commands.entity(environment_entity).add_child(sky_entity);
        }

        hydrate_environment_visual_sample_renderer_entities(
            &mut commands,
            environment_entity,
            world_member,
            definitions,
            definition,
            &mut meshes,
            &mut materials,
        );
        for camera in &cameras {
            insert_neutral_environment_fog(&mut commands, camera);
        }
    }
}
