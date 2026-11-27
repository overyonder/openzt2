//! Archive-selected dependency winners and image dimensions for UI source lowering.

use std::{collections::BTreeMap, io::Cursor, path::Path};

use crate::{
    asset_source::AssetArchives,
    {
        model_source::native_model_source_lowering::native_model_scene_labelled_asset_path,
        source_document::{
            blue_fang_source_dependency_reference_discovery::source_scene_component_prefers_available_blue_fang_model,
            ordered_source_document_types::{
                OrderedSourceDocument, OrderedSourceDocumentAttribute, OrderedSourceDocumentChild,
                OrderedSourceDocumentChildren, OrderedSourceDocumentNode,
            },
            path::AssetPath,
        },
        ui_document::source::lower::authored_ui_document_lowering::{
            UiResolvedDependencies, CAMPAIGN_ROW_DOCUMENT, CAMPAIGN_SCENARIO_ROW_DOCUMENT,
            DISPLAY_RESOLUTION_ROW_DOCUMENT, FINANCE_VALUE_ROW_DOCUMENT,
            GRAPHICS_DETAIL_SETTING_ROW_DOCUMENT, LOAD_SLOT_ROW_DOCUMENT, MULTILIST_ROW_DOCUMENTS,
            NATIVE_FINANCE_ROW_DOCUMENTS, OVERVIEW_LEGEND_ROW_DOCUMENT, PROFILE_ROW_DOCUMENT,
            ZOO_MESSAGE_ROW_DOCUMENT,
        },
    },
};

#[cfg(test)]
mod tests;

pub(in crate::ui_document::source) const SELECTED_SCRIPT_PATH_ATTRIBUTE: &str =
    "openzt2SelectedScriptPath";

pub(super) fn annotate_ui_source_script_dependencies(
    document: &OrderedSourceDocument,
    scripts: &BTreeMap<String, String>,
) -> Option<OrderedSourceDocument> {
    fn annotate(
        source: &OrderedSourceDocumentNode,
        prefix: &str,
        scripts: &BTreeMap<String, String>,
    ) -> OrderedSourceDocumentNode {
        let mut attributes = source.attributes.clone();
        if source.attribute("msg") == Some("ZT_RUN_SCRIPT") {
            attributes.retain(|attribute| attribute.name() != SELECTED_SCRIPT_PATH_ATTRIBUTE);
            if let Some(reference) = script_payload(source).and_then(ui_script_reference) {
                let key = AssetPath::new(reference).key();
                if let Some(winner) = scripts.get(&format!("{prefix}{key}")) {
                    attributes.push(OrderedSourceDocumentAttribute::new_synthetic_ordered_source_document_attribute(
                        SELECTED_SCRIPT_PATH_ATTRIBUTE, winner.as_str(),
                    ));
                }
            }
        }
        let children = source
            .child_items()
            .iter()
            .map(|child| match child {
                OrderedSourceDocumentChild::Element(node) => {
                    OrderedSourceDocumentChild::Element(Box::new(annotate(node, prefix, scripts)))
                }
                child => child.clone(),
            })
            .collect();
        OrderedSourceDocumentNode {
            name: source.name.clone(),
            attributes,
            children: OrderedSourceDocumentChildren::from_ordered_source_document_child_items(
                children,
            ),
            span: source.span,
        }
    }
    let prefix = format!("{}\0", document.path.key());
    let (first, _) = scripts.range(prefix.clone()..).next()?;
    if !first.starts_with(&prefix) {
        return None;
    }
    Some(OrderedSourceDocument {
        path: document.path.clone(),
        format: document.format,
        root: annotate(&document.root, &prefix, scripts),
    })
}

fn ui_script_reference(command: &str) -> Option<&str> {
    command.split_ascii_whitespace().nth(1)
}

fn script_payload(node: &OrderedSourceDocumentNode) -> Option<&str> {
    node.attribute("string").or_else(|| node.attribute("value"))
}

