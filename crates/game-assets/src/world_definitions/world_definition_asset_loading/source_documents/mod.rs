use super::authored_binder_reference_resolution::AuthoredBinderSources;
use super::source_reference_discovery::{
    authored_biome_automatic_placement_supplement_source_paths,
    authored_hireable_generic_binder_family_variant_source_paths,
    collect_authored_typed_binder_family_references,
    collect_authored_world_definition_source_document_dependency_references,
    ordered_source_document_contains_element_named,
    resolved_world_definition_source_documents_declare_viewing_area_type,
    AuthoredWorldDefinitionSourceReference,
};
use super::unresolved_reference;
use crate::asset_source::AssetArchives;
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_collection_precedence::reorder_ordered_source_documents_by_requested_normalized_path_keys_and_last_definition_precedence,
    ordered_source_document_types::OrderedSourceDocument, path::AssetPath,
};
use bevy::asset::LoadContext;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
};

pub(super) async fn load_world_definition_source_documents(
    archives: &AssetArchives,
    binder_source_paths: &AuthoredBinderSources,
    context: &mut LoadContext<'_>,
    path: &str,
    primary: OrderedSourceDocument,
) -> io::Result<(Vec<OrderedSourceDocument>, bool)> {
    let mut documents = vec![primary];
    let mut uses_binder_sources = false;
    let mut loaded = BTreeSet::from([path.to_owned()]);
    let mut dependencies = BTreeMap::<String, BTreeSet<String>>::new();
    let mut supplements = Vec::<std::path::PathBuf>::new();
    if documents[0].root.name.eq_ignore_ascii_case("ZTPuzzleMgr") {
        supplements.push("ui/modes/modes.xml".into());
    }
    if path.eq_ignore_ascii_case("ai/guestmgr.xml") {
        supplements.push("ai/ztai.xml".into());
    }
    if ordered_source_document_contains_element_named(&documents[0].root, "ZTAIShowMgr") {
        for supplement in ["config/ztshowtimermanager.xml", "config/showmixer.xml"] {
            if archives
                .resolve_source_reference(Path::new(path), supplement)
                .is_some()
            {
                supplements.push(supplement.into());
            }
        }
    }
    if path.eq_ignore_ascii_case("config/adoption.xml") {
        supplements.extend(
            archives
                .source_paths_under(Path::new("xpInfo"))
                .into_iter()
                .filter(|source_path| {
                    source_path.parent().is_some_and(|parent| {
                        parent.to_string_lossy().eq_ignore_ascii_case("xpInfo")
                    }) && source_path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
                }),
        );
    }
    supplements.extend(authored_biome_automatic_placement_supplement_source_paths(
        archives,
        &documents[0],
    ));
    supplements.extend(
        authored_hireable_generic_binder_family_variant_source_paths(archives, path, &documents[0]),
    );
    for source_path in supplements {
        let source_path = source_path.to_string_lossy().replace('\\', "/");
        if loaded.insert(source_path.clone()) {
            documents.push(read_source_document(context, &source_path).await?);
            dependencies
                .entry(path.to_owned())
                .or_default()
                .insert(source_path);
        }
    }

    let mut families = collect_authored_typed_binder_family_references(&documents);
    while let Some((from, reference)) = families.pop_first() {
        uses_binder_sources = true;
        let Some(dependency) = resolve_typed_binder_family_source_path(
            archives,
            binder_source_paths,
            &from,
            &reference,
        )?
        else {
            continue;
        };
        let dependency = dependency.to_string_lossy().replace('\\', "/");
        dependencies
            .entry(from)
            .or_default()
            .insert(dependency.clone());
        if !loaded.insert(dependency.clone()) {
            continue;
        }
        let document = read_source_document(context, &dependency).await?;
        families.extend(collect_authored_typed_binder_family_references(
            std::slice::from_ref(&document),
        ));
        documents.push(document);
    }

    if documents.iter().any(|document| {
        ordered_source_document_contains_element_named(&document.root, "ZTTransportSkyTrack")
    }) {
        let rope = "entities/transportation/track/ai/testropeobj.xml";
        if loaded.insert(rope.to_owned()) {
            documents.push(read_source_document(context, rope).await?);
            dependencies
                .entry(path.to_owned())
                .or_default()
                .insert(rope.to_owned());
        }
    }
    if path
        .to_ascii_lowercase()
        .starts_with("ui/zoopedia/entries/")
    {
        let mut entries = archives
            .resolved_paths()
            .iter()
            .filter(|source_path| {
                source_path
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_ascii_lowercase()
                    .starts_with("ui/zoopedia/entries/")
                    && source_path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            })
            .cloned()
            .collect::<Vec<_>>();
        entries.sort_unstable_by_key(|source_path| {
            source_path
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
        });
        for source_path in entries {
            let source_path = source_path.to_string_lossy().replace('\\', "/");
            if loaded.insert(source_path.clone()) {
                documents.push(read_source_document(context, &source_path).await?);
            }
        }
    }
    if resolved_world_definition_source_documents_declare_viewing_area_type(&documents) {
        if let Some(dependency) =
            archives.resolve_source_reference(Path::new(path), "ViewingArea.xml")
        {
            let dependency = dependency.to_string_lossy().replace('\\', "/");
            if loaded.insert(dependency.clone()) {
                documents.push(read_source_document(context, &dependency).await?);
                dependencies
                    .entry(path.to_owned())
                    .or_default()
                    .insert(dependency);
            }
        }
    }
    let mut references =
        collect_authored_world_definition_source_document_dependency_references(&documents);
    while let Some((from, reference)) = references.pop_first() {
        let dependency = match reference {
            AuthoredWorldDefinitionSourceReference::Document(reference) => archives
                .resolve_source_reference(Path::new(&from), &reference)
                .ok_or_else(|| unresolved_reference(&from, "source document", &reference))?,
            AuthoredWorldDefinitionSourceReference::OptionalDocument(reference) => {
                let Some(dependency) =
                    archives.resolve_source_reference(Path::new(&from), &reference)
                else {
                    continue;
                };
                dependency
            }
            AuthoredWorldDefinitionSourceReference::GateCollisionType(actor_type) => {
                let _ = context
                    .read_asset_bytes("config/gamectxt.xml")
                    .await
                    .map_err(io::Error::other)?;
                uses_binder_sources = true;
                binder_source_paths
                    .resolve_binder_type_source_path(archives, &from, &actor_type)?
                    .ok_or_else(|| {
                        unresolved_reference(&from, "authored binder type", &actor_type)
                    })?
            }
        };
        let dependency = dependency.to_string_lossy().replace('\\', "/");
        dependencies
            .entry(from)
            .or_default()
            .insert(dependency.clone());
        if !loaded.insert(dependency.clone()) {
            continue;
        }
        let document = read_source_document(context, &dependency).await?;
        references.extend(
            collect_authored_world_definition_source_document_dependency_references(
                std::slice::from_ref(&document),
            ),
        );
        documents.push(document);
    }
    let mut order = Vec::with_capacity(loaded.len());
    dependency_order(path, &dependencies, &mut BTreeSet::new(), &mut order);
    reorder_ordered_source_documents_by_requested_normalized_path_keys_and_last_definition_precedence(&mut documents, &order);
    Ok((documents, uses_binder_sources))
}

