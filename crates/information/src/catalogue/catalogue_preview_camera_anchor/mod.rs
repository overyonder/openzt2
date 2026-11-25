use std::{collections::BTreeMap, io};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

#[derive(Asset, TypePath)]
pub(crate) struct CataloguePreviewCameraAnchor(pub(crate) Vec3);

#[derive(Resource)]
pub(crate) struct CataloguePreviewCameraAnchorSource(
    pub(crate) Handle<CataloguePreviewCameraAnchor>,
);

#[derive(Default, TypePath)]
pub(crate) struct CataloguePreviewCameraAnchorLoader;

impl AssetLoader for CataloguePreviewCameraAnchorLoader {
    type Asset = CataloguePreviewCameraAnchor;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let document = parse_blue_fang_source_document(
            AssetPath::new(context.path().path().to_string_lossy()),
            &bytes,
        )
        .map_err(io::Error::other)?;
        let child = child_named;
        let nodes = child(&document.root, "BFSceneGraphComponent")
            .and_then(|node| child(node, "extraNodes"))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing camera attachments")
            })?;
        let named: BTreeMap<_, _> = nodes
            .element_children()
            .filter_map(|node| Some((node.attribute_named_any(&["name"])?, node)))
            .collect();
        let mut next = Some("buy_panel_fist");
        let mut anchor = Vec3::ZERO;
        let mut depth = 0;
        while let Some(name) = next {
            if depth >= named.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "camera attachment cycle",
                ));
            }
            let node = named.get(name).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("missing camera attachment {name}"),
                )
            })?;
            if let Some(rotation) = child(node, "rotation") {
                let row = |name| child(rotation, name).map_or(Ok(Vec3::ZERO), parse_vector);
                anchor =
                    Mat3::from_cols(row("row0")?, row("row1")?, row("row2")?).transpose() * anchor;
            }
            if let Some(position) = child(node, "position") {
                anchor += parse_vector(position)?;
            }
            next = node.attribute_named_any(&["parent"]);
            depth += 1;
        }
        Ok(CataloguePreviewCameraAnchor(anchor))
    }
}

fn child_named<'a>(
    node: &'a OrderedSourceDocumentNode,
    name: &str,
) -> Option<&'a OrderedSourceDocumentNode> {
    node.element_children()
        .find(|child| child.name.eq_ignore_ascii_case(name))
}

fn parse_vector(node: &OrderedSourceDocumentNode) -> io::Result<Vec3> {
    let number = |name| {
        node.attribute_named_any(&[name])
            .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
            .filter(|value| value.is_finite())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid camera attachment axis {name}"),
                )
            })
    };
    Ok(Vec3::new(number("x")?, number("y")?, number("z")?))
}

pub(crate) fn load_catalogue_preview_camera_anchor(
    mut commands: Commands,
    server: Res<AssetServer>,
) {
    commands.insert_resource(CataloguePreviewCameraAnchorSource(
        server.load("ui/fx/cameraobject.xml"),
    ));
}
