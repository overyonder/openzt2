use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document,
    ordered_source_document_types::OrderedSourceDocumentNode, path::AssetPath,
};

#[derive(Clone, Copy)]
struct Triangle {
    tile: IVec2,
    direction: u8,
}

struct Curve {
    on: Vec<Triangle>,
    off: Vec<Triangle>,
    curbs: Option<[Triangle; 2]>,
}

#[derive(Asset, TypePath)]
pub(super) struct PathCurbPatterns(Vec<Curve>);

#[derive(Default, TypePath)]
pub(super) struct PathCurbPatternLoader;

impl AssetLoader for PathCurbPatternLoader {
    type Asset = PathCurbPatterns;
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
        let mut curves = Vec::new();
        for node in document
            .root
            .element_children()
            .filter(|node| node.name == "curve")
        {
            let triangles = |key| {
                node.element_children()
                    .filter(|child| child.name == key)
                    .flat_map(OrderedSourceDocumentNode::element_children)
                    .filter(|triangle| triangle.name == "TriangleID")
                    .map(parse_triangle)
                    .collect::<io::Result<Vec<_>>>()
            };
            let first = triangles("curb1")?;
            let second = triangles("curb2")?;
            curves.push(Curve {
                on: triangles("on")?,
                off: triangles("off")?,
                curbs: first.first().zip(second.first()).map(|(&a, &b)| [a, b]),
            });
        }
        Ok(PathCurbPatterns(curves))
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

fn parse_triangle(node: &OrderedSourceDocumentNode) -> io::Result<Triangle> {
    let value = |key| {
        node.attribute(key)
            .and_then(|value| value.parse::<i32>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid curb triangle"))
    };
    let direction = value("dir")?;
    if !(0..8).contains(&direction) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid curb direction",
        ));
    }
    Ok(Triangle {
        tile: IVec2::new(value("x")?, value("y")?),
        direction: direction as u8,
    })
}

#[derive(Resource)]
pub(super) struct PathCurbPatternAsset(pub Handle<PathCurbPatterns>);

impl FromWorld for PathCurbPatternAsset {
    fn from_world(world: &mut World) -> Self {
        Self(
            world
                .resource::<AssetServer>()
                .load("ui/modes/pathcurves.xml"),
        )
    }
}

type CurbEdge = [IVec3; 2];

impl PathCurbPatterns {
    pub(super) fn pair(
        &self,
        centre: IVec3,
        direction: u8,
        half_width: i32,
        occupied: impl Fn(IVec3) -> bool,
    ) -> Option<[CurbEdge; 2]> {
        let turns = ((direction + 2) % 8) / 2;
        let resolve = |triangle: Triangle| {
            let mut tile = triangle.tile;
            for _ in 0..turns {
                tile = IVec2::new(tile.y, -tile.x);
            }
            (
                centre + IVec3::new(tile.x, -tile.y, 0) * (half_width * 4),
                (triangle.direction + turns * 2) % 8,
            )
        };
        let present = |triangle| {
            let (tile, side) = resolve(triangle);
            occupied(tile + (compass(side) * half_width).extend(0))
        };
        let curve = self.0.iter().find(|curve| {
            curve.on.iter().copied().all(present) && !curve.off.iter().copied().any(present)
        })?;
        let [a, b] = curve.curbs?;
        let edge = |triangle| {
            let (tile, side) = resolve(triangle);
            let (start, end) = match side {
                0 => (IVec2::new(-1, -1), IVec2::new(1, -1)),
                1 => (IVec2::new(1, -1), IVec2::new(-1, 1)),
                2 => (IVec2::new(1, -1), IVec2::new(1, 1)),
                3 => (IVec2::ONE, IVec2::new(-1, -1)),
                4 => (IVec2::new(-1, 1), IVec2::ONE),
                5 => (IVec2::new(-1, 1), IVec2::new(1, -1)),
                6 => (IVec2::new(-1, -1), IVec2::new(-1, 1)),
                _ => (IVec2::new(-1, -1), IVec2::ONE),
            };
            [start, end].map(|offset| tile + (offset * (half_width * 2)).extend(0))
        };
        Some([edge(a), edge(b)])
    }
}

fn compass(direction: u8) -> IVec2 {
    [
        IVec2::NEG_Y,
        IVec2::new(1, -1),
        IVec2::X,
        IVec2::ONE,
        IVec2::Y,
        IVec2::new(-1, 1),
        IVec2::NEG_X,
        IVec2::new(-1, -1),
    ][usize::from(direction)]
}
