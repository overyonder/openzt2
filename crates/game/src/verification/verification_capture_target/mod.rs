use bevy::{
    asset::RenderAssetUsages,
    camera::RenderTarget,
    prelude::*,
    render::render_resource::{TextureFormat, TextureUsages},
};

use crate::plugins;

use super::{
    verification_journey_state_types::VerificationJourneySession,
    verification_journey_types::{VERIFICATION_TARGET_HEIGHT, VERIFICATION_TARGET_WIDTH},
};

#[derive(Component)]
pub(crate) struct VerificationPointerMarker;

#[derive(Component)]
pub(crate) struct VerificationCaptureTargetInstalled;

/// Headless runs render every game camera into one offscreen image instead of a window.
pub(crate) fn install_verification_capture_target_and_pointer_marker(
    mut commands: Commands,
    mut session: ResMut<VerificationJourneySession>,
    mut images: ResMut<Assets<Image>>,
    cameras: Query<
        (Entity, &RenderTarget),
        (
            Or<(
                With<plugins::shell::shell_screen_presentation_types::ShellUiCamera>,
                With<plugins::shell::shell_screen_presentation_types::ShellClearCamera>,
                With<plugins::camera::camera_runtime_state_types::ZooCamera>,
                With<Camera3d>,
            )>,
            Without<VerificationCaptureTargetInstalled>,
        ),
    >,
    marker: Query<(), With<VerificationPointerMarker>>,
) {
    let windowed = session.windowed;
    let target = session.capture_target.get_or_insert_with(|| {
        if windowed {
            return RenderTarget::Window(bevy::window::WindowRef::Primary);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let mut image = Image::new_target_texture(
            VERIFICATION_TARGET_WIDTH as u32,
            VERIFICATION_TARGET_HEIGHT as u32,
            TextureFormat::Rgba8UnormSrgb,
            None,
        );
        image.asset_usage = RenderAssetUsages::all();
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        RenderTarget::Image(images.add(image).into())
    });
    for (camera, camera_target) in &cameras {
        if matches!(camera_target, RenderTarget::Window(_)) {
            commands
                .entity(camera)
                .insert((target.clone(), VerificationCaptureTargetInstalled));
        }
    }
    // The marker is a hollow ring so the object under the pointer, such as a
    // placement preview centred on the hotspot, stays visible in captures.
    if marker.is_empty() {
        commands.spawn((
            VerificationPointerMarker,
            Name::new("Verification Pointer Marker"),
            Node {
                position_type: PositionType::Absolute,
                width: px(14.0),
                height: px(14.0),
                border: UiRect::all(px(2.0)),
                border_radius: BorderRadius::all(px(7.0)),
                ..default()
            },
            BorderColor::all(Color::srgb(1.0, 0.05, 0.05)),
            GlobalZIndex(i32::MAX),
            Pickable::IGNORE,
        ));
    }
}
