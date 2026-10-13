use bevy::prelude::*;

#[derive(Component)]
pub struct LoadingScreen;

/// UI camera retained across the boot and menu shell phases.
#[derive(Component)]
pub struct ShellUiCamera;

/// Black clear pass beneath the alpha-composited shell UI camera.
#[derive(Component)]
pub struct ShellClearCamera;

#[derive(Component)]
pub struct MainMenuBackdrop;

#[derive(Component)]
pub struct MainMenuLogoFallback;
