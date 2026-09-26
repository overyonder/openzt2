//! Primary, supporting, and transitively referenced UI source-document loading.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

use bevy::asset::LoadContext;

use crate::assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
};

use super::{
    resolver::resolution_profile::SelectedUiSourceResolutionProfile,
    ui_template_source_path_discovery::collect_referenced_ui_template_source_paths,
};

pub(super) async fn load_primary_supporting_and_referenced_ui_source_documents(
    primary_source_path: &str,
    primary_source_bytes: &[u8],
    asset_archives: &crate::asset_source::AssetArchives,
    template_declaration_paths: &BTreeMap<String, String>,
    load_context: &mut LoadContext<'_>,
) -> io::Result<Vec<OrderedSourceDocument>> {
    let mut source_documents = vec![parse_blue_fang_source_document(
        AssetPath::new(primary_source_path),
        primary_source_bytes,
    )
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?];
    let mut planned_source_paths = BTreeSet::from([AssetPath::new(primary_source_path).key()]);

    let (_, winning_asset_paths) = asset_archives.resolved_paths();
    let load_authored_biome_panels =
        AssetPath::new(primary_source_path).key() == "ui/layout/shell.xml";
    let supporting_source_paths =
        SelectedUiSourceResolutionProfile::for_primary_source_path(primary_source_path)
            .supporting_source_paths()
            .into_iter()
            .chain(winning_asset_paths.iter().filter_map(|path| {
                (load_authored_biome_panels
                    && path.parent() == Some(Path::new("biomes"))
                    && path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml")))
                .then(|| path.to_string_lossy().into_owned())
            }));
    for supporting_source_path in supporting_source_paths {
        load_ui_source_document_once(
            &mut source_documents,
            &mut planned_source_paths,
            &supporting_source_path,
            load_context,
        )
        .await?;
    }

    loop {
        let unplanned_template_source_paths = collect_referenced_ui_template_source_paths(
            &source_documents,
            template_declaration_paths,
        )
        .into_iter()
        .filter(|source_path| !planned_source_paths.contains(&AssetPath::new(source_path).key()))
        .collect::<Vec<_>>();
        if unplanned_template_source_paths.is_empty() {
            break;
        }
        for template_source_path in unplanned_template_source_paths {
            load_ui_source_document_once(
                &mut source_documents,
                &mut planned_source_paths,
                &template_source_path,
                load_context,
            )
            .await?;
        }
    }

    Ok(source_documents)
}

async fn load_ui_source_document_once(
    source_documents: &mut Vec<OrderedSourceDocument>,
    planned_source_paths: &mut BTreeSet<String>,
    source_path: &str,
    load_context: &mut LoadContext<'_>,
) -> io::Result<()> {
    let normalized_source_path = AssetPath::new(source_path).key();
    if !planned_source_paths.insert(normalized_source_path.clone()) {
        return Ok(());
    }
    let source_bytes = load_context
        .read_asset_bytes(normalized_source_path.clone())
        .await
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    source_documents.push(
        parse_blue_fang_source_document(AssetPath::new(&normalized_source_path), &source_bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
    );
    Ok(())
}
