//! Photo zoom changes the authored frustum slopes, never the camera position.

use super::{
    camera_runtime_state_types::{CameraDefinition, CameraIntent, ZooCamera},
    fixed_aspect_perspective_projection::FixedAspectPerspectiveProjection,
};
use crate::assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use crate::assets::world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions;
use crate::assets::world_definitions::world_definition_document_asset_and_demand_loaded_dependency_paths::WorldDefinitionAsset;
use crate::plugins::{
    input::input_types::{
        ActionSource, ActiveInputDevice, DeviceControlAxes, PrimaryPointerInputState,
    },
    photos::photo_capture_types::PhotoMode,
    ui::authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiValue},
};
use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::document::UiDocumentRole,
    world_definitions::immersive_mode_policy::ImmersiveModeKind,
};

#[derive(Component)]
pub(super) struct PhotoCameraZoom {
    scale: f32,
    displayed_percent: i64,
}

pub(super) fn apply_photo_camera_zoom(
    mut commands: Commands,
    time: Res<Time<Real>>,
    pointer: Res<PrimaryPointerInputState>,
    axes: Res<DeviceControlAxes>,
    active_input: Res<ActiveInputDevice>,
    definitions: Res<Assets<WorldDefinitionAsset>>,
    active_definitions: Res<WorldDefinitions>,
    documents: Res<Assets<UiDocumentAsset>>,
    roots: Query<&UiDocumentRoot>,
    mut sliders: Query<(&Name, &UiDocumentOwner, &mut UiValue)>,
    inactive_cameras: Query<Entity, (With<PhotoCameraZoom>, Without<PhotoMode>)>,
    mut cameras: Query<
        (
            Entity,
            Ref<CameraDefinition>,
            Ref<PhotoMode>,
            &CameraIntent,
            &mut Projection,
            Option<&mut PhotoCameraZoom>,
        ),
        With<ZooCamera>,
    >,
) {
    for entity in &inactive_cameras {
        commands.entity(entity).remove::<PhotoCameraZoom>();
    }
    let Some(definitions) = active_definitions.get(&definitions) else {
        return;
    };
    let Some(policy) = definitions
        .immersive_mode_policies()
        .find(|policy| policy.mode == ImmersiveModeKind::Photo)
    else {
        return;
    };
    for (entity, selected, photo_mode, intent, mut projection, zoom) in &mut cameras {
        let Some(definition) = definitions.find_camera(selected.0) else {
            continue;
        };
        let minimum = definition.minimum_zoom_m;
        let maximum = definition.maximum_zoom_m[2];
        if !minimum.is_finite() || !maximum.is_finite() || minimum <= 0.0 || maximum <= minimum {
            continue;
        }
        let mut initial = PhotoCameraZoom {
            scale: maximum,
            displayed_percent: 0,
        };
        let needs_initialization = zoom.is_none();
        let mut zoom = zoom;
        let state = zoom.as_deref_mut().unwrap_or(&mut initial);
        if photo_mode.is_added() {
            state.scale = maximum;
            state.displayed_percent = 0;
        }
        let previous = state.scale;
        for (name, owner, value) in &mut sliders {
            if name.as_str() != "ZoomSlider" {
                continue;
            }
            let Some(document) = roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
            else {
                continue;
            };
            if document.canonical_ui_document().role != UiDocumentRole::PhotoMode {
                continue;
            }
            if value.0 != state.displayed_percent && !photo_mode.is_added() {
                state.scale = maximum - (maximum - minimum) * value.0.clamp(0, 100) as f32 / 100.0;
            }
        }
        let wheel = if pointer.wheel_y == 0.0 {
            0.0
        } else {
            pointer.wheel_y.signum()
        };
        let held_command =
            f32::from(intent.photo_zoom_in_command) - f32::from(intent.photo_zoom_out_command);
        let controller = if matches!(active_input.source, ActionSource::Controller(_)) {
            axes.zoom
        } else {
            0.0
        };
        let held_delta = (held_command - controller) * 10.0 * time.delta_secs();
        state.scale =
            (state.scale + policy.photo_zoom_step * (held_delta - wheel)).clamp(minimum, maximum);
        state.displayed_percent =
            ((maximum - state.scale) / (maximum - minimum) * 100.0).round() as i64;
        for (name, owner, mut value) in &mut sliders {
            if name.as_str() != "ZoomSlider" {
                continue;
            }
            let Some(document) = roots
                .get(owner.0)
                .ok()
                .and_then(|root| documents.get(&root.document))
            else {
                continue;
            };
            if document.canonical_ui_document().role == UiDocumentRole::PhotoMode
                && value.0 != state.displayed_percent
            {
                value.0 = state.displayed_percent;
            }
        }
        if previous != state.scale
            || photo_mode.is_added()
            || needs_initialization
            || selected.is_changed()
        {
            let fov = 2.0 * ((definition.fov_y_radians * 0.5).tan() * state.scale).atan();
            *projection = Projection::custom(FixedAspectPerspectiveProjection::new(
                fov,
                definition.fixed_aspect_ratio,
                definition.near_m,
                definition.far_m,
            ));
        }
        if needs_initialization {
            commands.entity(entity).insert(initial);
        }
    }
}