/// A typed-binder family names a binder type, which is usually also the stem
/// of the file declaring it. Where the same-named file is another kind of
/// document, such as `guests/YoungMale/Guest_Young_M.xml`, an animation graph
/// beside the `Guest_Young_M` binder in `guests/ai/YoungMale.xml`, the binder
/// index names the declaring file, including for families with no same-named
/// file. Fall back to file-name resolution when the index has no declaration.
fn resolve_typed_binder_family_source_path(
    archives: &AssetArchives,
    binder_source_paths: &AuthoredBinderSources,
    from: &str,
    family: &str,
) -> io::Result<Option<std::path::PathBuf>> {
    if let Some(declaration) =
        binder_source_paths.resolve_binder_type_source_path(archives, from, family)?
    {
        return Ok(Some(declaration));
    }
    Ok(archives.resolve_source_reference(Path::new(from), family))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_source::{
        create_bevy_asset_source_for_enabled_z2f_archives, tests::TestDirectory,
    };

    #[test]
    fn family_lookup_distinguishes_indexed_missing_and_ambiguous_declarations() -> io::Result<()> {
        let directory = TestDirectory::new()?;
        let archive = directory.archive(
            "guests.z2f",
            &[
                (
                    "config/gamectxt.xml",
                    br#"<BFGameContext><BFBinderRoot><root path="guests/ai"/></BFBinderRoot></BFGameContext>"#,
                ),
                ("guests/ai/adult.xml", br#"<BFTypedBinder binderType="Guest_Adult"/>"#),
                ("guests/ai/malformed.xml", b"this is not a binder document"),
                ("guests/ai/young.xml", br#"<BFTypedBinder binderType="Guest_Young"/>"#),
                ("guests/ai/other.xml", br#"<BFTypedBinder binderType="Guest_Young"/>"#),
                ("guests/guest_young.xml", b"<BFAnimationGraph/>"),
                ("guests/guest_infant.xml", b"<BFAnimationGraph/>"),
            ],
        )?;
        let (_, archives) = create_bevy_asset_source_for_enabled_z2f_archives(vec![archive])?;
        let index = archives.authored_binder_sources();
        let resolve = |family| {
            resolve_typed_binder_family_source_path(&archives, &index, "guests/variant.xml", family)
        };

        assert_eq!(resolve("Guest_Adult")?, Some("guests/ai/adult.xml".into()));
        assert_eq!(
            resolve("Guest_Infant")?,
            Some("guests/guest_infant.xml".into())
        );
        assert_eq!(
            resolve("Guest_Young").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        Ok(())
    }
}

pub(super) async fn read_source_document(
    context: &mut LoadContext<'_>,
    path: &str,
) -> io::Result<OrderedSourceDocument> {
    let bytes = context
        .read_asset_bytes(path.to_owned())
        .await
        .map_err(io::Error::other)?;
    parse_blue_fang_source_document(AssetPath::new(path), &bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn dependency_order(
    path: &str,
    dependencies: &BTreeMap<String, std::collections::BTreeSet<String>>,
    visited: &mut std::collections::BTreeSet<String>,
    output: &mut Vec<String>,
) {
    if !visited.insert(path.to_owned()) {
        return;
    }
    dependencies
        .get(path)
        .into_iter()
        .flatten()
        .for_each(|dependency| dependency_order(dependency, dependencies, visited, output));
    output.push(path.to_owned());
}
