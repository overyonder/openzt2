use super::authored_globe_camera_framing::GlobeSphere;
use super::authored_globe_presentation_types::UiGlobePrefab;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_rotation_geometry::source_rotation_to_bevy;
use crate::authored_globe_presentation_types::UiGlobeMarkerAnchor;
use crate::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::authored_globe_presentation_types::UiGlobePresentation;
use crate::authored_globe_presentation_types::UiGlobeSceneAssets;
use bevy::prelude::*;
use openzt2_interface_state::shell::shell_selection_types::GlobeMarker;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::shell::shell_selection_types::{
    WorldChoiceMarkerView, WorldChoiceMarkers,
};
use openzt2_interface_state::ui::authored_ui_node_projection_components::UiDocumentOwner;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_world_state::world_spawn::prefab_presentation_types::PrefabModel;

pub(crate) fn project_globe_markers(
    mut commands: Commands,
    globes: Query<(
        Entity,
        &UiGlobePresentation,
        &UiGlobeSceneAssets,
        &UiGlobeRenderScene,
        &UiDocumentOwner,
    )>,
    spheres: Query<&GlobeSphere>,
    markers: Query<(Entity, &GlobeMarker), Without<WorldChoiceMarkers>>,
) {
    if markers.is_empty() {
        return;
    }
    let Some((_globe_entity, globe, scene_assets, scene, owner)) = globes.iter().next() else {
        return;
    };
    let Ok(&GlobeSphere { center, radius }) = spheres.get(scene.world()) else {
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
                WorldChoiceMarkerView(choice_entity),
                UiSelected(false),
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
                ChildOf(scene.world()),
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
    }
}

/// The campaign globe marks only the selected scenario's location; the other
/// modes flag every map and ring the selected one.
pub(crate) fn present_globe_marker_selection(
    selection: Option<Res<ShellSelection>>,
    anchors: Query<(Ref<UiSelected>, &Children), With<UiGlobeMarkerAnchor>>,
    mut visuals: Query<(&UiGlobeMarkerVisual, &mut Visibility), Without<PrefabModel>>,
) {
    let campaign = selection.as_ref().is_some_and(|selection| {
        selection.mode == Some(openzt2_world_state::game_session_types::WorldSessionMode::Campaign)
    });
    for (selected, children) in &anchors {
        if !selected.is_changed()
            && !selection
                .as_ref()
                .is_some_and(|selection| selection.is_changed())
        {
            continue;
        }
        for child in children {
            if let Ok((visual, mut visibility)) = visuals.get_mut(*child) {
                visibility.set_if_neq(if selected.0 || !(visual.selected || campaign) {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
    }
}
