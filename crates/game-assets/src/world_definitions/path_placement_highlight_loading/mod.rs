use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};

use super::path_placement_highlight_asset::{
    PathPlacementHighlightAsset, PathPlacementHighlightCurve,
};
use crate::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

#[derive(Default, TypePath)]
pub(super) struct PathPlacementHighlightLoader;

impl AssetLoader for PathPlacementHighlightLoader {
    type Asset = PathPlacementHighlightAsset;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        (): &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = context.path().path().to_string_lossy().into_owned();
        let document = parse_blue_fang_source_document(AssetPath::new(&path), &bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if document.root.name != "CurveMap" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "path highlight document has no CurveMap",
            ));
        }
        let mut curves = Vec::new();
        for curve in document
            .root
            .element_children()
            .filter(|node| node.name == "curve")
        {
            let texture = curve.attribute("texture").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "path highlight curve has no texture",
                )
            })?;
            let requirements = |name: &str, cardinal: bool| -> io::Result<Vec<[i32; 3]>> {
                curve
                    .element_children()
                    .filter(|node| node.name == name)
                    .flat_map(OrderedSourceDocumentNode::element_children)
                    .filter(|node| node.name == "TriangleID")
                    .map(|node| {
                        let value = |key| {
                            node.attribute(key)
                                .and_then(|value| value.parse::<i32>().ok())
                                .ok_or_else(|| {
                                    io::Error::new(
                                        io::ErrorKind::InvalidData,
                                        "invalid path highlight TriangleID",
                                    )
                                })
                        };
                        let direction = value("dir")?;
                        if !(0..8).contains(&direction) || (cardinal && direction % 2 != 0) {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "invalid path curve TriangleID direction",
                            ));
                        }
                        Ok([value("x")?, value("y")?, direction])
                    })
                    .collect()
            };
            curves.push(PathPlacementHighlightCurve {
                texture: context.load(format!("entities/objects/paths/shared/{texture}")),
                on: requirements("on", true)?,
                off: requirements("off", true)?,
                curb_endpoints: [
                    requirements("curb1", false)?.first().copied(),
                    requirements("curb2", false)?.first().copied(),
                ],
            });
        }
        Ok(PathPlacementHighlightAsset {
            fallback: context.load("entities/objects/paths/shared/west_hilite.tga"),
            curves,
        })
    }
}
