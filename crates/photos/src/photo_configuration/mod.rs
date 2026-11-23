use std::io;

use super::photo_album_types::MAX_PHOTO_ALBUM_SPREADS;
use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
};
use openzt2_game_assets::source_document::{
    blue_fang_source_document_parsing::parse_blue_fang_source_document, path::AssetPath,
};

#[derive(Asset, TypePath)]
pub(super) struct PhotoConfiguration {
    pub(super) album_spreads: u32,
    pub(super) maximum_subject_distance_squared: f32,
    pub(super) minimum_visible_fraction: f32,
    pub(super) minimum_screen_fraction: f32,
    pub(super) close_distance_squared: f32,
}

#[derive(Resource)]
pub(super) struct PhotoConfigurationSource(pub(super) Handle<PhotoConfiguration>);

pub(super) fn load_photo_configuration(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(PhotoConfigurationSource(
        server.load("config/photoconfig.xml"),
    ));
}

#[derive(Default, TypePath)]
pub(super) struct PhotoConfigurationLoader;

impl AssetLoader for PhotoConfigurationLoader {
    type Asset = PhotoConfiguration;
    type Settings = ();
    type Error = io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        context: &mut LoadContext<'_>,
    ) -> io::Result<PhotoConfiguration> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        let path = AssetPath::new(context.path().path().to_string_lossy());
        let document = parse_blue_fang_source_document(path, &bytes).map_err(io::Error::other)?;
        let number = |attribute| {
            document
                .root
                .attribute_named_any(&[attribute])
                .and_then(|text| text.parse::<f32>().ok())
                .filter(|number| number.is_finite() && *number >= 0.0)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, attribute))
        };
        let maximum_distance = number("validPhotoDistance")?;
        let close_distance = number("validPhotoDistanceAlwaysValid")?;
        let album_spreads = document
            .root
            .attribute_named_any(&["maxPagesInAlbums"])
            .and_then(|text| text.parse::<u32>().ok())
            .filter(|pages| (1..=MAX_PHOTO_ALBUM_SPREADS).contains(pages))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid maxPagesInAlbums")
            })?;
        Ok(PhotoConfiguration {
            album_spreads,
            maximum_subject_distance_squared: maximum_distance * maximum_distance,
            minimum_visible_fraction: number("validPhotoPercentSeen")? * 0.01,
            minimum_screen_fraction: number("validPhotoPercentOfScreen")? * 0.01,
            close_distance_squared: close_distance * close_distance,
        })
    }
}
