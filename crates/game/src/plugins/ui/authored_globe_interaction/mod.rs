use super::authored_globe_presentation_types::GlobeSelectedMarker;
use super::authored_globe_presentation_types::GlobeSelectionTurn;
use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::UiGlobeRenderScene;
use super::authored_globe_rotation_geometry::constrained_globe_angles;
use super::authored_globe_rotation_geometry::globe_alignment_angles;
use super::authored_globe_rotation_geometry::globe_pitch_after_pointer_drag;
use super::authored_globe_rotation_geometry::globe_rotation;
use super::authored_globe_rotation_geometry::shortest_angle_delta;
use crate::plugins::input::input_types::ActionRequest;
use crate::plugins::input::input_types::ActiveInputDevice;
use crate::plugins::input::input_types::GameAction;
use crate::plugins::shell::shell_selection_types::ShellSelection;
use crate::plugins::shell::shell_selection_types::WorldChoiceView;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeDrag;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerAnchor;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::plugins::ui::authored_globe_presentation_types::UiGlobePresentation;
use crate::plugins::ui::authored_ui_activation_contracts::UiNodeActivated;
use crate::plugins::ui::authored_ui_selection_state::UiSelected;
use crate::plugins::world_spawn::prefab_presentation_types::PrefabModel;
use bevy::gltf::Gltf;
use bevy::prelude::*;
use bevy::ui::UiGlobalTransform;
use bevy::window::PrimaryWindow;
use openzt2_game_data::ui_document::action::UiTrigger;

pub(in crate::plugins::ui) fn interact_with_globe(
    active_input: Res<ActiveInputDevice>,
    windows: Query<&Window, With<PrimaryWindow>>,
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    mut globes: Query<
        (&ComputedNode, &UiGlobalTransform, &mut UiGlobeDrag),
        With<UiGlobePresentation>,
    >,
    cameras: Query<(&Camera, &GlobalTransform), With<UiGlobeCamera>>,
    models: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<bevy::gltf::GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    marker_geometry: Query<(
        &WorldChoiceView,
        &UiGlobeMarkerVisual,
        &PrefabModel,
        &GlobalTransform,
        &InheritedVisibility,
    )>,
    anchors: Query<(Entity, &WorldChoiceView, &GlobalTransform), With<UiGlobeMarkerAnchor>>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    if primary_pointer.just_released {
        for (_, _, mut drag) in &mut globes {
            drag.active = false;
        }
    }
    if !primary_pointer.just_pressed {
        return;
    }
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(pointer) = window.cursor_position() else {
        return;
    };
    let physical_pointer = pointer * window.resolution.scale_factor();
    let Some((_, _, mut drag)) = globes
        .iter_mut()
        .find(|(node, transform, _)| node.contains_point(**transform, physical_pointer))
    else {
        return;
    };
    let Some((ray, camera_position)) = cameras.iter().find_map(|(camera, transform)| {
        camera
            .viewport_to_world(transform, pointer)
            .ok()
            .map(|ray| (ray, transform.translation()))
    }) else {
        return;
    };
    let picked = marker_geometry
        .iter()
        .filter(|(_, visual, _, _, visible)| !visual.selected && visible.get())
        .filter_map(|(view, _, prefab, transform, _)| {
            let (anchor_entity, _, anchor) =
                anchors.iter().find(|(_, choice, _)| choice.0 == view.0)?;
            // The anchor's +Z axis is the globe's outward surface normal.
            let outward = anchor.affine().transform_vector3(Vec3::Z);
            if outward.dot(camera_position - anchor.translation()) <= 0.0 {
                return None;
            }
            let model = models.get(prefab.handle.as_ref()?)?;
            crate::assets::model::model_asset_queries::loaded_gltf_model_aggregate_local_bounds(
                model,
                &gltf_meshes,
                &meshes,
            )
            .into_iter()
            .filter_map(|(minimum, maximum)| ray_box_distance(&ray, transform, minimum, maximum))
            .min_by(f32::total_cmp)
            .map(|distance| (distance, anchor_entity))
        })
        .min_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, anchor)| anchor);
    drag.active = picked.is_none();
    if let Some(node) = picked {
        activated.write(UiNodeActivated {
            source: active_input.source,
            node,
            trigger: UiTrigger::Press,
        });
    }
}

fn ray_box_distance(
    ray: &Ray3d,
    transform: &GlobalTransform,
    minimum: Vec3,
    maximum: Vec3,
) -> Option<f32> {
    let inverse = transform.affine().inverse();
    let origin = inverse.transform_point3(ray.origin);
    let direction = inverse.transform_vector3(*ray.direction);
    let (near, far) = (0..3).try_fold((0.0_f32, f32::INFINITY), |(near, far), axis| {
        let origin = origin[axis];
        let direction = direction[axis];
        if direction.abs() <= f32::EPSILON {
            (origin >= minimum[axis] && origin <= maximum[axis]).then_some((near, far))
        } else {
            let first = (minimum[axis] - origin) / direction;
            let second = (maximum[axis] - origin) / direction;
            Some((near.max(first.min(second)), far.min(first.max(second))))
        }
    })?;
    (far >= near.max(0.0)).then_some(near.max(0.0))
}

