//! Photo zoom changes the authored frustum slopes without moving the camera.

use bevy::{ecs::relationship::RelationshipTarget, input::mouse::AccumulatedMouseScroll, prelude::*};
use openzt2_game_assets::graphics_settings_types::GraphicsSettings;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use openzt2_game_assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use openzt2_game_data::ui_document::document::UiDocumentRole;
use openzt2_game_data::world_definitions::immersive_mode_policy::ImmersiveModeKind;
use openzt2_interface_state::input::input_types::{ActionSource, ActiveInputDevice};
use openzt2_interface_state::photos::photo_capture_types::PhotoMode;
use openzt2_interface_state::ui::authored_ui_node_projection_components::{
    UiDocumentMembers, UiDocumentRoot, UiSliderValueChanged, UiValue,
};
use openzt2_interface_state::ui::pointer_capture_types::{UiPointerCapture, UiWheelCapture};

use super::camera_execution_types::CameraIntent;
use super::camera_runtime_state_types::{CameraDefinition, ZooCamera};
use super::fixed_aspect_perspective_projection::FixedAspectPerspectiveProjection;
use super::systems::overhead_camera_zoom_policy::select_three_level_graphics_detail_value;
use crate::input::input_execution_types::DeviceControlAxes;

#[derive(Component, PartialEq)]
pub(super) struct PhotoCameraZoom(f32);

#[derive(Component, PartialEq, Eq)]
#[relationship(relationship_target = PhotoZoomSliders)]
pub(super) struct PhotoZoomSliderFor(Entity);

#[derive(Component)]
#[relationship_target(relationship = PhotoZoomSliderFor)]
pub(super) struct PhotoZoomSliders(Vec<Entity>);

pub(super) fn bind_photo_zoom_sliders(
    mut commands: Commands,
    cameras: Query<(Entity, Ref<PhotoMode>), With<ZooCamera>>,
    roots: Query<(Ref<UiDocumentRoot>, Ref<UiDocumentMembers>)>,
    documents: Res<Assets<UiDocumentAsset>>,
    sliders: Query<(&Name, Option<&PhotoZoomSliderFor>), With<UiValue>>,
) {
    let Ok((camera, mode)) = cameras.single() else {
        return;
    };
    for (root, members) in &roots {
        if !mode.is_added() && !root.is_changed() && !members.is_changed() {
            continue;
        }
        let is_photo_document = documents.get(&root.document).is_some_and(|document| {
            document.canonical_ui_document().role == UiDocumentRole::PhotoMode
        });
        if !is_photo_document {
            continue;
        }
        for slider in members.iter() {
            if sliders.get(slider).is_ok_and(|(name, binding)| {
                name.as_str() == "ZoomSlider" && binding != Some(&PhotoZoomSliderFor(camera))
            }) {
                commands.entity(slider).insert(PhotoZoomSliderFor(camera));
            }
        }
    }
}

pub(super) fn apply_photo_camera_zoom(
    mut commands: Commands,
    time: Res<Time<Real>>,
    scroll: Res<AccumulatedMouseScroll>,
    capture: Res<UiPointerCapture>,
    axes: Res<DeviceControlAxes>,
    active_input: Res<ActiveInputDevice>,
    definition_assets: Res<Assets<WorldDefinitionAsset>>,
    graphics: Res<GraphicsSettings>,
    active_definitions: Res<WorldDefinitions>,
    mut slider_input: MessageReader<UiSliderValueChanged>,
    slider_bindings: Query<&PhotoZoomSliderFor>,
    mut slider_values: Query<&mut UiValue>,
    inactive_cameras: Query<Entity, (With<PhotoCameraZoom>, Without<PhotoMode>)>,
    mut cameras: Query<
        (
            Entity,
            Ref<CameraDefinition>,
            Ref<PhotoMode>,
            &CameraIntent,
            &mut Projection,
            Option<&mut PhotoCameraZoom>,
            Option<Ref<PhotoZoomSliders>>,
        ),
        With<ZooCamera>,
    >,
) {
    for entity in &inactive_cameras {
        commands.entity(entity).remove::<PhotoCameraZoom>();
    }
    let (
        Some(definitions),
        Ok((camera, selected, photo_mode, intent, mut projection, zoom, sliders)),
    ) = (
        active_definitions.get(&definition_assets),
        cameras.single_mut(),
    )
    else {
        slider_input.clear();
        return;
    };
    let (Some(definition), Some(policy)) = (
        definitions.find_camera(selected.0),
        definitions
            .immersive_mode_policies()
            .find(|policy| policy.mode == ImmersiveModeKind::Photo),
    ) else {
        slider_input.clear();
        return;
    };
    let minimum = definition.minimum_zoom_m;
    let maximum = definition.maximum_zoom_m[2];
    if !minimum.is_finite() || !maximum.is_finite() || minimum <= 0.0 || maximum <= minimum {
        slider_input.clear();
        return;
    }
    let initialize = photo_mode.is_added() || zoom.is_none();
    let previous = zoom.as_ref().map_or(maximum, |zoom| zoom.0);
    let slider_percent = slider_input
        .read()
        .filter_map(|input| {
            (slider_bindings
                .get(input.slider)
                .is_ok_and(|binding| binding.0 == camera))
            .then_some(input.value)
        })
        .last();
    let scale = if initialize {
        maximum
    } else if let Some(percent) = slider_percent {
        maximum - (maximum - minimum) * percent.clamp(0, 100) as f32 / 100.0
    } else {
        previous
    };
    let wheel = if capture.wheel == UiWheelCapture::World {
        scroll.delta.y.signum() * f32::from(scroll.delta.y != 0.0)
    } else {
        0.0
    };
    let held_command =
        f32::from(intent.photo_zoom_in_command) - f32::from(intent.photo_zoom_out_command);
    let controller = if matches!(active_input.source, ActionSource::Controller(_)) {
        axes.zoom
    } else {
        0.0
    };
    let held_delta = (held_command - controller) * 10.0 * time.delta_secs();
    let scale = (scale + policy.photo_zoom_step * (held_delta - wheel)).clamp(minimum, maximum);
    let zoom_changed = initialize || scale != previous;
    let definition_changed =
        selected.is_changed() || active_definitions.is_changed() || definition_assets.is_changed();

    if let Some(mut zoom) = zoom {
        zoom.set_if_neq(PhotoCameraZoom(scale));
    } else {
        commands.entity(camera).insert(PhotoCameraZoom(scale));
    }
    if zoom_changed || definition_changed {
        let fov = 2.0 * ((definition.fov_y_radians * 0.5).tan() * scale).atan();
        *projection = Projection::custom(FixedAspectPerspectiveProjection::new(
            fov,
            definition.fixed_aspect_ratio,
            definition.near_m,
            select_three_level_graphics_detail_value(definition.far_m, graphics.view_distance),
        ));
    }
    if let Some(sliders) = sliders.filter(|sliders| {
        zoom_changed || definition_changed || slider_percent.is_some() || sliders.is_changed()
    }) {
        let percent = ((maximum - scale) / (maximum - minimum) * 100.0).round() as i64;
        for slider in sliders.iter() {
            if let Ok(mut value) = slider_values.get_mut(slider) {
                value.set_if_neq(UiValue(percent));
            }
        }
    }
}
