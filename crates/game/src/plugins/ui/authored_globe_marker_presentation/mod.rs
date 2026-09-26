use super::authored_globe_camera_framing::transformed_model_bounds;
use super::authored_globe_presentation_types::UiGlobeMarkerProjected;
use super::authored_globe_presentation_types::UiGlobePrefab;
use super::authored_globe_presentation_types::UiGlobePrimaryRenderable;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_rotation_geometry::source_rotation_to_bevy;
use crate::plugins::shell::shell_selection_types::GlobeMarker;
use crate::plugins::shell::shell_selection_types::WorldChoiceView;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerAnchor;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobePresentation;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeSceneAssets;
use crate::plugins::ui::authored_ui_node_projection_components::UiDocumentOwner;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabModel;
use bevy::gltf::Gltf;
use bevy::prelude::*;

pub(in crate::plugins::ui) fn project_globe_markers(
    mut commands: Commands,
    models: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    globes: Query<(
        Entity,
        &UiGlobePresentation,
        &UiGlobeSceneAssets,
        &UiGlobeRenderScene,
        &UiDocumentOwner,
    )>,
    prefabs: Query<(&PrefabModel, &GlobalTransform), With<UiGlobePrimaryRenderable>>,
    transforms: Query<&GlobalTransform>,
    markers: Query<(Entity, &GlobeMarker), Without<UiGlobeMarkerProjected>>,
) {
    let Some((_globe_entity, globe, scene_assets, scene, owner)) = globes.iter().next() else {
        return;
    };
    let Ok(world_transform) = transforms.get(scene.world) else {
        return;
    };
    let Some((center, radius)) = transformed_model_bounds(
        &models,
        &gltf_meshes,
        &meshes,
        &prefabs,
        Some(world_transform),
    ) else {
        return;
    };
    for (choice_entity, marker) in &markers {
        let longitude = f32::from(marker.0[0]).to_radians() / 100.0;
        let latitude = f32::from(marker.0[1]).to_radians() / 100.0;
        // The refresh composes the complete source X(latitude) and
        // Z(-longitude) matrices. Their stored Gamebryo convention maps the
        // flag model's source -Y axis to +Z for positive latitude and +X for
        // positive longitude. Preserve that complete basis rather than
        // reducing it to a radial direction: the latter loses the authored
        // roll used by both flag prefabs.
        let marker_rotation = source_rotation_to_bevy(latitude, longitude);
        let outward = marker_rotation * Vec3::Z;
        let rendered_marker = commands
            .spawn((
                Name::new("map location marker anchor"),
                UiGlobeMarkerAnchor,
                UiSelected(false),
                WorldChoiceView(choice_entity),
                *owner,
                // world-scenario catalog stores the direction on the authored globe, not a
                // model-space position. This entity is an ECS transform
                // anchor only; the visible/pickable dot belongs to Bevy UI so
                // it shares the window's pointer coordinate space.
                // The ordinary marker prefab is authored at source Y=-25,
                // which model conversion maps to local +Z. Align that radial
                // axis with this location's globe normal.
                Transform::from_translation(center + outward * radius)
                    .with_rotation(marker_rotation),
                Visibility::Inherited,
                ChildOf(scene.world),
            ))
            .id();
        if let Some(handle) = scene_assets.dot.clone() {
            commands.spawn((
                Name::new("map location marker dot"),
                UiGlobePrefab {
                    handle,
                    primary: false,
                },
                UiGlobeMarkerVisual { selected: false },
                WorldChoiceView(choice_entity),
                *owner,
                // The normal flag is built around source Y=-25 while the
                // Earth mesh has an authored radius of 25 after NIF scaling.
                // Move the prefab root back to the globe centre; the model's
                // own offset then places the flag exactly on the surface.
                Transform::from_translation(
                    marker_rotation.inverse()
                        * (globe.dot_translation - globe.primary_translation - outward * radius),
                ),
                Visibility::Inherited,
                ChildOf(rendered_marker),
            ));
        }
        if let Some(handle) = scene_assets.selected_dot.clone() {
            commands.spawn((
                Name::new("selected map location marker dot"),
                UiGlobePrefab {
                    handle,
                    primary: false,
                },
                UiGlobeMarkerVisual { selected: true },
                WorldChoiceView(choice_entity),
                *owner,
                Transform::from_translation(
                    marker_rotation.inverse()
                        * (globe.selected_dot_translation
                            - globe.primary_translation
                            - outward * radius),
                ),
                Visibility::Hidden,
                ChildOf(rendered_marker),
            ));
        }
        commands
            .entity(choice_entity)
            .insert(UiGlobeMarkerProjected);
    }
}

pub(in crate::plugins::ui) fn present_globe_marker_selection(
    anchors: Query<(&WorldChoiceView, &UiSelected), With<UiGlobeMarkerAnchor>>,
    mut visuals: Query<
        (&WorldChoiceView, &UiGlobeMarkerVisual, &mut Visibility),
        Without<PrefabModel>,
    >,
) {
    for (view, selected) in &anchors {
        for (visual_view, visual, mut visibility) in &mut visuals {
            if visual_view.0 != view.0 {
                continue;
            }
            let requested = if !visual.selected || selected.0 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != requested {
                *visibility = requested;
            }
        }
    }
}
