use bevy::prelude::*;
use openzt2_game_data::ui_document::{
    document::UiDocumentRole, node_property_binding::UiTextPropertyBindingSource,
};

use crate::{
    assets::texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image,
    plugins::{
        persistence::profile_types::LoadProfileIndex,
        ui::{
            authored_ui_node_projection_components::UiDocumentRoot,
            authored_ui_text_content_binding::UiTextBinding,
            ui_document_lifecycle_contracts::UiRoleRequests,
        },
    },
};

use super::{
    shell_screen_presentation_types::MainMenuLogoFallback, shell_selection_types::ShellScreen,
};

const MAIN_MENU_LOGO_TEXTURE_PATH: &str = "ui/shared/zoo2logo.dds";

pub(super) fn create_main_menu_screen_and_request_profile_index(
    mut commands: Commands,
    assets: Res<AssetServer>,
    old_shell_screens: Query<(Entity, &ShellScreen)>,
    mut requested_ui_roles: ResMut<UiRoleRequests>,
    mut profile_index_load_requests: MessageWriter<LoadProfileIndex>,
) {
    for (entity, _) in &old_shell_screens {
        commands.entity(entity).despawn();
    }
    let main_menu_screen_owner = commands
        .spawn((
            ShellScreen::MainMenu,
            Node {
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Visibility::Inherited,
        ))
        .id();
    commands.spawn((
        Name::new("main menu logo"),
        ImageNode::new(
            assets.load(select_bevy_image_asset_path_for_blue_fang_source_image(
                MAIN_MENU_LOGO_TEXTURE_PATH,
            )),
        ),
        Node {
            position_type: PositionType::Absolute,
            left: percent(40.625),
            top: percent(17.84),
            width: percent(18.75),
            aspect_ratio: Some(1.0),
            ..default()
        },
        GlobalZIndex(10),
        MainMenuLogoFallback,
        ChildOf(main_menu_screen_owner),
    ));
    requested_ui_roles.request(UiDocumentRole::MainMenu, main_menu_screen_owner);
    profile_index_load_requests.write(LoadProfileIndex);
}

/// The translated main-menu document owns its presentation once available.
/// The direct image exists only while that document cannot yet be projected.
pub(super) fn remove_fallback_main_menu_logo_after_document_projection(
    mut commands: Commands,
    fallback_logos: Query<(Entity, &ChildOf), With<MainMenuLogoFallback>>,
    projected_document_roots: Query<&ChildOf, With<UiDocumentRoot>>,
) {
    for (fallback_logo, parent) in &fallback_logos {
        if projected_document_roots
            .iter()
            .any(|document_root| document_root.parent() == parent.parent())
        {
            commands.entity(fallback_logo).despawn();
        }
    }
}

pub(super) fn project_application_version_into_new_authored_text_bindings(
    mut authored_text_fields: Query<(Ref<UiTextBinding>, &mut Text)>,
) {
    for (binding, mut text) in &mut authored_text_fields {
        if matches!(binding.0, UiTextPropertyBindingSource::ApplicationVersion)
            && binding.is_added()
        {
            text.0.clear();
            text.0.push_str(env!("CARGO_PKG_VERSION"));
        }
    }
}

pub(super) fn despawn_shell_screens_when_leaving_main_menu(
    mut commands: Commands,
    shell_screens: Query<Entity, With<ShellScreen>>,
) {
    for shell_screen in &shell_screens {
        commands.entity(shell_screen).despawn();
    }
}