pub(super) fn plan_selected_ui_source_dependency_winners_and_image_dimensions(
    ordered_ui_source_documents: &[OrderedSourceDocument],
    asset_archives: &AssetArchives,
) -> UiResolvedDependencies {
    fn visit_ordered_ui_source_document_node_attributes(
        source_document_path: &str,
        source_document_node: &OrderedSourceDocumentNode,
        asset_archives: &AssetArchives,
        resolved_dependencies: &mut UiResolvedDependencies,
    ) {
        if source_document_node.attribute("msg") == Some("ZT_RUN_SCRIPT") {
            if let Some(reference) =
                script_payload(source_document_node).and_then(ui_script_reference)
            {
                if let Some(winner) = asset_archives
                    .resolve_script_reference(Path::new(source_document_path), reference)
                {
                    let key = AssetPath::new(reference).key();
                    resolved_dependencies.scripts.insert(
                        format!("{source_document_path}\0{key}"),
                        winner.to_string_lossy().replace('\\', "/"),
                    );
                }
            }
        }
        let mut insert_selected_dependency_winner =
            |authored_reference: &str, selected_winner_path: String| {
                let lookup_keys = [
                    AssetPath::new(authored_reference).key(),
                    AssetPath::new(&selected_winner_path).key(),
                ];
                let selected_winner_extension = selected_winner_path
                    .split_once('#')
                    .map_or(selected_winner_path.as_str(), |(path, _)| path)
                    .rsplit_once('.')
                    .map_or("", |(_, extension)| extension);
                match selected_winner_extension {
                    "bmp" | "cur" | "dds" | "jpg" | "jpeg" | "png" | "tga" => {
                        lookup_keys.into_iter().for_each(|lookup_key| {
                            resolved_dependencies
                                .images
                                .insert(format!("*\0{lookup_key}"), selected_winner_path.clone());
                        });
                    }
                    "nif" | "bfb" | "glb" | "gltf" => {
                        let selected_scene_labelled_asset_path =
                            native_model_scene_labelled_asset_path(&selected_winner_path);
                        lookup_keys
                            .into_iter()
                            .chain(std::iter::once(
                                AssetPath::new(&selected_scene_labelled_asset_path).key(),
                            ))
                            .for_each(|lookup_key| {
                                resolved_dependencies.models.insert(
                                    format!("*\0{lookup_key}"),
                                    selected_scene_labelled_asset_path.clone(),
                                );
                            });
                    }
                    "wav" | "mp3" | "ogg" => {
                        let selected_winner_basename = selected_winner_path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&selected_winner_path);
                        let selected_winner_stem = selected_winner_basename
                            .rsplit_once('.')
                            .map_or(selected_winner_basename, |(stem, _)| stem);
                        lookup_keys.into_iter().for_each(|lookup_key| {
                            resolved_dependencies
                                .audio
                                .insert(format!("*\0{lookup_key}"), selected_winner_path.clone());
                        });
                        resolved_dependencies
                            .audio
                            .insert(format!("*\0{selected_winner_stem}"), selected_winner_path);
                    }
                    "xml" | "zt2" | "old" => {
                        lookup_keys.into_iter().for_each(|lookup_key| {
                            resolved_dependencies
                                .documents
                                .insert(format!("*\0{lookup_key}"), selected_winner_path.clone());
                        });
                    }
                    _ => {}
                }
            };
        if source_document_node.attribute("msg") == Some("ZT_GRAPH_SELECTION") {
            if let Some(layout) = source_document_node
                .attribute("string")
                .or_else(|| source_document_node.attribute("value"))
                .and_then(|payload| payload.split_ascii_whitespace().nth(2))
            {
                if let Some(winner) =
                    asset_archives.resolve_ui_reference(Path::new(source_document_path), layout)
                {
                    insert_selected_dependency_winner(
                        layout,
                        winner.to_string_lossy().replace('\\', "/"),
                    );
                }
            }
        }

        for source_attribute in &source_document_node.attributes {
            let authored_reference = source_attribute.value().trim();
            if authored_reference.is_empty() {
                continue;
            }
            let authored_reference_without_label = authored_reference
                .split_once('#')
                .map_or(authored_reference, |(path, _)| path);
            if authored_reference_without_label
                .chars()
                .any(char::is_whitespace)
                && !Path::new(authored_reference_without_label)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| !extension.chars().any(char::is_whitespace))
            {
                continue;
            }
            if source_attribute.name().eq_ignore_ascii_case("directory")
                || source_attribute.name().eq_ignore_ascii_case("dir")
            {
                for selected_path in
                    asset_archives.source_paths_under(Path::new(authored_reference))
                {
                    let selected_path = selected_path.to_string_lossy().replace('\\', "/");
                    insert_selected_dependency_winner(&selected_path, selected_path.clone());
                }
            }
            let selected_winner_path = if source_attribute.name().eq_ignore_ascii_case("modelfile")
            {
                asset_archives.resolve_model_reference_with_blue_fang_bfb_preference(
                    Path::new(source_document_path),
                    authored_reference_without_label,
                    source_scene_component_prefers_available_blue_fang_model(
                        &source_document_node.name,
                        source_document_node.attribute("isBFR"),
                    ),
                )
            } else {
                asset_archives.resolve_ui_reference(
                    Path::new(source_document_path),
                    authored_reference_without_label,
                )
            }
            .or_else(|| {
                let reference = crate::original_game_asset_typo_fixups::corrected_ui_image_path(
                    &AssetPath::new(authored_reference_without_label).key(),
                )?;
                asset_archives.resolve_ui_reference(Path::new(source_document_path), reference)
            })
            .or_else(|| {
                authored_reference_without_label
                    .rsplit_once('.')
                    .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("cur"))
                    .then(|| {
                        let cursor =
                            crate::original_game_asset_typo_fixups::corrected_cursor_filename(
                                authored_reference_without_label,
                            );
                        format!("ui/cursor/{cursor}")
                    })
                    .and_then(|cursor_reference| {
                        asset_archives.resolve_ui_reference(
                            Path::new(source_document_path),
                            &cursor_reference,
                        )
                    })
            });
            let Some(selected_winner_path) = selected_winner_path else {
                continue;
            };
            let selected_winner_path = selected_winner_path.to_string_lossy().replace('\\', "/");
            insert_selected_dependency_winner(authored_reference, selected_winner_path);
        }
        if source_document_node.name.eq_ignore_ascii_case("ZTGlobe")
            && source_document_node.attribute("screamThreshold").is_some()
        {
            const SCREAM: &str = "ui/sound/WorldScreams.wav";
            if let Some(path) =
                asset_archives.resolve_ui_reference(Path::new(source_document_path), SCREAM)
            {
                insert_selected_dependency_winner(
                    SCREAM,
                    path.to_string_lossy().replace('\\', "/"),
                );
            }
        }
        drop(insert_selected_dependency_winner);
        source_document_node
            .element_children()
            .for_each(|child_node| {
                visit_ordered_ui_source_document_node_attributes(
                    source_document_path,
                    child_node,
                    asset_archives,
                    resolved_dependencies,
                );
            });
    }

    let mut resolved_dependencies = UiResolvedDependencies::default();
    for ordered_ui_source_document in ordered_ui_source_documents {
        let source_document_path = ordered_ui_source_document.path.key();
        for root_attribute in &ordered_ui_source_document.root.attributes {
            let synthetic_root_node =
                OrderedSourceDocumentNode::new_synthetic_ordered_source_document_node(
                    ordered_ui_source_document.root.name.clone(),
                    vec![root_attribute.clone()],
                    Default::default(),
                    ordered_ui_source_document.root.span,
                );
            visit_ordered_ui_source_document_node_attributes(
                &source_document_path,
                &synthetic_root_node,
                asset_archives,
                &mut resolved_dependencies,
            );
        }
        ordered_ui_source_document
            .root
            .element_children()
            .for_each(|source_document_node| {
                visit_ordered_ui_source_document_node_attributes(
                    &source_document_path,
                    source_document_node,
                    asset_archives,
                    &mut resolved_dependencies,
                );
            });
        // The original entity-information presenter instantiates this row
        // document from native code rather than an XML attribute. Resolve it
        // through the same selected archive overlay as explicit UI document
        // references so the typed list retains the winning Bevy dependency.
        if source_document_path == "ui/layout/entityinfo.xml" {
            if let Some(selected_need_row_path) = asset_archives
                .resolve_ui_reference(Path::new(&source_document_path), "needsitem.xml")
            {
                let selected_need_row_path =
                    selected_need_row_path.to_string_lossy().replace('\\', "/");
                ["ui/layout/needsitem.xml", selected_need_row_path.as_str()]
                    .into_iter()
                    .for_each(|lookup_key| {
                        resolved_dependencies.documents.insert(
                            format!("*\0{}", AssetPath::new(lookup_key).key()),
                            selected_need_row_path.clone(),
                        );
                    });
            }
        }
        if source_document_path == "ui/layout/shell.xml" {
            for row in [
                "filterlistitem.xml",
                "verticaldivide.xml",
                "researchbutton.xml",
            ] {
                if let Some(path) =
                    asset_archives.resolve_ui_reference(Path::new(&source_document_path), row)
                {
                    let path = path.to_string_lossy().replace('\\', "/");
                    resolved_dependencies.documents.insert(
                        format!("*\0{}", AssetPath::new(&format!("ui/layout/{row}")).key()),
                        path,
                    );
                }
            }
        }
        // The original photo-album component instantiates the camera exposure
        // and album-choice rows from native code rather than XML attributes.
        // Resolve those two shipped row documents through the selected overlay
        // so the authored live collections borrow ordinary typed fragments.
        if source_document_path == "ui/layout/photoalbum.xml" {
            for native_photo_album_row_document in ["exposure.xml", "albumsentry.xml"] {
                let Some(selected_photo_album_row_path) = asset_archives.resolve_ui_reference(
                    Path::new(&source_document_path),
                    native_photo_album_row_document,
                ) else {
                    continue;
                };
                let selected_photo_album_row_path = selected_photo_album_row_path
                    .to_string_lossy()
                    .replace('\\', "/");
                [
                    format!("ui/layout/{native_photo_album_row_document}"),
                    selected_photo_album_row_path.clone(),
                ]
                .into_iter()
                .for_each(|lookup_key| {
                    resolved_dependencies.documents.insert(
                        format!("*\0{}", AssetPath::new(&lookup_key).key()),
                        selected_photo_album_row_path.clone(),
                    );
                });
            }
        }
        if source_document_path == "ui/layout/zoostatus.xml" {
            for (source_row_filename, canonical_fragment_path) in NATIVE_FINANCE_ROW_DOCUMENTS {
                insert_native_created_ui_document_dependency_selected_through_archive_overlay(
                    asset_archives,
                    &source_document_path,
                    source_row_filename,
                    canonical_fragment_path,
                    &mut resolved_dependencies,
                );
            }
        }
        if source_document_path == "ui/layout/balancesheetitem.xml" {
            insert_native_created_ui_document_dependency_selected_through_archive_overlay(
                asset_archives,
                &source_document_path,
                "econlistitem.xml",
                FINANCE_VALUE_ROW_DOCUMENT,
                &mut resolved_dependencies,
            );
        }
        // These lists are filled from native code with row documents that no
        // layout references, so the rows are planned with their owning layout.
        for (layout_path, row_source_path, canonical_row_document) in [
            (
                "ui/layout/load.xml",
                "ui/layout/loadentry.xml",
                LOAD_SLOT_ROW_DOCUMENT,
            ),
            (
                "ui/layout/freeformselection.xml",
                "ui/layout/campaign/campaign.xml",
                CAMPAIGN_ROW_DOCUMENT,
            ),
            (
                "ui/layout/freeformselection.xml",
                "ui/layout/campaign/unlocked.xml",
                CAMPAIGN_SCENARIO_ROW_DOCUMENT,
            ),
            (
                "ui/layout/message.xml",
                "ui/layout/messagenormal.xml",
                ZOO_MESSAGE_ROW_DOCUMENT,
            ),
            (
                "ui/layout/profiledialog.xml",
                "ui/layout/profileentry.xml",
                PROFILE_ROW_DOCUMENT,
            ),
            (
                "ui/layout/options.xml",
                "ui/layout/resolution.xml",
                DISPLAY_RESOLUTION_ROW_DOCUMENT,
            ),
            (
                "ui/layout/options.xml",
                "ui/layout/optionsslider.xml",
                GRAPHICS_DETAIL_SETTING_ROW_DOCUMENT,
            ),
        ] {
            if source_document_path == layout_path {
                insert_native_created_ui_document_dependency_selected_through_archive_overlay(
                    asset_archives,
                    &source_document_path,
                    row_source_path,
                    canonical_row_document,
                    &mut resolved_dependencies,
                );
            }
        }
        if source_document_path == "ui/layout/overview.xml" {
            insert_native_created_ui_document_dependency_selected_through_archive_overlay(
                asset_archives,
                &source_document_path,
                "legenditem.xml",
                OVERVIEW_LEGEND_ROW_DOCUMENT,
                &mut resolved_dependencies,
            );
            for native_marker_image_path in [
                "ui/zoomap/urhere.dds",
                "ui/zoomap/animalcircle.dds",
                "ui/zoomap/buildingsquare.dds",
            ] {
                let Some(selected_marker_image_path) = asset_archives.resolve_ui_reference(
                    Path::new(&source_document_path),
                    native_marker_image_path,
                ) else {
                    continue;
                };
                let selected_marker_image_path = selected_marker_image_path
                    .to_string_lossy()
                    .replace('\\', "/");
                [
                    native_marker_image_path,
                    selected_marker_image_path.as_str(),
                ]
                .into_iter()
                .for_each(|lookup_key| {
                    resolved_dependencies.images.insert(
                        format!("*\0{}", AssetPath::new(lookup_key).key()),
                        selected_marker_image_path.clone(),
                    );
                });
            }
        }
        // The original multilist component selects one native row document for
        // each live entity category. Resolve those shipped fragments through
        // the same archive overlay as explicit document references.
        if source_document_path == "ui/layout/multilist.xml" {
            for (_, fragment_path) in MULTILIST_ROW_DOCUMENTS {
                let source_row_path = fragment_path
                    .strip_prefix("ui/fragment/")
                    .expect("canonical multilist rows are UI fragments");
                let Some(selected_row_path) = asset_archives
                    .resolve_ui_reference(Path::new(&source_document_path), source_row_path)
                else {
                    continue;
                };
                let selected_row_path = selected_row_path.to_string_lossy().replace('\\', "/");
                [source_row_path, selected_row_path.as_str()]
                    .into_iter()
                    .for_each(|lookup_key| {
                        resolved_dependencies.documents.insert(
                            format!("*\0{}", AssetPath::new(lookup_key).key()),
                            selected_row_path.clone(),
                        );
                    });
            }
        }
    }

    let mut image_dimensions_by_selected_winner = BTreeMap::new();
    for (lookup_key, selected_winner_path) in resolved_dependencies.images.clone() {
        let selected_winner_key = AssetPath::new(&selected_winner_path).key();
        let image_dimensions = image_dimensions_by_selected_winner
            .entry(selected_winner_path.clone())
            .or_insert_with(|| {
                read_selected_ui_image_dimensions(asset_archives, &selected_winner_path)
            });
        let Some(image_dimensions) = *image_dimensions else {
            continue;
        };
        resolved_dependencies
            .image_dimensions
            .insert(selected_winner_key, image_dimensions);
        resolved_dependencies.image_dimensions.insert(
            lookup_key
                .strip_prefix("*\0")
                .unwrap_or(&lookup_key)
                .to_owned(),
            image_dimensions,
        );
    }
    resolved_dependencies
}

