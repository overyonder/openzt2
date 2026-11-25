use std::io;

use bevy::{
    asset::{io::Reader, AssetLoader, LoadContext},
    ecs::system::SystemParam,
    prelude::*,
};
use openzt2_game_assets::{
    source_document::{
        blue_fang_source_document_parsing::parse_blue_fang_source_document,
        blue_fang_source_numeric_lexeme::parse_blue_fang_source_numeric_lexeme, path::AssetPath,
    },
    terrain::terrain_asset_types_and_borrowing_queries::TerrainAsset,
    texture::source_image_asset_path_selection::select_bevy_image_asset_path_for_blue_fang_source_image,
};
use openzt2_zoo_state::terrain::{
    terrain_chunk_types::{EditedTerrainSamples, TerrainChunk, TerrainIndex},
    terrain_world_sampling::{sample_terrain, terrain_chunk_at},
};

const GROUND_TRACK_SOURCE: &str = "entities/transportation/track/ai/groundtrack.xml";

#[derive(Asset, TypePath)]
pub(super) struct GroundTrackPolicy {
    maximum_across_grade: f32,
    pub(super) collision_radius: f32,
    pub(super) minimum_obstruction_speed_squared: f32,
    paints: Vec<GroundTrackPaintRow>,
}

pub(super) struct GroundTrackPaintSource {
    pub(super) texture: Handle<Image>,
    pub(super) mask: Handle<Image>,
    pub(super) start: Vec2,
    pub(super) size: Vec2,
}

struct GroundTrackPaintRow {
    piece_type: u8,
    appearance: u8,
    flip: u8,
    normal: GroundTrackPaintSource,
    low: Option<GroundTrackPaintSource>,
}

impl GroundTrackPolicy {
    pub(super) fn uses_image(&self, image: bevy::asset::AssetId<Image>) -> bool {
        self.paints.iter().any(|row| {
            std::iter::once(&row.normal)
                .chain(row.low.as_ref())
                .any(|source| source.texture.id() == image || source.mask.id() == image)
        })
    }

    pub(super) fn paint(
        &self,
        piece_type: u8,
        appearance: u8,
        low: bool,
    ) -> Option<(&GroundTrackPaintSource, u8)> {
        let row = self
            .paints
            .iter()
            .find(|row| row.piece_type == piece_type && row.appearance == appearance)?;
        Some((
            if low {
                row.low.as_ref().unwrap_or(&row.normal)
            } else {
                &row.normal
            },
            row.flip,
        ))
    }
}

#[derive(Default, TypePath)]
pub(super) struct GroundTrackPolicyLoader;

impl AssetLoader for GroundTrackPolicyLoader {
    type Asset = GroundTrackPolicy;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<Self::Asset> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let source = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let data = source
            .root
            .element_children()
            .find(|node| node.name.eq_ignore_ascii_case("shared"))
            .and_then(|node| {
                node.element_children()
                    .find(|node| node.name.eq_ignore_ascii_case("BFAIEntityDataShared"))
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "missing ground track shared data",
                )
            })?;
        let read = |name| {
            data.attribute_named_any(&[name])
                .and_then(parse_blue_fang_source_numeric_lexeme::<f32>)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("invalid ground track field {name}"),
                    )
                })
        };
        let mut paints = Vec::new();
        let texture_rows = source
            .root
            .element_children()
            .find(|node| node.name == "shared")
            .and_then(|node| {
                node.element_children()
                    .find(|node| node.name == "ZTTransportGroundTrackShared")
            })
            .and_then(|node| {
                node.element_children()
                    .find(|node| node.name == "textureData")
            });
        if let Some(rows) = texture_rows {
            for node in rows
                .element_children()
                .filter(|node| node.name == "ZTTransportGroundTrackTextureData")
            {
                let index = |key| {
                    node.attribute(key)
                        .and_then(parse_blue_fang_source_numeric_lexeme::<u8>)
                };
                let pair = |key| {
                    let mut values = node.attribute(key)?.split_ascii_whitespace();
                    Some(Vec2::new(
                        parse_blue_fang_source_numeric_lexeme::<f32>(values.next()?)?,
                        parse_blue_fang_source_numeric_lexeme::<f32>(values.next()?)?,
                    ))
                };
                let mut recipe = |texture, mask, start, size| -> Option<GroundTrackPaintSource> {
                    Some(GroundTrackPaintSource {
                        texture: context.load(
                            select_bevy_image_asset_path_for_blue_fang_source_image(
                                node.attribute(texture)?,
                            ),
                        ),
                        mask: context.load(
                            select_bevy_image_asset_path_for_blue_fang_source_image(
                                node.attribute(mask)?,
                            ),
                        ),
                        start: pair(start)?,
                        size: pair(size)?,
                    })
                };
                let Some((piece_type, appearance)) = index("pieceType").zip(index("appearance"))
                else {
                    continue;
                };
                let Some(normal) = recipe(
                    "paintTexture",
                    "paintMask",
                    "paintSampleStart",
                    "paintCopyDimension",
                ) else {
                    continue;
                };
                let flip = index("maskFlip")
                    .unwrap_or_else(|| u8::from(node.attribute("maskFlip") == Some("true")));
                let low = recipe(
                    "lowResTexture",
                    "lowResMask",
                    "lowResSampleStart",
                    "lowResCopyDimension",
                );
                paints.push(GroundTrackPaintRow {
                    piece_type,
                    appearance,
                    flip,
                    normal,
                    low,
                });
            }
        }
        Ok(GroundTrackPolicy {
            paints,
            maximum_across_grade: read("f_MaximumAcrossSlope")?,
            collision_radius: read("f_CollisionDetectionRadius")?,
            minimum_obstruction_speed_squared: read("f_MinVelocityForObstruction")?,
        })
    }

    fn extensions(&self) -> &[&str] {
        &["xml"]
    }
}

