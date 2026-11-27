use bevy::prelude::*;
use bevy::{
    picking::{events::PointerPress, hover::DirectlyHovered, pointer::PointerButton},
    ui::Pressed,
};
use openzt2_game_data::ui_document::document::UiDocumentRole;

use openzt2_interface_state::ui::authored_ui_interaction_enabled_state::UiInteractionEnabled;

use openzt2_interface_state::shell::shell_selection_types::ShellScreen;

use openzt2_game_assets::{
    application_lifecycle::GamePhase, asset_source::AssetArchives,
    ui_document::ui_document_role_asset_path_selection::select_bevy_asset_path_for_canonical_ui_document_role,
};

use super::archive_reload_operation::AssetArchiveReloadOperation;
use openzt2_game_assets::ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset;

use super::mod_manager_types::{
    AssetArchiveEnabledStateCheckbox, ModManagerOverlayButton, ModManagerOverlayRoot,
    ModManagerReloadInputBlocker, ReloadArchiveAtSamePath,
};

pub(super) fn spawn_mod_manager_overlay_from_asset_archives(
    mut commands: Commands,
    asset_archives: Res<AssetArchives>,
) {
    let mut overlay = commands.spawn((
        Name::new("mod manager"),
        ModManagerOverlayRoot::default(),
        Pickable::default(),
        Visibility::Hidden,
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
    ));
    let overlay_entity = overlay.id();
    overlay.with_children(|overlay_root| {
        overlay_root
            .spawn((
                Name::new("Close Mods"),
                bevy::ui_widgets::Button,
                UiInteractionEnabled(true),
                ModManagerOverlayButton {
                    overlay: overlay_entity,
                    open: false,
                },
                Node {
                    padding: UiRect::axes(px(8), px(5)),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
            ))
            .with_child((
                Text::new("Close"),
                TextFont::from_font_size(17.0),
                TextColor(Color::WHITE),
            ));
        overlay_root.spawn((
            super::redo_button::RedoButtonCheckbox,
            bevy::ui_widgets::Button,
            UiInteractionEnabled(true),
            Text::new("[x] Redo button (built-in)"),
            TextFont::from_font_size(17.0),
            TextColor(Color::WHITE),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
        ));
        overlay_root.spawn((
            crate::blender_link::ModderToolsCheckbox,
            Name::new("Modder tools"),
            bevy::ui_widgets::Button,
            UiInteractionEnabled(true),
            Text::new(crate::blender_link::modder_tools_checkbox_label(false)),
            TextFont::from_font_size(17.0),
            TextColor(Color::WHITE),
            Node {
                padding: UiRect::axes(px(8), px(5)),
                ..default()
            },
            BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
        ));
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
            overlay_root
                .spawn(Node {
                    width: percent(100),
                    column_gap: px(4),
                    ..default()
                })
                .with_children(|row| {
                    let mut archive_checkbox_row = row.spawn((
                        Name::new("archive checkbox"),
                        AssetArchiveEnabledStateCheckbox {
                            archive_status_index,
                        },
                        Node {
                            flex_grow: 1.0,
                            padding: UiRect::axes(px(8), px(5)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
                    ));
                    if !archive_status.is_required_base_archive {
                        archive_checkbox_row.insert((
                            bevy::ui_widgets::Button,
                            DirectlyHovered::default(),
                            UiInteractionEnabled(true),
                        ));
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
                    if !archive_status.is_required_base_archive {
                        row.spawn((
                            ReloadArchiveAtSamePath(archive_status_index),
                            bevy::ui_widgets::Button,
                            UiInteractionEnabled(true),
                            Text::new("Reload"),
                            TextFont::from_font_size(17.0),
                            TextColor(Color::WHITE),
                            BackgroundColor(Color::srgb(0.14, 0.14, 0.17)),
                            Node {
                                padding: UiRect::axes(px(8), px(5)),
                                ..default()
                            },
                        ));
                    }
                });
        }
    });
    commands
        .spawn((
            Name::new("Open Mods"),
            bevy::ui_widgets::Button,
            UiInteractionEnabled(true),
            ModManagerOverlayButton {
                overlay: overlay_entity,
                open: true,
            },
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: px(24),
                right: px(24),
                padding: UiRect::all(px(16)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.04, 0.05, 0.96)),
            GlobalZIndex(1_000),
            Visibility::Hidden,
        ))
        .with_child((
            Text::new("Mods"),
            TextFont::from_font_size(20.0),
            TextColor(Color::WHITE),
        ));
    commands
        .spawn((
            Name::new("mod reload input blocker"),
            ModManagerReloadInputBlocker,
            bevy::ui_widgets::Button,
            UiInteractionEnabled(true),
            Visibility::Hidden,
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

pub(super) fn apply_mod_manager_visibility_controls(
    phase: Res<State<GamePhase>>,
    reloads: Query<(), With<AssetArchiveReloadOperation>>,
    shell_screens: Query<&ShellScreen>,
    mut presses: MessageReader<PointerPress>,
    controls: Query<&ModManagerOverlayButton>,
    mut overlays: Query<&mut ModManagerOverlayRoot>,
) {
    if *phase.get() != GamePhase::MainMenu
        || !shell_screens
            .iter()
            .any(|screen| *screen == ShellScreen::MainMenu)
        || !reloads.is_empty()
    {
        presses.clear();
        return;
    }
    for press in presses
        .read()
        .filter(|press| press.pointer.id.is_mouse() && press.button == PointerButton::Primary)
    {
        if let Ok(control) = controls.get(press.entity) {
            if let Ok(mut overlay) = overlays.get_mut(control.overlay) {
                overlay.is_open = control.open;
            }
        }
    }
}

pub(super) fn apply_selected_asset_archive_change(
    mut commands: Commands,
    mut asset_archives: ResMut<AssetArchives>,
    asset_server: Res<AssetServer>,
    phase: Res<State<GamePhase>>,
    reloads: Query<(), With<AssetArchiveReloadOperation>>,
    shell_screens: Query<&ShellScreen>,
    mut presses: MessageReader<PointerPress>,
    archive_checkboxes: Query<&AssetArchiveEnabledStateCheckbox>,
    reload_buttons: Query<&ReloadArchiveAtSamePath>,
    mut overlays: MessageWriter<openzt2_game_assets::asset_source::ArchiveOverlayChanged>,
) {
    if *phase.get() != GamePhase::MainMenu
        || !shell_screens
            .iter()
            .any(|screen| *screen == ShellScreen::MainMenu)
        || !reloads.is_empty()
    {
        presses.clear();
        return;
    }
    for press in presses
        .read()
        .filter(|press| press.pointer.id.is_mouse() && press.button == PointerButton::Primary)
    {
        let toggle = archive_checkboxes.get(press.entity).ok();
        let Some(index) = toggle
            .map(|checkbox| checkbox.archive_status_index)
            .or_else(|| reload_buttons.get(press.entity).ok().map(|reload| reload.0))
        else {
            continue;
        };
        let archive_statuses = asset_archives.enabled_archive_statuses();
        let Some(selected_archive_status) = archive_statuses.get(index) else {
            continue;
        };
        if selected_archive_status.is_required_base_archive {
            continue;
        }
        let edit = if toggle.is_some() {
            asset_archives.set_archive_enabled(index, !selected_archive_status.is_enabled)
        } else {
            asset_archives.reload_archive_at_same_path(index)
        };
        let edit = match edit {
            Ok(edit) => edit,
            Err(archive_mutation_error) => {
                error!("could not update Z2F archive: {archive_mutation_error}");
                return;
            }
        };
        if edit.publication.is_none() {
            return;
        }
        let Some(main_menu_document_path) =
            select_bevy_asset_path_for_canonical_ui_document_role(UiDocumentRole::MainMenu)
        else {
            error!("main-menu UI document has no canonical asset path");
            return;
        };
        let menu_path = bevy::asset::AssetPath::parse(&main_menu_document_path).into_owned();
        let existing_menu = asset_server.get_handle::<UiDocumentAsset>(menu_path.clone());
        let main_menu = existing_menu
            .clone()
            .unwrap_or_else(|| asset_server.load(menu_path.clone()));
        let generation = edit
            .publication
            .as_ref()
            .map(|publication| publication.generation);
        let mut operation = AssetArchiveReloadOperation::new(generation);
        if let Some(publication) = edit.publication {
            for change in &publication.changes {
                use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;
                let path = &change.normalized_asset_path;
                if asset_server
                    .get_handle::<WorldScenarioDocumentAsset>(path.clone())
                    .is_some()
                {
                    if let Some(entry) = publication.reload_entry_path(path) {
                        let snapshot = asset_server.load::<WorldScenarioDocumentAsset>(entry);
                        operation.expect_reload(snapshot.untyped());
                    }
                }
            }
            overlays.write(publication);
        }
        let mut menu_reload_requested = false;
        for path in edit
            .changed_asset_paths
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        {
            let asset_path = bevy::asset::AssetPath::from(path);
            menu_reload_requested |= asset_path == menu_path;
            for handle in asset_server.get_handles_untyped(asset_path.clone()) {
                operation.expect_reload(handle);
            }
            if !asset_server.watching_for_changes() {
                asset_server.reload(asset_path.clone());
            }
        }
        if existing_menu.is_some() && !menu_reload_requested {
            asset_server.reload(menu_path);
        }
        operation.expect_reload(main_menu.untyped());
        commands.spawn(operation);
        return;
    }
}

pub(super) fn synchronize_mod_manager_visibility_and_reload_input_blocking(
    phase: Res<State<GamePhase>>,
    reloads: Query<(), With<AssetArchiveReloadOperation>>,
    shell_screens: Query<&ShellScreen>,
    mut mod_manager_overlay_roots: Query<
        (&ModManagerOverlayRoot, &mut Node, &mut Visibility),
        (
            With<ModManagerOverlayRoot>,
            Without<ModManagerReloadInputBlocker>,
        ),
    >,
    mut visibility_controls: Query<
        (&ModManagerOverlayButton, &mut Node, &mut Visibility),
        (
            Without<ModManagerOverlayRoot>,
            Without<ModManagerReloadInputBlocker>,
        ),
    >,
    mut reload_input_blockers: Query<
        (&mut Node, &mut Visibility),
        (
            With<ModManagerReloadInputBlocker>,
            Without<ModManagerOverlayRoot>,
            Without<ModManagerOverlayButton>,
        ),
    >,
) {
    let mod_manager_is_available = *phase.get() == GamePhase::MainMenu
        && shell_screens
            .iter()
            .any(|screen| *screen == ShellScreen::MainMenu);
    for (overlay, mut node, mut visibility) in &mut mod_manager_overlay_roots {
        let display = if mod_manager_is_available && overlay.is_open {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        visibility.set_if_neq(if display == Display::Flex {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (control, mut node, mut visibility) in &mut visibility_controls {
        if control.open {
            let is_closed = mod_manager_overlay_roots
                .get(control.overlay)
                .is_ok_and(|(overlay, _, _)| !overlay.is_open);
            let display = if mod_manager_is_available && is_closed {
                Display::Flex
            } else {
                Display::None
            };
            if node.display != display {
                node.display = display;
            }
            visibility.set_if_neq(if display == Display::Flex {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    for (mut reload_input_blocker, mut visibility) in &mut reload_input_blockers {
        let display = if mod_manager_is_available && !reloads.is_empty() {
            Display::Flex
        } else {
            Display::None
        };
        if reload_input_blocker.display != display {
            reload_input_blocker.display = display;
        }
        visibility.set_if_neq(if display == Display::Flex {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

pub(super) fn present_asset_archive_checkbox_interaction_state(
    reloads: Query<(), With<AssetArchiveReloadOperation>>,
    reloads_started: Query<(), Added<AssetArchiveReloadOperation>>,
    mut reloads_finished: RemovedComponents<AssetArchiveReloadOperation>,
    mut released: RemovedComponents<Pressed>,
    mut archive_checkboxes: Query<(
        Entity,
        Ref<AssetArchiveEnabledStateCheckbox>,
        Ref<DirectlyHovered>,
        Option<Ref<Pressed>>,
        &mut BackgroundColor,
    )>,
) {
    let reload_changed = reloads_finished.read().count() != 0 || !reloads_started.is_empty();
    let released: bevy::ecs::entity::EntityHashSet = released.read().collect();
    let blocked = !reloads.is_empty();
    for (entity, checkbox, hovered, pressed, mut background) in &mut archive_checkboxes {
        if !(reload_changed
            || checkbox.is_changed()
            || hovered.is_changed()
            || pressed.as_ref().is_some_and(DetectChanges::is_added)
            || released.contains(&entity))
        {
            continue;
        }
        let color = if blocked {
            Color::srgb(0.08, 0.08, 0.10)
        } else {
            match (hovered.get(), pressed.is_some()) {
                (_, true) => Color::srgb(0.32, 0.32, 0.38),
                (true, false) => Color::srgb(0.24, 0.24, 0.30),
                (false, false) => Color::srgb(0.14, 0.14, 0.17),
            }
        };
        background.set_if_neq(BackgroundColor(color));
    }
}

pub(super) fn synchronize_mod_manager_archive_checkbox_labels(
    asset_archives: Res<AssetArchives>,
    archive_checkboxes: Query<(Ref<AssetArchiveEnabledStateCheckbox>, Ref<Children>)>,
    mut archive_checkbox_labels: Query<&mut Text>,
) {
    let archives_changed = asset_archives.is_changed();
    if !archives_changed
        && !archive_checkboxes
            .iter()
            .any(|(checkbox, children)| checkbox.is_changed() || children.is_changed())
    {
        return;
    }
    let archive_statuses = asset_archives.enabled_archive_statuses();
    for (archive_checkbox, archive_checkbox_children) in &archive_checkboxes {
        if !archives_changed
            && !archive_checkbox.is_changed()
            && !archive_checkbox_children.is_changed()
        {
            continue;
        }
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
