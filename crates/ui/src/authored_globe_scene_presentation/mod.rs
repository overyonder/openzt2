use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobePrefab;
use super::authored_globe_presentation_types::UiGlobePrimaryRenderable;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_presentation_types::UiGlobeSceneOwner;
use super::authored_globe_presentation_types::{
    UiGlobeBiomeVisual, UiGlobeBiomeVisuals, UiGlobeHostedSceneEntities, UiGlobeWorldOf,
};
use crate::authored_globe_presentation_types::UiGlobeBiomeAsset;
use crate::authored_globe_presentation_types::UiGlobeBiomeModel;
use crate::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::authored_globe_presentation_types::UiGlobePresentation;
use crate::authored_globe_presentation_types::UiGlobeSceneAssets;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use openzt2_game_assets::scene_prefab::ScenePrefabAsset;
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_visual_types::UiVisualLayer;
use openzt2_world::world_spawn::prefab_presentation_render_tree::spawn_prefab_render_tree;

const GLOBE_RENDER_LAYER: usize = 31;

pub(crate) fn hydrate_globe_models(
    mut commands: Commands,
    globes: Query<
        (Entity, &UiGlobePresentation, &UiGlobeSceneAssets),
        Without<UiGlobeHostedSceneEntities>,
    >,
    biome_models: Query<
        (
            Entity,
            &UiGlobeBiomeModel,
            &UiGlobeBiomeAsset,
            &Visibility,
            &ChildOf,
        ),
        (Without<UiGlobeBiomeVisuals>, Without<UiGlobePrefab>),
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
                openzt2_world_state::world_spawn::prefab_fixed_function_world_lighting_policy::PrefabFixedFunctionWorldLightingPolicy {
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
                UiGlobeWorldOf(entity),
            ))
            .id();
        let mut has_primary = false;
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
            commands.spawn((
                Name::new(name),
                UiGlobePrefab {
                    handle,
                    primary: is_primary,
                },
                Transform::from_translation(translation - globe.primary_translation),
                Visibility::Inherited,
                ChildOf(world),
            ));
            has_primary |= is_primary;
        }
        if has_primary {
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
                UiGlobeCamera,
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
            ChildOf(scene.world()),
            UiGlobeBiomeVisual(entity),
        ));
    }
}

pub(crate) fn project_globe_prefabs(
    mut commands: Commands,
    prefabs: Res<Assets<ScenePrefabAsset>>,
    requests: Query<(
        Entity,
        &UiGlobePrefab,
        &InheritedVisibility,
        &ChildOf,
        Option<&UiGlobeMarkerVisual>,
        Option<&UiDocumentOwner>,
    )>,
) {
    for (entity, request, inherited, parent, marker, owner) in &requests {
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
            let globe_world = parent.parent();
            for renderable in &renderables {
                commands
                    .entity(*renderable)
                    .insert(UiGlobePrimaryRenderable(globe_world));
            }
        }
        if let Some(marker) = marker.copied() {
            for renderable in &renderables {
                let mut entity = commands.entity(*renderable);
                entity.insert(marker);
                if !marker.selected {
                    entity.insert(
                        openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint(
                            Color::WHITE,
                        ),
                    );
                }
                if let Some(owner) = owner.copied() {
                    entity.insert(owner);
                }
            }
        }
        commands.entity(entity).remove::<UiGlobePrefab>();
    }
}