#[derive(Resource)]
pub(super) struct GroundTrackPolicySource(Handle<GroundTrackPolicy>);

pub(super) fn load_ground_track_policy(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(GroundTrackPolicySource(assets.load(GROUND_TRACK_SOURCE)));
}

#[derive(SystemParam)]
pub(super) struct GroundTrackPolicyAsset<'w> {
    source: Option<Res<'w, GroundTrackPolicySource>>,
    assets: Res<'w, Assets<GroundTrackPolicy>>,
}

impl GroundTrackPolicyAsset<'_> {
    pub(super) fn is_changed(&self) -> bool {
        self.assets.is_changed()
            || self
                .source
                .as_ref()
                .is_some_and(|source| source.is_changed())
    }

    pub(super) fn get(&self) -> Option<&GroundTrackPolicy> {
        self.assets.get(&self.source.as_ref()?.0)
    }
}

#[derive(SystemParam)]
pub(super) struct TrackTerrain<'w, 's> {
    indexes: Query<'w, 's, &'static TerrainIndex>,
    assets: Option<Res<'w, Assets<TerrainAsset>>>,
    chunks: Query<'w, 's, (&'static TerrainChunk, Option<&'static EditedTerrainSamples>)>,
}

impl TrackTerrain<'_, '_> {
    pub(super) fn height_at(&self, root: Entity, point: Vec2) -> Option<f32> {
        let index = self.indexes.get(root).ok()?;
        let (chunk, edited) = self.chunks.get(terrain_chunk_at(index, point)?).ok()?;
        Some(
            sample_terrain(
                chunk,
                self.assets.as_ref()?.get(&chunk.asset)?,
                edited,
                point,
            )?
            .height_m,
        )
    }

    pub(super) fn across_grade_allowed(
        &self,
        root: Entity,
        path: &[Vec3],
        station_ends: [bool; 2],
        policy: &GroundTrackPolicy,
    ) -> bool {
        path.windows(2).enumerate().all(|(index, points)| {
            if (index == 0 && station_ends[0]) || (index + 2 == path.len() && station_ends[1]) {
                return true;
            }
            let horizontal = (points[1] - points[0]).xz();
            let scale = horizontal.abs().max_element();
            if !scale.is_finite() || scale <= f32::EPSILON {
                return false;
            }
            let offset = Vec2::new(-horizontal.y, horizontal.x) / scale;
            let centre = points[0].xz().midpoint(points[1].xz());
            let Some(left) = self.height_at(root, centre - offset) else {
                return false;
            };
            let Some(right) = self.height_at(root, centre + offset) else {
                return false;
            };
            (right - left).abs() <= policy.maximum_across_grade * 2.0 * offset.length()
        })
    }
}
