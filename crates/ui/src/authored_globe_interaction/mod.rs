use super::authored_globe_model_bounds::UiGlobeModelBounds;
use super::authored_globe_presentation_types::GlobeSelectedMarker;
use super::authored_globe_presentation_types::GlobeSelectionTurn;
use super::authored_globe_presentation_types::UiGlobeCamera;
use super::authored_globe_presentation_types::{
    GlobeHoveredMarker, GlobeSpin, UiGlobeHostedSceneEntities, UiGlobeRenderScene,
};
use super::authored_globe_rotation_geometry::constrained_globe_angles;
use super::authored_globe_rotation_geometry::globe_alignment_angles;
use super::authored_globe_rotation_geometry::globe_pitch_after_pointer_drag;
use super::authored_globe_rotation_geometry::globe_rotation;
use super::authored_globe_rotation_geometry::shortest_angle_delta;
use crate::authored_globe_presentation_types::UiGlobeDrag;
use crate::authored_globe_presentation_types::UiGlobeMarkerAnchor;
use crate::authored_globe_presentation_types::UiGlobeMarkerVisual;
use crate::authored_globe_presentation_types::UiGlobePresentation;
use bevy::ecs::system::SystemParam;
use bevy::{
    ecs::entity::EntityHashMap,
    picking::{
        events::{PointerCancel, PointerDrag, PointerPress, PointerRelease},
        hover::PointerCaptureMap,
        pointer::{PointerButton, PointerId, PointerPressState},
    },
    prelude::*,
};
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::action::UiTrigger;
use openzt2_interface_state::input::input_types::ActionRequest;
use openzt2_interface_state::input::input_types::ActiveInputDevice;
use openzt2_interface_state::input::input_types::GameAction;
use openzt2_interface_state::input::input_types::MousePointer;
use openzt2_interface_state::shell::shell_selection_types::ShellSelection;
use openzt2_interface_state::ui::authored_ui_activation_contracts::UiNodeActivated;
use openzt2_interface_state::ui::authored_ui_selection_state::UiSelected;
use openzt2_interface_state::ui::{
    authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    pointer_capture_types::UiPointerCapture,
};
use openzt2_world_state::world_spawn::prefab_model_tint::PrefabModelTint;
use openzt2_zoo_state::audio::audio_playback_message_types::PlayAudioClip;

pub(crate) fn interact_with_globe(
    active_input: Res<ActiveInputDevice>,
    mut presses: MessageReader<PointerPress>,
    mut releases: MessageReader<PointerRelease>,
    mut cancellations: MessageReader<PointerCancel>,
    pointer_buttons: Query<(&PointerId, &PointerPressState)>,
    mut capture: ResMut<PointerCaptureMap>,
    mut globes: Query<(&UiGlobeRenderScene, &mut UiGlobeDrag)>,
    picking: GlobePicking,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    modal: Res<openzt2_interface_state::ui::input_frame_capture_types::AuthoredModalInputCapture>,
    mut activated: MessageWriter<UiNodeActivated>,
) {
    let still_pressed = pointer_buttons
        .iter()
        .any(|(id, state)| id.is_mouse() && state.is_primary_pressed());
    let released = releases
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
        .count()
        != 0;
    let cancelled = cancellations
        .read()
        .filter(|event| event.pointer.id.is_mouse())
        .count()
        != 0;
    let enabled = |globe| {
        context.visible_and_enabled(globe)
            && modal
                .0
                .is_none_or(|modal| context.is_in_modal(globe, modal))
    };
    if let Some((captured, _)) = capture.get(&PointerId::Mouse) {
        if globes.contains(captured) && !enabled(captured) {
            capture.release(PointerId::Mouse);
            if let Ok((_, mut drag)) = globes.get_mut(captured) {
                drag.active = false;
            }
        }
    }
    if released || cancelled || !still_pressed {
        for (_, mut drag) in &mut globes {
            if drag.active {
                drag.active = false;
            }
        }
    }
    for event in presses
        .read()
        .filter(|event| event.pointer.id.is_mouse() && event.button == PointerButton::Primary)
    {
        let globe = event.entity;
        let Ok((scene, mut drag)) = globes.get_mut(globe) else {
            continue;
        };
        if !enabled(globe) {
            continue;
        }
        let picked = picking.marker(globe, scene.world(), event.pointer.position);
        drag.active = picked.is_none() && still_pressed && !cancelled;
        if let Some(node) = picked {
            activated.write(UiNodeActivated {
                source: active_input.source,
                node,
                trigger: UiTrigger::Press,
            });
        } else if drag.active {
            capture.capture(event.pointer.id, globe, event.hit.clone());
        }
    }
}

