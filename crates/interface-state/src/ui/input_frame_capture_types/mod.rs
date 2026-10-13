use bevy::prelude::*;

/// Input-frame capture survives a modal being hidden or despawned by the same
/// cancel press. Gameplay must not consume that press later in the frame.
#[derive(Resource, Default)]
pub struct AuthoredModalInputCapture(pub Option<Entity>);

/// Whether the zoo view was in the original overhead mode when this input
/// frame began: no construction tool and no immersive mode. Those modes own
/// Escape themselves, and their own handling ends them before UI routing runs.
#[derive(Resource, Default)]
pub struct OverheadModeInputCapture(pub bool);
