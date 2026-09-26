use bevy::prelude::*;
use openzt2_game_data::{
    ui_document::{document::UiDocumentRole, node_layout::UiNodeRegionMetric},
    AssetId,
};

use crate::{
    assets::{
        localization::{
            localization_asset_types::LocalizationAsset,
            localization_precedence_index::LocalizationPrecedenceIndex,
        },
        ui_document::ui_document_asset_types_and_borrowing_queries::UiDocumentAsset,
        world_definitions::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitions,
    },
    plugins::{
        ui::{
            authored_ui_node_projection_components::{UiDocumentOwner, UiDocumentRoot, UiNodeId},
            authored_ui_visual_types::{UiSourceRect, UiVisualLayer},
            ui_document_lifecycle_contracts::UiRoleRequests,
        },
        world_spawn::{
            world_hydration_types::WorldHydration, world_load_completion_marker::WorldLoadCompleted,
        },
    },
};

#[derive(Component)]
pub(super) struct LoadingScreen;

pub(super) fn show_authored_loading_screen(
    mut commands: Commands,
    mut roles: ResMut<UiRoleRequests>,
) {
    let owner = commands
        .spawn((LoadingScreen, Visibility::Inherited, GlobalZIndex(100)))
        .id();
    roles.request(UiDocumentRole::Loading, owner);
}

pub(super) fn close_authored_loading_screen(
    mut commands: Commands,
    screens: Query<Entity, With<LoadingScreen>>,
) {
    for screen in &screens {
        commands.entity(screen).despawn();
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn project_world_loading_progress(
    definitions: Res<WorldDefinitions>,
    hydration: Query<&WorldHydration>,
    completed: Query<(), With<WorldLoadCompleted>>,
    screens: Query<(), With<LoadingScreen>>,
    roots: Query<(&UiDocumentRoot, &ChildOf)>,
    documents: Res<Assets<UiDocumentAsset>>,
    localization: Res<LocalizationPrecedenceIndex>,
    localization_assets: Res<Assets<LocalizationAsset>>,
    mut nodes: Query<(
        Entity,
        &UiNodeId,
        &UiDocumentOwner,
        &mut Node,
        Option<&mut Text>,
    )>,
    mut visuals: Query<(&ChildOf, &UiVisualLayer, &mut UiSourceRect)>,
) {
    let (finished, total, status) = if let Some(hydration) = hydration.iter().next() {
        if hydration.terrain_hydration_is_pending() {
            (0, 0, "loadstatus:initnodes")
        } else {
            let (finished, total) = hydration.prefab_loading_progress();
            (finished, total, "loadstatus:attachentities")
        }
    } else if !completed.is_empty() {
        (1, 1, "loadstatus:attachentities")
    } else {
        let (finished, total) = definitions.loading_progress();
        if finished < total {
            (finished, total, "loadstatus:objects")
        } else {
            (0, 0, "loadstatus:map")
        }
    };
    let fraction = if total == 0 {
        0.0
    } else {
        (finished as f32 / total as f32).clamp(0.0, 1.0)
    };
    let status_text = localization
        .borrow_loaded_localization_view(&localization_assets)
        .and_then(|view| view.find_plain_localized_text(AssetId::from_key(status)))
        .unwrap_or_default();
    for (entity, id, owner, mut node, text) in &mut nodes {
        let Ok((root, parent)) = roots.get(owner.0) else {
            continue;
        };
        if !screens.contains(parent.parent()) {
            continue;
        }
        let Some(document) = documents.get(&root.document) else {
            continue;
        };
        let document = document.canonical_ui_document();
        let Some(record) = document.nodes.get(id.index as usize) else {
            continue;
        };
        if record.name == "Loading Bar Front" {
            // Crop source and destination together; scaled canvas clipping distorts the UVs.
            for (parent, layer, mut source) in &mut visuals {
                if parent.parent() != entity {
                    continue;
                }
                let Some(visual) = record
                    .visuals
                    .iter()
                    .find(|visual| visual.visual_state == layer.0)
                else {
                    continue;
                };
                let [UiNodeRegionMetric::Pixels(x), UiNodeRegionMetric::Pixels(y), UiNodeRegionMetric::Pixels(width), UiNodeRegionMetric::Pixels(height)] =
                    &visual.source_rect.metrics
                else {
                    continue;
                };
                if *width <= 0.0 || *height <= 0.0 {
                    continue;
                }
                let filled_width = (width * fraction).floor();
                node.width = px(record.rect[2] * filled_width / width);
                let rectangle = [*x as i32, *y as i32, filled_width as i32, *height as i32];
                if source.0 != rectangle {
                    source.0 = rectangle;
                }
            }
        } else if let Some(mut text) = text {
            let value = match record.name.as_str() {
                "Loading Status Text" => status_text.to_owned(),
                "Loading Status Number" if total > 0 => format!("{finished} / {total}"),
                "Loading Status Number" => String::new(),
                _ => continue,
            };
            if text.0 != value {
                text.0 = value;
            }
        }
    }
}
