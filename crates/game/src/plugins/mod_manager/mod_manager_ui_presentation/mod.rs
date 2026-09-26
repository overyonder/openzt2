use bevy::{asset::UntypedAssetId, prelude::*};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use crate::{
    application_lifecycle::GamePhase,
    asset_source::AssetArchives,
    assets::ui_document::ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
    plugins::shell::{
        shell_navigation_request_types::ReturnToMainMenu, shell_selection_types::ShellScreen,
    },
};

use super::mod_manager_types::{
    AssetArchiveEnabledStateCheckbox, AssetArchiveReloadState, ModManagerOverlayRoot,
    ModManagerReloadInputBlocker,
};

pub(super) fn spawn_mod_manager_overlay_from_asset_archives(
    mut commands: Commands,
    asset_archives: Res<AssetArchives>,
) {
    commands
        .spawn((
            Name::new("mod manager"),
            ModManagerOverlayRoot,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: px(24),
                right: px(24),
                min_width: px(300),
                padding: UiRect::all(px(16)),
                flex_direction: FlexDirection::Column,
                row_gap: px(8),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.05, 0.96)),
            GlobalZIndex(1_000),
        ))
        .with_children(|overlay_root| {
            overlay_root.spawn((
                Text::new("Z2F priority — highest first"),
                TextFont::from_font_size(20.0),
                TextColor(Color::WHITE),
            ));
            for (archive_status_index, archive_status) in asset_archives
                .enabled_archive_statuses()
                .into_iter()
                .enumerate()
                .rev()
            {
                let mut archive_checkbox_row = overlay_root.spawn((
                    Name::new("archive checkbox"),
                    AssetArchiveEnabledStateCheckbox {
                        archive_status_index,
                    },
                    Node {
                        width: percent(100),
                        padding: UiRect::axes(px(8), px(5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
                ));
                if !archive_status.is_required_base_archive {
                    archive_checkbox_row.insert(Button);
                }
                archive_checkbox_row.with_child((
                    Text::new(format_asset_archive_enabled_state_checkbox_label(
                        archive_status.is_enabled,
                        archive_status.is_required_base_archive,
                        &archive_status.archive_name.to_string_lossy(),
                    )),
                    TextFont::from_font_size(17.0),
                    TextColor(if archive_status.is_required_base_archive {
                        Color::srgb(0.65, 0.65, 0.68)
                    } else {
                        Color::WHITE
                    }),
                ));
            }
        });
    commands
        .spawn((
            Name::new("mod reload input blocker"),
            ModManagerReloadInputBlocker,
            Button,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
            GlobalZIndex(2_000),
        ))
        .with_child((
            Text::new("Reloading mods…"),
            TextFont::from_font_size(24.0),
            TextColor(Color::WHITE),
        ));
}

pub(super) fn toggle_selected_asset_archive_enabled_state(
    asset_archives: Res<AssetArchives>,
    asset_server: Res<AssetServer>,
    phase: Res<State<GamePhase>>,
    mut reload_state: ResMut<AssetArchiveReloadState>,
    shell_screens: Query<&ShellScreen>,
    changed_archive_checkboxes: Query<
        (&AssetArchiveEnabledStateCheckbox, &Interaction),
        Changed<Interaction>,
    >,
) {
    if *phase.get() != GamePhase::MainMenu
        || !shell_screens
            .iter()
            .any(|screen| *screen == ShellScreen::MainMenu)
        || reload_state.is_in_progress()
    {
        return;
    }
    for (archive_checkbox, interaction) in &changed_archive_checkboxes {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let archive_statuses = asset_archives.enabled_archive_statuses();
        let Some(selected_archive_status) =
            archive_statuses.get(archive_checkbox.archive_status_index)
        else {
            continue;
        };
        if selected_archive_status.is_required_base_archive {
            continue;
        }
        let changed_bevy_asset_paths = match asset_archives.set_archive_enabled(
            archive_checkbox.archive_status_index,
            !selected_archive_status.is_enabled,
        ) {
            Ok(changed_bevy_asset_paths) => changed_bevy_asset_paths,
            Err(archive_mutation_error) => {
                error!("could not change Z2F state: {archive_mutation_error}");
                return;
            }
        };
        let mut pending_loaded_asset_identifiers = Vec::new();
        for loaded_asset_identifier in changed_bevy_asset_paths
            .iter()
            .flat_map(|path| asset_server.get_path_ids(path.to_owned()))
        {
            if !pending_loaded_asset_identifiers.contains(&loaded_asset_identifier) {
                pending_loaded_asset_identifiers.push(loaded_asset_identifier);
            }
        }
        let Some(main_menu_document_path) =
            select_bevy_asset_path_for_canonical_ui_document_role(UiDocumentRole::MainMenu)
        else {
            error!("main-menu UI document has no canonical asset path");
            return;
        };
        let main_menu_document = asset_server.load(main_menu_document_path.clone());
        asset_server.reload(main_menu_document_path);
        reload_state.begin(pending_loaded_asset_identifiers, main_menu_document);
        return;
    }
}

pub(super) fn finish_asset_archive_reload_and_request_main_menu_document_rebuild(
    asset_server: Res<AssetServer>,
    mut reload_state: ResMut<AssetArchiveReloadState>,
    mut return_to_main_menu: MessageWriter<ReturnToMainMenu>,
) {
    if !reload_state.is_in_progress()
        || reload_state.wait_for_asset_source_event_observation()
        || reload_state
            .pending_loaded_asset_identifiers()
            .iter()
            .any(|asset_identifier| asset_load_is_not_finished(&asset_server, *asset_identifier))
        || reload_state
            .main_menu_document()
            .is_some_and(|main_menu_document| {
                asset_load_is_not_finished(&asset_server, main_menu_document.id().untyped())
            })
    {
        return;
    }
    reload_state.finish();
    return_to_main_menu.write(ReturnToMainMenu);
}

fn asset_load_is_not_finished(
    asset_server: &AssetServer,
    asset_identifier: UntypedAssetId,
) -> bool {
    asset_server.get_load_states(asset_identifier).is_some_and(
        |(asset, _, recursive_dependencies)| {
            asset.is_loading()
                || recursive_dependencies.is_loading()
                || asset.is_loaded()
                    && matches!(
                        recursive_dependencies,
                        bevy::asset::RecursiveDependencyLoadState::NotLoaded
                    )
        },
    )
}

pub(super) fn synchronize_mod_manager_visibility_and_reload_input_blocking(
    phase: Res<State<GamePhase>>,
    reload_state: Res<AssetArchiveReloadState>,
    shell_screens: Query<&ShellScreen>,
    mut mod_manager_overlay_roots: Query<
        &mut Node,
        (
            With<ModManagerOverlayRoot>,
            Without<ModManagerReloadInputBlocker>,
        ),
    >,
    mut reload_input_blockers: Query<
        &mut Node,
        (
            With<ModManagerReloadInputBlocker>,
            Without<ModManagerOverlayRoot>,
        ),
    >,
) {
    let mod_manager_is_available = *phase.get() == GamePhase::MainMenu
        && shell_screens
            .iter()
            .any(|screen| *screen == ShellScreen::MainMenu);
    for mut mod_manager_overlay_root in &mut mod_manager_overlay_roots {
        mod_manager_overlay_root.display = if mod_manager_is_available {
            Display::Flex
        } else {
            Display::None
        };
    }
    for mut reload_input_blocker in &mut reload_input_blockers {
        reload_input_blocker.display = if mod_manager_is_available && reload_state.is_in_progress()
        {
            Display::Flex
        } else {
            Display::None
        };
    }
}

pub(super) fn present_asset_archive_checkbox_interaction_state(
    reload_state: Res<AssetArchiveReloadState>,
    mut archive_checkboxes: Query<
        (&Interaction, &mut BackgroundColor),
        With<AssetArchiveEnabledStateCheckbox>,
    >,
) {
    for (interaction, mut background) in &mut archive_checkboxes {
        background.0 = if reload_state.is_in_progress() {
            Color::srgb(0.08, 0.08, 0.10)
        } else {
            match interaction {
                Interaction::Pressed => Color::srgb(0.32, 0.32, 0.38),
                Interaction::Hovered => Color::srgb(0.24, 0.24, 0.30),
                Interaction::None => Color::srgb(0.14, 0.14, 0.17),
            }
        };
    }
}

pub(super) fn synchronize_mod_manager_archive_checkbox_labels(
    asset_archives: Res<AssetArchives>,
    archive_checkboxes: Query<(&AssetArchiveEnabledStateCheckbox, &Children)>,
    mut archive_checkbox_labels: Query<&mut Text>,
) {
    let archive_statuses = asset_archives.enabled_archive_statuses();
    for (archive_checkbox, archive_checkbox_children) in &archive_checkboxes {
        let Some(archive_status) = archive_statuses.get(archive_checkbox.archive_status_index)
        else {
            continue;
        };
        let Some(archive_checkbox_label_entity) = archive_checkbox_children.first().copied() else {
            continue;
        };
        let Ok(mut archive_checkbox_label) =
            archive_checkbox_labels.get_mut(archive_checkbox_label_entity)
        else {
            continue;
        };
        let expected_archive_checkbox_label = format_asset_archive_enabled_state_checkbox_label(
            archive_status.is_enabled,
            archive_status.is_required_base_archive,
            &archive_status.archive_name.to_string_lossy(),
        );
        if archive_checkbox_label.0 != expected_archive_checkbox_label {
            archive_checkbox_label.0 = expected_archive_checkbox_label;
        }
    }
}

fn format_asset_archive_enabled_state_checkbox_label(
    archive_is_enabled: bool,
    archive_is_required_base: bool,
    archive_name: &str,
) -> String {
    let required_base_archive_suffix = if archive_is_required_base {
        " (required)"
    } else {
        ""
    };
    let enabled_state_character = if archive_is_enabled { 'x' } else { ' ' };
    format!("[{enabled_state_character}] {archive_name}{required_base_archive_suffix}")
}
