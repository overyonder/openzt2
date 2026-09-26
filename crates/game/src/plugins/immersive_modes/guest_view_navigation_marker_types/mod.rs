use bevy::prelude::*;

#[derive(Component)]
pub(super) struct GuestViewNavigationMarkerPresentation {
    pub controller_entity: Entity,
    pub targeted_entity: Entity,
    pub navigation_revision: u64,
    pub gizmo_asset: Handle<GizmoAsset>,
}
