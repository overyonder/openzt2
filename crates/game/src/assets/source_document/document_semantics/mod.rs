//! Classifies source documents by the kind of asset they define.

use crate::assets::source_document::{
    blue_fang_source_document_format::BlueFangSourceDocumentFormat,
    ordered_source_document_types::OrderedSourceDocument,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceDocumentKind {
    Ui,
    Localization,
    Behavior,
    Animation,
    SpeciesOrEntity,
    Scenario,
    Audio,
    Terrain,
    WorldDefinitions,
    Renderer,
    Manager,
    Catalogue,
    ToolingSchema,
    Unknown,
}

pub(crate) fn classify_source_document(document: &OrderedSourceDocument) -> SourceDocumentKind {
    let root = document.root.name.to_ascii_lowercase();
    let path = document.path.key();
    if document.format == BlueFangSourceDocumentFormat::XmlDataReduced
        || path.starts_with("schema/")
    {
        return SourceDocumentKind::ToolingSchema;
    }
    if document.format == BlueFangSourceDocumentFormat::AnimationTextKeys {
        return SourceDocumentKind::Animation;
    }
    // UI-path ownership is normally decisive, but the tank policy lives under
    // ui/modes while its root attributes describe rates consumed by the
    // world-definition loader. The ordinary terrain deformation document
    // shares this root type and remains UI-owned.
    if root == "terrdeformationui" && document.root.attribute("tankFloorSpeed").is_some() {
        return SourceDocumentKind::WorldDefinitions;
    }
    // BFHelpEntry documents are authored application content which happens to
    // live beneath UI/.  The original help component consumes and merges this
    // hierarchy; it is not a renderable UI document.
    if contains_element(document, "BFHelpEntry") {
        return SourceDocumentKind::WorldDefinitions;
    }
    if root == "ui"
        || root == "ztapp"
        || path.starts_with("ui/")
        || matches!(
            root.as_str(),
            "uilayout" | "uilistbox" | "imeuiconfig" | "zttrickbuttonmgr" | "zttourdatafeedback"
        )
    {
        return SourceDocumentKind::Ui;
    }
    if contains_element(document, "LOC_STRING") || path.starts_with("lang/") {
        return SourceDocumentKind::Localization;
    }
    if matches!(
        document.format,
        BlueFangSourceDocumentFormat::Task | BlueFangSourceDocumentFormat::Behavior
    ) || matches!(
        root.as_str(),
        "bfaitasktemplatelist"
            | "behaviorsets"
            | "ztgesturemgr"
            | "ztpuzzlemgr"
            | "gestures"
            | "tricks"
    ) {
        return SourceDocumentKind::Behavior;
    }
    if root == "ztpuzzle" {
        return SourceDocumentKind::WorldDefinitions;
    }
    if path.starts_with("scenario/")
        || path.starts_with("campaigns/")
        || path.starts_with("maps/")
        || path.starts_with("locations/")
        || path.starts_with("photochall/")
        || matches!(
            root.as_str(),
            "bfcampaign"
                | "campaign"
                | "scenario"
                | "scenarios"
                | "maps"
                | "saveroot"
                | "challenges"
        )
    {
        return SourceDocumentKind::Scenario;
    }
    if path.starts_with("sounds/")
        || path.starts_with("world/sound")
        || contains_element(document, "soundtags")
        || contains_element(document, "BFSoundtag")
        || matches!(
            root.as_str(),
            "bfsoundmgr"
                | "bfsoundstagemgr"
                | "uisoundmgr"
                | "soundtags"
                | "ambientallowed"
                | "ambientsallowed"
                | "ambientbiomes"
                | "ambientsounds"
                | "ambient"
                | "ztaiambientsmgr"
                | "ztworldsndmgr"
        )
    {
        return SourceDocumentKind::Audio;
    }
    if contains_element(document, "BFParticleDictionary")
        || matches!(
            root.as_str(),
            "bfdx9renderer" | "bfparticledictionary" | "bfwaterfall"
        )
    {
        return SourceDocumentKind::Renderer;
    }
    if path.starts_with("entities/")
        || matches!(
            root.as_str(),
            "bftypedbinder" | "bfphysobj" | "bfgbiomeobjects" | "biomeoptions"
        )
    {
        return SourceDocumentKind::SpeciesOrEntity;
    }
    if path.starts_with("world/terrain") || root == "bfterrain" {
        return SourceDocumentKind::Terrain;
    }
    if matches!(
        root.as_str(),
        "bfworldenvironment"
            | "bfgbiome"
            | "fog"
            | "lights"
            | "loddistances"
            | "skylayers"
            | "suspense"
            | "water"
            | "crowd"
    ) {
        return SourceDocumentKind::WorldDefinitions;
    }
    if matches!(
        root.as_str(),
        "bfcontext"
            | "ztdevconsole"
            | "bfuseroptions"
            | "bfluaconfig"
            | "bfuserprofilemgr"
            | "bftimekeeper"
            | "bflocalemgr"
            | "ztmainmode"
            | "hotkeys"
            | "bfscriptcontextmgr"
            | "ztadoptionmgr"
            | "ztadoptionslotmgr"
            | "ztexpansioninfo"
            | "ztfavoriteanimalmgr"
            | "ztdiseasemgr"
            | "bfgentitytriggermgr"
            | "ztfeedbackmgr"
            | "ztaiguestmgr"
            | "bfginfluencemgr"
            | "bfgpathfindermgr"
            | "bfgrampagemgr"
            | "ztworldmgr"
            | "bfgtraversabilitymgr"
            | "ztlocationmgr"
            | "ztaistaffmgr"
            | "ztaistaffwatercleaningmgr"
            | "watercleaning"
            | "ztuserstaffaction"
            | "zttankmgr"
            | "zttankmanipulationui"
            | "tankmanipulation"
            | "ztphotomanager"
            | "ztphotochallengemgr"
            | "ztaimgr"
            | "ztaishowmgr"
            | "ztshowmixermanager"
            | "ztshowtimermanager"
            | "zttourdata"
            | "zteconomymgr"
            | "ztstatus"
            | "bfgmanager"
            | "startup"
            | "timeofdayupdateinterval"
            | "zttourdataattr"
            | "zttourrecorderdata"
    ) {
        return SourceDocumentKind::Manager;
    }
    if root == "ztmovielist" {
        return SourceDocumentKind::ToolingSchema;
    }
    if root == "binder" || path.starts_with("xpinfo/") {
        return SourceDocumentKind::Catalogue;
    }
    SourceDocumentKind::Unknown
}

fn contains_element(document: &OrderedSourceDocument, wanted: &str) -> bool {
    fn descendants(
        node: &crate::assets::source_document::ordered_source_document_types::OrderedSourceDocumentNode,
        wanted: &str,
    ) -> bool {
        node.name.eq_ignore_ascii_case(wanted)
            || node
                .element_children()
                .any(|child| descendants(child, wanted))
    }
    document
        .root
        .element_children()
        .any(|node| descendants(node, wanted))
}
