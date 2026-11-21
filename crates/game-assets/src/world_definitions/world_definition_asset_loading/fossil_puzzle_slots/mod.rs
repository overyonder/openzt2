use std::io;

use bevy::{
    asset::{AssetPath, LoadContext},
    gltf::Gltf,
};
use openzt2_game_data::{
    world_definitions::{
        document::WorldDefinitionDocument, extinct_animal_recovery::FossilSlotDefinition,
    },
    AssetId,
};

use crate::{
    asset_source::AssetArchives,
    model_source::native_model_source_lowering::native_model_scene_labelled_asset_path,
    scene_prefab::ScenePrefabAsset,
    source_document::ordered_source_document_types::OrderedSourceDocument,
};

use super::{
    authored_binder_reference_resolution::AuthoredBinderSources,
    presentation_dependencies::resolve_world_definition_presentation_models,
    source_documents::read_source_document, unresolved_reference,
};

pub(super) async fn lower_fossil_puzzle_background_slots(
    archives: &AssetArchives,
    binders: &AuthoredBinderSources,
    context: &mut LoadContext<'_>,
    path: &str,
    documents: &[OrderedSourceDocument],
    document: &mut WorldDefinitionDocument,
) -> io::Result<bool> {
    let Some(puzzle) = documents.iter().find(|document| {
        document.path.key().eq_ignore_ascii_case(path)
            && document.root.name.eq_ignore_ascii_case("ZTPuzzle")
    }) else {
        return Ok(false);
    };
    let Some(animal) = puzzle.root.attribute_named_any(&["animal"]) else {
        return Ok(false);
    };
    let binder = format!("{animal}Background");
    let source = binders
        .resolve_binder_type_source_path(archives, path, &binder)?
        .ok_or_else(|| unresolved_reference(path, "fossil puzzle background", &binder))?;
    let source = source.to_string_lossy();
    let background = read_source_document(context, &source).await?;
    let (_, models) =
        resolve_world_definition_presentation_models(archives, context, &source, &[background])
            .await?;
    let mut models = models.values();
    let model = models
        .next()
        .filter(|first| models.all(|other| other == *first))
        .ok_or_else(|| unresolved_reference(path, "unambiguous fossil puzzle model", &binder))?;
    let scene_path = native_model_scene_labelled_asset_path(model);
    let scene_path = AssetPath::parse(&scene_path);
    let model = context
        .load_builder()
        .load_value::<Gltf>(scene_path.without_label())
        .await
        .map_err(io::Error::other)?;
    let background = scene_path
        .label()
        .and_then(|label| model.get_labeled(label))
        .and_then(|scene| scene.get::<ScenePrefabAsset>())
        .ok_or_else(|| unresolved_reference(path, "fossil puzzle scene", &binder))?;
    for piece in puzzle
        .root
        .element_children()
        .filter(|child| child.name.eq_ignore_ascii_case("pieces"))
        .flat_map(|group| group.element_children())
    {
        let node = piece
            .attribute_named_any(&["node"])
            .ok_or_else(|| unresolved_reference(path, "fossil puzzle piece node", &binder))?;
        let pose = background
            .prefab_attachment_transform(AssetId::from_key(&node.to_ascii_lowercase()))
            .ok_or_else(|| unresolved_reference(path, "fossil puzzle slot", node))?;
        document.fossil_slots.push(FossilSlotDefinition {
            id: AssetId::from_virtual_path(&format!("fossil-slots/{animal}/{node}")),
            transform: pose.to_cols_array_2d(),
            tolerance_cm: 0,
            tolerance_degrees: 0,
        });
    }
    Ok(true)
}