fn insert_native_created_ui_document_dependency_selected_through_archive_overlay(
    asset_archives: &AssetArchives,
    source_document_path: &str,
    source_row_filename: &str,
    canonical_fragment_path: &str,
    resolved_dependencies: &mut UiResolvedDependencies,
) {
    let Some(selected_row_path) =
        asset_archives.resolve_ui_reference(Path::new(source_document_path), source_row_filename)
    else {
        return;
    };
    let selected_row_path = selected_row_path.to_string_lossy().replace('\\', "/");
    [
        canonical_fragment_path,
        canonical_fragment_path
            .strip_prefix("ui/fragment/")
            .unwrap_or(canonical_fragment_path),
        selected_row_path.as_str(),
    ]
    .into_iter()
    .for_each(|lookup_key| {
        resolved_dependencies.documents.insert(
            format!("*\0{}", AssetPath::new(lookup_key).key()),
            selected_row_path.clone(),
        );
    });
}

fn read_selected_ui_image_dimensions(
    asset_archives: &AssetArchives,
    selected_image_path: &str,
) -> Option<[u32; 2]> {
    let selected_image_bytes = asset_archives
        .read_source(Path::new(selected_image_path))
        .ok()?;
    let selected_image_extension = Path::new(selected_image_path)
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    let image_dimensions = if selected_image_extension == "dds" {
        let direct_draw_surface =
            image_dds::ddsfile::Dds::read(&mut Cursor::new(selected_image_bytes)).ok()?;
        (
            direct_draw_surface.get_width(),
            direct_draw_surface.get_height(),
        )
    } else {
        image::ImageReader::new(Cursor::new(selected_image_bytes))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()?
    };
    Some([image_dimensions.0, image_dimensions.1])
}
