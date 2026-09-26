use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobeModelsHydrated;
use super::authored_globe_presentation_types::UiGlobePrefab;
use super::authored_globe_presentation_types::UiGlobePrimaryRenderable;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_presentation_types::UiGlobeSceneOwner;
use crate::assets::scene_prefab::ScenePrefabAsset;
use crate::plugins::shell::shell_selection_types::WorldChoiceView;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeBiomeAsset;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeBiomeModel;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobePresentation;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeSceneAssets;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_visual_types::UiVisualLayer;
use crate::plugins::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::prelude::*;

const GLOBE_RENDER_LAYER: usize = 31;

pub(in crate::plugins::ui) fn hydrate_globe_models(
    mut commands: Commands,
    globes: Query<
        (Entity, &UiGlobePresentation, &UiGlobeSceneAssets),
        Without<UiGlobeModelsHydrated>,
    >,
    biome_models: Query<
        (
            Entity,
            &UiGlobeBiomeModel,
            &UiGlobeBiomeAsset,
            &Visibility,
            &ChildOf,
        ),
        (Without<UiGlobeModelsHydrated>, Without<UiGlobePrefab>),
    >,
    scenes: Query<&UiGlobeRenderScene>,
    children: Query<&Children>,
    visual_layers: Query<(), With<UiVisualLayer>>,
) {
    for (entity, globe, scene_assets) in &globes {
        let scene = commands
            .spawn((
                Name::new("map selection globe scene"),
                Transform::IDENTITY,
                Visibility::Visible,
                RenderLayers::layer(GLOBE_RENDER_LAYER),
                UiGlobeSceneOwner(entity),
                crate::plugins::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy {
                    enabled: false,
                    preserve_authored_material_lighting: true,
                },
            ))
            .id();
        let world = commands
            .spawn((
                Name::new("map selection globe world"),
                Transform::from_translation(globe.primary_translation),
                Visibility::Inherited,
                RenderLayers::layer(GLOBE_RENDER_LAYER),
                ChildOf(scene),
            ))
            .id();
        let mut primary = None;
        for (name, handle, translation, is_primary) in [
            (
                "globe",
                scene_assets.primary.as_ref(),
                globe.primary_translation,
                true,
            ),
            (
                "clouds",
                scene_assets.clouds.as_ref(),
                globe.clouds_translation,
                false,
            ),
        ] {
            let Some(handle) = handle.cloned() else {
                continue;
            };
            let model = commands
                .spawn((
                    Name::new(name),
                    UiGlobePrefab {
                        handle,
                        primary: is_primary,
                    },
                    Transform::from_translation(translation - globe.primary_translation),
                    Visibility::Inherited,
                    ChildOf(world),
                ))
                .id();
            if is_primary {
                primary = Some(model);
            }
        }
        if let Some(model) = primary {
            commands.spawn((
                Name::new("map selection globe camera"),
                Camera3d::default(),
                Camera {
                    // The shell UI camera first draws the authored backdrop.
                    // Confine this later pass to the globe viewport so the
                    // authored 3D model appears inside that UI surface.
                    order: 150,
                    // The shared intermediate already contains the shell
                    // backdrop and the globe's authored material blending.
                    // Copy that result once: blending its destination alpha
                    // again makes even the opaque Earth show the backdrop.
                    clear_color: ClearColorConfig::None,
                    ..default()
                },
                Transform::from_xyz(0.0, 0.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
                UiGlobeCamera { model },
                UiGlobeSceneOwner(entity),
                RenderLayers::layer(GLOBE_RENDER_LAYER),
            ));
        }
        if let Ok(children) = children.get(entity) {
            for child in children
                .iter()
                .filter(|child| visual_layers.contains(*child))
            {
                commands.entity(child).despawn();
            }
        }
        commands
            .entity(entity)
            .insert((UiGlobeRenderScene { world }, UiGlobeModelsHydrated));
    }
    for (entity, biome, asset, visibility, parent) in &biome_models {
        let Ok(scene) = scenes.get(parent.parent()) else {
            continue;
        };
        commands.spawn((
            Name::new("globe biome overlay"),
            *biome,
            UiGlobePrefab {
                handle: asset.0.clone(),
                primary: false,
            },
            Transform::IDENTITY,
            *visibility,
            ChildOf(scene.world),
        ));
        commands.entity(entity).insert(UiGlobeModelsHydrated);
    }
}

pub(in crate::plugins::ui) fn project_globe_prefabs(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    requests: Query<(
        Entity,
        &UiGlobePrefab,
        &InheritedVisibility,
        Option<&UiGlobeMarkerVisual>,
        Option<&WorldChoiceView>,
        Option<&UiDocumentOwner>,
    )>,
    mut cameras: Query<(Entity, &mut UiGlobeCamera)>,
) {
    for (entity, request, inherited, marker, choice, owner) in &requests {
        if !inherited.get() {
            continue;
        }
        let Some(prefab) = prefabs.get(&request.handle) else {
            continue;
        };
        let (_, renderables, lights) =
            spawn_prefab_render_tree(&mut commands, prefab, entity, request.primary);
        for renderable in &renderables {
            commands
                .entity(*renderable)
                .insert(RenderLayers::layer(GLOBE_RENDER_LAYER));
        }
        for light in lights {
            commands
                .entity(light)
                .insert(RenderLayers::layer(GLOBE_RENDER_LAYER));
        }
        if request.primary {
            for renderable in &renderables {
                commands
                    .entity(*renderable)
                    .insert(UiGlobePrimaryRenderable);
            }
            if let Some(model) = renderables.first().copied() {
                for (camera_entity, mut camera) in &mut cameras {
                    camera.model = model;
                    if let Some(light) = prefab
                        .canonical_scene_prefab_document()
                        .entities
                        .iter()
                        .flat_map(|entity| &entity.lights)
                        .find(|light| {
                            light.kind == openzt2_game_data::scene_prefab::PrefabLightKind::Ambient
                        })
                    {
                        commands.entity(camera_entity).insert(AmbientLight {
                            color: Color::srgb(
                                light.color_srgb[0],
                                light.color_srgb[1],
                                light.color_srgb[2],
                            ),
                            // Native materials use the D3D fixed-function
                            // ambient colour directly, without PBR exposure
                            // or a Lambertian illuminance conversion.
                            brightness: light.intensity,
                            affects_lightmapped_meshes: true,
                        });
                    }
                }
            }
        }
        if let Some(choice) = choice.copied() {
            for renderable in &renderables {
                let mut entity = commands.entity(*renderable);
                entity.insert(choice);
                if let Some(marker) = marker.copied() {
                    entity.insert(marker);
                }
                if let Some(owner) = owner.copied() {
                    entity.insert(owner);
                }
            }
        }
        commands.entity(entity).remove::<UiGlobePrefab>();
    }
}

pub(in crate::plugins::ui) fn cleanup_globe_scenes(
    mut commands: Commands,
    scenes: Query<(Entity, &UiGlobeSceneOwner)>,
    owners: Query<(), With<UiGlobePresentation>>,
) {
    for (entity, owner) in &scenes {
        if owners.get(owner.0).is_err() {
            commands.entity(entity).despawn();
        }
    }
}
