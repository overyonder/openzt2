use super::path::AssetPath;

/// Source syntaxes handled by the live Blue Fang document frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlueFangSourceDocumentFormat {
    Xml,
    DownloadManifest,
    Maxml,
    ZooTycoon2SavedGame,
    Task,
    Behavior,
    Trick,
    BlueFangMaterial,
    BlueFangModelManifest,
    AnimationTextKeys,
    XmlDataReduced,
    OldSourceDocument,
    ParticleSystem,
}

impl BlueFangSourceDocumentFormat {
    pub(crate) fn from_source_document_path(source_document_path: &AssetPath) -> Option<Self> {
        Self::from_source_document_extension(source_document_path.as_str().rsplit_once('.')?.1)
    }

    fn from_source_document_extension(source_document_extension: &str) -> Option<Self> {
        match source_document_extension.to_ascii_lowercase().as_str() {
            "xml" => Some(Self::Xml),
            "dl" => Some(Self::DownloadManifest),
            "maxml" => Some(Self::Maxml),
            "zt2" => Some(Self::ZooTycoon2SavedGame),
            "tsk" => Some(Self::Task),
            "beh" => Some(Self::Behavior),
            "trk" => Some(Self::Trick),
            "bfmat" => Some(Self::BlueFangMaterial),
            "bfm" => Some(Self::BlueFangModelManifest),
            "txtkeys" => Some(Self::AnimationTextKeys),
            "xdr" => Some(Self::XmlDataReduced),
            "old" => Some(Self::OldSourceDocument),
            "psys" => Some(Self::ParticleSystem),
            _ => None,
        }
    }
}
