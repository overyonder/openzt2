//! Selected image, model, and document dependency winner resolution.

use std::collections::{BTreeMap, BTreeSet};

use crate::assets::{
    source_document::{
        path::AssetPath,
        ui::model::{SourceUiEvent, SourceUiNode, SourceUiWidgetData},
    },
    ui_document::source::lower::authored_ui_document_lowering::UiResolvedDependencies,
};

pub(super) fn collect_ui_visual_image_paths_without_selected_dependency_winners(
    owner: &str,
    node: &SourceUiNode,
    winners: &BTreeMap<String, String>,
    absent: &mut BTreeSet<String>,
) {
    if let Some(aspect) = &node.aspect {
        aspect
            .default
            .iter()
            .chain(aspect.standard.iter().map(|named| &named.visual))
            .chain(aspect.alternate.iter().map(|named| &named.visual))
            .filter_map(|visual| visual.image.as_ref())
            .map(|path| path.key())
            .filter(|path| {
                resolve_selected_ui_dependency_winner_path(owner, path, winners).is_none()
            })
            .for_each(|path| {
                if absent.insert(path.clone()) {
                    bevy::log::warn!(
                        document = owner,
                        image = path,
                        "UI image is unavailable; leaving this visual empty"
                    );
                }
            });
    }
    node.children.iter().for_each(|child| {
        collect_ui_visual_image_paths_without_selected_dependency_winners(
            owner, child, winners, absent,
        );
    });
}

pub(super) fn resolve_selected_ui_dependency_winner_path(
    owner: &str,
    authored: &str,
    winners: &BTreeMap<String, String>,
) -> Option<String> {
    let key = AssetPath::new(authored).key();
    winners
        .get(&format!("{owner}\0{key}"))
        .or_else(|| winners.get(&format!("*\0{key}")))
        .cloned()
        .or_else(|| {
            winners
                .values()
                .any(|winner| winner == &key)
                .then_some(key.clone())
        })
        .or_else(|| {
            key.rsplit('/')
                .next()
                .and_then(|basename| winners.get(&format!("@basename\0{basename}")))
                .cloned()
        })
        .or_else(|| {
            let basename = key.rsplit('/').next()?;
            let matches = winners
                .values()
                .filter(|winner| winner.rsplit('/').next() == Some(basename))
                .collect::<BTreeSet<_>>();
            (matches.len() == 1)
                .then(|| matches.into_iter().next().cloned())
                .flatten()
        })
}

pub(super) fn rewrite_ui_source_node_dependency_paths_to_selected_winners(
    owner: &str,
    node: &mut SourceUiNode,
    dependencies: &UiResolvedDependencies,
) {
    let resolve_image_path = |image: &mut AssetPath| {
        if let Some(winner) =
            resolve_selected_ui_dependency_winner_path(owner, image.as_str(), &dependencies.images)
        {
            *image = AssetPath::new(winner);
        }
    };
    if let Some(cursor) = node.cursor.as_mut() {
        if let Some(winner) =
            resolve_selected_ui_dependency_winner_path(owner, cursor, &dependencies.images)
        {
            *cursor = winner;
        }
    }
    if let Some(aspect) = node.aspect.as_mut() {
        aspect
            .default
            .iter_mut()
            .chain(aspect.standard.iter_mut().map(|named| &mut named.visual))
            .chain(aspect.alternate.iter_mut().map(|named| &mut named.visual))
            .for_each(|visual| {
                if let Some(image) = visual.image.as_mut() {
                    resolve_image_path(image);
                }
                if let Some(image) = visual.font.as_mut().and_then(|font| font.image.as_mut()) {
                    resolve_image_path(image);
                }
            });
    }
    match &mut node.widget {
        SourceUiWidgetData::MultiIcon(entries) => entries
            .iter_mut()
            .filter_map(|entry| entry.image.as_mut())
            .for_each(resolve_image_path),
        SourceUiWidgetData::WorldMap(world_map) => world_map
            .layers
            .iter_mut()
            .filter_map(|layer| layer.icon.as_mut())
            .for_each(resolve_image_path),
        SourceUiWidgetData::Globe(globe) => {
            globe
                .primary_model
                .iter_mut()
                .chain(globe.clouds_model.iter_mut())
                .chain(globe.dot_model.iter_mut())
                .chain(globe.selected_dot_model.iter_mut())
                .chain(globe.pointer_model.iter_mut())
                .chain(globe.secondary_models.iter_mut().map(|(_, model)| model))
                .for_each(|model| {
                    if let Some(winner) = resolve_selected_ui_dependency_winner_path(
                        owner,
                        model.as_str(),
                        &dependencies.models,
                    ) {
                        *model = AssetPath::new(winner);
                    }
                });
            if let Some(cursor) = globe.dot_highlight_cursor.as_mut() {
                if let Some(winner) =
                    resolve_selected_ui_dependency_winner_path(owner, cursor, &dependencies.images)
                {
                    *cursor = winner;
                }
            }
        }
        SourceUiWidgetData::XmlEdit { document } => {
            if let Some(document) = document {
                if let Some(winner) = resolve_selected_ui_dependency_winner_path(
                    owner,
                    document.as_str(),
                    &dependencies.documents,
                ) {
                    *document = AssetPath::new(winner);
                }
            }
        }
        _ => {}
    }
    node.events
        .iter_mut()
        .flat_map(|block| &mut block.events)
        .chain(node.hotkeys.iter_mut().map(|hotkey| &mut hotkey.event))
        .for_each(|event| {
            rewrite_ui_event_image_or_cursor_path_to_selected_winner(
                owner,
                event,
                &dependencies.images,
            );
        });
    node.children.iter_mut().for_each(|child| {
        rewrite_ui_source_node_dependency_paths_to_selected_winners(owner, child, dependencies);
    });
}

fn rewrite_ui_event_image_or_cursor_path_to_selected_winner(
    owner: &str,
    event: &mut SourceUiEvent,
    winners: &BTreeMap<String, String>,
) {
    if event.message.eq_ignore_ascii_case("UI_SET_IMAGE")
        || event.message.eq_ignore_ascii_case("UI_SETCURSOR")
    {
        if let Some(authored) = event.string.as_mut() {
            if let Some(winner) =
                resolve_selected_ui_dependency_winner_path(owner, authored, winners)
            {
                *authored = winner;
            }
        }
    }
    if let Some(child) = event.child.as_deref_mut() {
        rewrite_ui_event_image_or_cursor_path_to_selected_winner(owner, child, winners);
    }
}
