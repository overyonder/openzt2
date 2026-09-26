use bevy::prelude::*;

/// UI camera retained across the boot and menu shell phases.
#[derive(Component)]
pub(crate) struct ShellUiCamera;

/// Black clear pass beneath the alpha-composited shell UI camera.
#[derive(Component)]
pub(crate) struct ShellClearCamera;

#[derive(Component)]
pub(super) struct MainMenuBackdrop;

#[derive(Component)]
pub(super) struct MainMenuLogoFallback;