#[derive(SystemParam)]
pub(super) struct GlobePicking<'w, 's> {
    hosted: Query<'w, 's, &'static UiGlobeHostedSceneEntities>,
    cameras: Query<'w, 's, (&'static Camera, &'static GlobalTransform), With<UiGlobeCamera>>,
    geometry: Query<
        'w,
        's,
        (
            &'static UiGlobeMarkerVisual,
            &'static UiGlobeModelBounds,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    anchors: Query<'w, 's, &'static GlobalTransform, With<UiGlobeMarkerAnchor>>,
    children: Query<'w, 's, &'static Children>,
}

impl GlobePicking<'_, '_> {
    fn marker(&self, globe: Entity, world: Entity, position: Vec2) -> Option<Entity> {
        let (camera, pose) = self
            .hosted
            .get(globe)
            .ok()?
            .iter()
            .find_map(|entity| self.cameras.get(entity).ok())?;
        if !camera.is_active {
            return None;
        }
        let ray = camera.viewport_to_world(pose, position).ok()?;
        let camera_position = pose.translation();
        self.children
            .get(world)
            .ok()?
            .iter()
            .filter_map(|anchor| {
                let pose = self.anchors.get(anchor).ok()?;
                let outward = pose.affine().transform_vector3(Vec3::Z);
                if outward.dot(camera_position - pose.translation()) <= 0.0 {
                    return None;
                }
                let distance = self
                    .children
                    .iter_descendants::<Children>(anchor)
                    .filter_map(|geometry| {
                        let (visual, bounds, transform, visible) =
                            self.geometry.get(geometry).ok()?;
                        if visual.selected || !visible.get() {
                            return None;
                        }
                        ray_box_distance(&ray, transform, bounds.minimum, bounds.maximum)
                    })
                    .min_by(f32::total_cmp)?;
                Some((distance, anchor))
            })
            .min_by(|(left, _), (right, _)| left.total_cmp(right))
            .map(|(_, anchor)| anchor)
    }
}

pub(super) fn highlight_hovered_globe_marker(
    capture: Res<UiPointerCapture>,
    pointer: MousePointer,
    context: super::active_authored_ui_context::ActiveAuthoredUiContext,
    picking: GlobePicking,
    mut globes: Query<(
        Entity,
        &UiGlobeRenderScene,
        &UiGlobeDrag,
        &UiGlobePresentation,
        &mut GlobeHoveredMarker,
    )>,
    mut tints: Query<&mut PrefabModelTint>,
) {
    for (globe, scene, drag, presentation, mut hovered) in &mut globes {
        let next = pointer
            .location()
            .filter(|_| {
                capture.target == Some(globe) && !drag.active && context.visible_and_enabled(globe)
            })
            .and_then(|location| picking.marker(globe, scene.world(), location.position));
        if hovered.0 == next {
            continue;
        }
        let [r, g, b, a] = presentation
            .dot_highlight_rgba
            .map(|byte| f32::from(byte) / 255.0);
        for (anchor, color) in [(hovered.0, Color::WHITE), (next, Color::srgba(r, g, b, a))] {
            let Some(anchor) = anchor else { continue };
            for model in picking.children.iter_descendants::<Children>(anchor) {
                if let Ok(mut tint) = tints.get_mut(model) {
                    tint.set_if_neq(PrefabModelTint(color));
                }
            }
        }
        hovered.0 = next;
    }
}

pub(super) fn play_globe_spin_sound(
    time: Res<Time>,
    roots: Query<&UiDocumentRoot>,
    documents: Res<Assets<UiDocumentAsset>>,
    mut globes: Query<(&UiDocumentOwner, &UiGlobePresentation, &mut GlobeSpin)>,
    mut sounds: MessageWriter<PlayAudioClip>,
) {
    for (owner, policy, mut spin) in &mut globes {
        if policy.scream_threshold <= 0.0 || spin.velocity.length() < policy.scream_threshold {
            if spin.above_threshold.elapsed_secs() > 0.0 {
                spin.above_threshold.reset();
                spin.sounded = false;
            }
            continue;
        }
        if spin.sounded {
            continue;
        }
        spin.above_threshold.tick(time.delta());
        if spin.above_threshold.elapsed_secs() < policy.scream_delay_seconds {
            continue;
        }
        if let Some(clip) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| documents.get(&root.document))
            .and_then(|document| document.audio_source_handle(policy.scream_audio))
        {
            spin.sounded = true;
            sounds.write(PlayAudioClip {
                clip: clip.clone(),
                emitter: None,
                selection: 0,
                force_looped: false,
            });
        }
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
pub(crate) fn target_selected_globe_marker(
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
            .entity(scene.world())
            .remove::<GlobeSelectedMarker>()
            .remove::<GlobeSelectionTurn>();
        return;
    };
    if targets
        .get(scene.world())
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
    let Ok(world) = worlds.get_mut(scene.world()) else {
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
    commands.entity(scene.world()).insert((
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

pub(crate) fn turn_globe_to_selection(
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

pub(crate) fn rotate_globe(
    mut commands: Commands,
    mut drags: MessageReader<PointerDrag>,
    time: Res<Time>,
    mut actions: MessageReader<ActionRequest>,
    mut globes: Query<(
        Entity,
        &UiGlobeDrag,
        &UiGlobePresentation,
        &UiGlobeRenderScene,
        &mut GlobeSpin,
    )>,
    turning: Query<(), With<GlobeSelectionTurn>>,
    mut transforms: Query<&mut Transform>,
) {
    let mut pointer_motion = EntityHashMap::<Vec2>::default();
    for drag in drags.read() {
        if drag.pointer.id.is_mouse() && drag.button == PointerButton::Primary {
            *pointer_motion.entry(drag.entity).or_default() += drag.delta;
        }
    }
    let action_delta = actions.read().fold(0.0, |delta, request| {
        delta
            + match request.action {
                GameAction::RotateLeft | GameAction::NavigateLeft => -1.0,
                GameAction::RotateRight | GameAction::NavigateRight => 1.0,
                _ => 0.0,
            }
    });
    let seconds = time.delta_secs();
    for (entity, drag, presentation, scene, mut spin) in &mut globes {
        let world = scene.world();
        let pointer_delta = pointer_motion
            .get(&entity)
            .filter(|_| drag.active)
            .map(|delta| *delta * presentation.mouse_increment);
        if drag.active {
            if spin.release_time.elapsed_secs() > 0.0 {
                spin.release_time.reset();
            }
        } else if spin.velocity != Vec2::ZERO {
            spin.release_time.tick(time.delta());
        }
        if (turning.contains(world) || action_delta != 0.0) && pointer_delta.is_none() {
            if spin.velocity != Vec2::ZERO {
                spin.velocity = Vec2::ZERO;
            }
        }
        let down = presentation.mouse_down_friction.clamp(0.0, 1.0);
        let up = presentation.mouse_up_friction.clamp(0.0, 1.0);
        let friction = if drag.active {
            down
        } else {
            let blend = (spin.release_time.elapsed_secs()
                / presentation.friction_transition_seconds.max(f32::EPSILON))
            .min(1.0);
            down.lerp(up, blend)
        };
        let decay = (1.0 - friction).powf(seconds * 60.0);
        let motion = if let Some(delta) = pointer_delta {
            if seconds > 0.0 {
                spin.velocity = (spin.velocity * decay + delta / seconds * (1.0 - decay))
                    .clamp_length_max(50.0);
            }
            delta
        } else if spin.velocity != Vec2::ZERO {
            spin.velocity *= decay;
            if spin.velocity.length_squared() < 0.0001 || (!drag.active && up == 0.0 && down == 0.0)
            {
                spin.velocity = Vec2::ZERO;
            }
            // Keep drag motion governed by pointer displacement. The
            // filtered speed is the input to the globe's sound policy.
            Vec2::ZERO
        } else {
            Vec2::ZERO
        };
        if motion == Vec2::ZERO && action_delta == 0.0 {
            continue;
        }
        let Some(mut transform) = transforms.get_mut(scene.world()).ok() else {
            continue;
        };
        let (mut yaw, mut pitch) = constrained_globe_angles(transform.rotation);
        if pointer_delta.is_some() {
            commands
                .entity(scene.world())
                .remove::<GlobeSelectionTurn>();
            // The original treats a rightward hand drag as positive yaw. Keep
            // pitch on the fixed world X axis so diagonal/circular drags can
            // never accumulate camera-axis roll.
            yaw += motion.x;
            // Screen Y increases downward. From the +Z camera, positive
            // X rotation moves the front surface downward as well.
            pitch = globe_pitch_after_pointer_drag(pitch, motion.y);
        }
        if action_delta != 0.0 {
            commands
                .entity(scene.world())
                .remove::<GlobeSelectionTurn>();
            yaw += action_delta * presentation.selection_rotate_speed;
        }
        transform.rotation = globe_rotation(yaw, pitch);
    }
}