/// Turns the selected map location toward the camera over the authored move time.
pub(in crate::plugins::ui) fn target_selected_globe_marker(
    mut commands: Commands,
    selection: Res<ShellSelection>,
    globes: Query<(&UiGlobePresentation, &UiGlobeRenderScene)>,
    selected: Query<(Entity, &UiSelected), With<UiGlobeMarkerAnchor>>,
    targets: Query<&GlobeSelectedMarker>,
    mut transforms: ParamSet<(
        Query<&Transform>,
        Query<&Transform, With<UiGlobeCamera>>,
        Query<&mut Transform>,
    )>,
) {
    let Some((presentation, scene)) = globes.iter().next() else {
        return;
    };
    let Some(anchor_entity) = selected
        .iter()
        .find_map(|(entity, selected)| selected.0.then_some(entity))
    else {
        commands
            .entity(scene.world)
            .remove::<GlobeSelectedMarker>()
            .remove::<GlobeSelectionTurn>();
        return;
    };
    if targets
        .get(scene.world)
        .is_ok_and(|target| target.0 == anchor_entity)
        && !selection.is_changed()
    {
        return;
    }
    let Ok(anchor_rotation) = transforms
        .p0()
        .get(anchor_entity)
        .map(|anchor| anchor.rotation)
    else {
        return;
    };
    let Some(camera_rotation) = transforms.p1().iter().next().map(|camera| camera.rotation) else {
        return;
    };
    let mut worlds = transforms.p2();
    let Ok(world) = worlds.get_mut(scene.world) else {
        return;
    };

    // Marker anchors rotate source -Y, converted to Bevy +Z, onto the
    // location normal. The camera's local +Z points back towards the camera.
    // Aligning those bases centres the selected location without embedding a
    // longitude correction or any knowledge of an individual map.
    let marker_normal = anchor_rotation * Vec3::Z;
    let camera_hemisphere = camera_rotation * Vec3::Z;
    let (from_yaw, from_pitch) = constrained_globe_angles(world.rotation);
    let (target_yaw, to_pitch) =
        globe_alignment_angles(marker_normal, camera_hemisphere, from_yaw, from_pitch);
    let to_yaw = from_yaw + shortest_angle_delta(from_yaw, target_yaw);
    commands.entity(scene.world).insert((
        GlobeSelectedMarker(anchor_entity),
        GlobeSelectionTurn {
            from_yaw,
            from_pitch,
            to_yaw,
            to_pitch,
            elapsed_seconds: 0.0,
            duration_seconds: presentation.move_seconds.max(f32::EPSILON),
        },
    ));
}

pub(in crate::plugins::ui) fn turn_globe_to_selection(
    mut commands: Commands,
    time: Res<Time>,
    mut worlds: Query<(Entity, &mut Transform, &mut GlobeSelectionTurn)>,
) {
    for (entity, mut transform, mut turn) in &mut worlds {
        turn.elapsed_seconds =
            (turn.elapsed_seconds + time.delta_secs()).min(turn.duration_seconds);
        let progress = turn.elapsed_seconds / turn.duration_seconds;
        let yaw = turn.from_yaw.lerp(turn.to_yaw, progress);
        let pitch = turn.from_pitch.lerp(turn.to_pitch, progress);
        transform.rotation = globe_rotation(yaw, pitch);
        if progress >= 1.0 {
            commands.entity(entity).remove::<GlobeSelectionTurn>();
        }
    }
}

pub(in crate::plugins::ui) fn rotate_globe(
    mut commands: Commands,
    primary_pointer: Res<crate::plugins::input::input_types::PrimaryPointerInputState>,
    mut actions: MessageReader<ActionRequest>,
    globes: Query<(&UiGlobeDrag, &UiGlobePresentation, &UiGlobeRenderScene)>,
    mut transforms: Query<&mut Transform>,
) {
    for (drag, presentation, scene) in &globes {
        let pointer_delta = (drag.active && primary_pointer.pressed)
            .then_some(primary_pointer.delta * presentation.mouse_increment);
        let action_delta = actions.read().fold(0.0, |delta, request| {
            delta
                + match request.action {
                    GameAction::RotateLeft | GameAction::NavigateLeft => -1.0,
                    GameAction::RotateRight | GameAction::NavigateRight => 1.0,
                    _ => 0.0,
                }
        });
        if pointer_delta.is_none() && action_delta == 0.0 {
            continue;
        }
        let Some(mut transform) = transforms.get_mut(scene.world).ok() else {
            continue;
        };
        let (mut yaw, mut pitch) = constrained_globe_angles(transform.rotation);
        if let Some(delta) = pointer_delta {
            commands.entity(scene.world).remove::<GlobeSelectionTurn>();
            // The original treats a rightward hand drag as positive yaw. Keep
            // pitch on the fixed world X axis so diagonal/circular drags can
            // never accumulate camera-axis roll.
            yaw += delta.x;
            // Screen Y increases downward. From the +Z camera, positive
            // X rotation moves the front surface downward as well.
            pitch = globe_pitch_after_pointer_drag(pitch, delta.y);
        }
        if action_delta != 0.0 {
            commands.entity(scene.world).remove::<GlobeSelectionTurn>();
            yaw += action_delta * presentation.selection_rotate_speed;
        }
        transform.rotation = globe_rotation(yaw, pitch);
    }
}
