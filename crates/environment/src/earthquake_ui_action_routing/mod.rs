use bevy::prelude::*;
use bevy::time::Stopwatch;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;
use openzt2_game_data::ui_document::{
    action::{presentation::UiPresentationAction, scenarios::UiScenarioAction, UiActionRecord},
    timed_presentation::UiTimedEventAction,
    widget::UiWidgetRecord,
};
use openzt2_game_data::AssetId;
use openzt2_interface_state::{
    camera::camera_runtime_state_types::{CameraMode, OverheadRig, ZooCamera},
    ui::{
        authored_ui_action_projection_components::{UiPresentationActions, UiScenarioActions},
        authored_ui_activation_contracts::UiNodeActivated,
        authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot},
    },
};

const EARTHQUAKE_DROPOFF_SECONDS: f64 = 5.6;

#[derive(Component, Clone, PartialEq)]
pub(super) struct EarthquakeCameraMotion {
    document: Entity,
    elapsed: Stopwatch,
    distance: f32,
    focus: Vec2,
}

fn is_earthquake_runner(document: &UiDocumentAsset, target: AssetId) -> bool {
    document.canonical_ui_document().nodes.iter().any(|node| {
        node.id == target
            && match &node.widget {
                UiWidgetRecord::TimedSequence { events, .. } => events.iter().any(|event| {
                    matches!(
                        &event.action,
                        UiTimedEventAction::Ui(UiActionRecord::Scenario(record))
                            if record.action == UiScenarioAction::RefreshEarthquake
                    )
                }),
                _ => false,
            }
    })
}

#[allow(clippy::type_complexity)]
pub(super) fn apply_authored_earthquake_motion_to_overhead_camera(
    mut commands: Commands,
    mut activations: MessageReader<UiNodeActivated>,
    document_assets: Res<Assets<UiDocumentAsset>>,
    routes: Query<(
        &UiDocumentOwner,
        Option<&UiScenarioActions>,
        Option<&UiPresentationActions>,
    )>,
    roots: Query<&UiDocumentRoot>,
    time: Res<Time<Real>>,
    mut cameras: Query<
        (
            Entity,
            &Camera,
            &CameraMode,
            &mut OverheadRig,
            Option<&mut EarthquakeCameraMotion>,
        ),
        With<ZooCamera>,
    >,
) {
    let active = cameras
        .iter()
        .find(|(_, camera, mode, _, _)| camera.is_active && **mode == CameraMode::Overhead)
        .map(|(entity, ..)| entity);
    let Some(camera) = active else {
        // A runner can keep issuing UI actions while the camera changes mode.
        for (entity, _, _, _, motion) in &cameras {
            if motion.is_some() {
                commands.entity(entity).remove::<EarthquakeCameraMotion>();
            }
        }
        activations.clear();
        return;
    };
    let Ok((_, _, _, mut rig, motion)) = cameras.get_mut(camera) else {
        return;
    };
    let mut next = motion
        .as_deref()
        .filter(|motion| roots.contains(motion.document))
        .cloned();
    if let Some(motion) = next.as_mut() {
        motion.elapsed.tick(time.delta());
    }
    for activation in activations.read() {
        let Ok((owner, scenario_actions, presentation_actions)) = routes.get(activation.node)
        else {
            continue;
        };
        let Some(document) = roots
            .get(owner.0)
            .ok()
            .and_then(|root| document_assets.get(&root.document))
        else {
            continue;
        };
        for action in scenario_actions
            .into_iter()
            .flat_map(|actions| actions.authored_action_records(document))
            .filter(|action| action.trigger == activation.trigger)
        {
            match action.action {
                UiScenarioAction::TriggerEarthquake => {
                    next = Some(EarthquakeCameraMotion {
                        document: owner.0,
                        elapsed: Stopwatch::new(),
                        distance: rig.distance,
                        focus: rig.focus,
                    });
                }
                UiScenarioAction::RefreshEarthquake => {
                    if let Some(motion) = next.as_ref().filter(|motion| motion.document == owner.0)
                    {
                        let elapsed = motion.elapsed.elapsed_secs_f64();
                        let remaining = (EARTHQUAKE_DROPOFF_SECONDS - elapsed).max(0.0);
                        let envelope =
                            (std::f64::consts::PI * remaining / EARTHQUAKE_DROPOFF_SECONDS).sin();
                        rig.reborrow()
                            .map_unchanged(|rig| &mut rig.distance)
                            .set_if_neq(
                                motion.distance + (2.0 * (elapsed * 0.5).sin() * envelope) as f32,
                            );
                        rig.reborrow()
                            .map_unchanged(|rig| &mut rig.focus)
                            .set_if_neq(
                                motion.focus
                                    + Vec2::X * (1.5 * (elapsed * 85.0).sin() * envelope) as f32,
                            );
                    }
                }
                _ => {}
            }
        }
        if next
            .as_ref()
            .is_some_and(|motion| motion.document == owner.0)
            && presentation_actions
                .into_iter()
                .flat_map(|actions| actions.authored_action_records(document))
                .any(|action| {
                    action.trigger == activation.trigger
                        && matches!(
                            action.action,
                            UiPresentationAction::SetTargetNodeSelectionAndTimedSequencesActive {
                                target_node, active: false, document_role: None,
                            } if is_earthquake_runner(document, target_node)
                        )
                })
        {
            next = None;
        }
    }
    match (motion, next) {
        (Some(mut current), Some(next)) => {
            current.set_if_neq(next);
        }
        (Some(_), None) => {
            commands.entity(camera).remove::<EarthquakeCameraMotion>();
        }
        (None, Some(next)) => {
            commands.entity(camera).insert(next);
        }
        (None, None) => {}
    }
}
