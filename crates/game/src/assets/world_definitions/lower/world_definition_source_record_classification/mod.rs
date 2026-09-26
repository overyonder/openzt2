//! Closed source root and record-type classification for world-definition lowering.

use crate::assets::source_document::resolved_source_record_index::RecordView;
use crate::assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;

#[derive(Clone, Copy)]
pub(super) enum WorldDefinitionSourceRecordKind {
    Container,
    Object,
    Placeable,
    Facility,
    Maintenance,
    Cleanliness,
    Staff,
    StaffJob,
    Guest,
    Fence,
    Path,
    Biome,
    Brush,
    Disease,
    Treatment,
    Tranquilizer,
    Rampage,
    Catalogue,
    Zoopedia,
    Research,
    Unlock,
    Rating,
    Fame,
    Award,
    Tank,
    Aquatic,
    ShowStage,
    Trick,
    ShowRule,
    Station,
    Track,
    Vehicle,
    TourView,
    FossilSet,
    FossilPiece,
    FossilSlot,
    FossilPuzzle,
    ImmersiveModePolicy,
    Camera,
    Environment,
    Weather,
    Ambient,
    Timing,
    GuestPolicy,
    TankPolicy,
    TankEdit,
    ShowPolicy,
    ShowTimer,
    ShowMixer,
    TourValues,
    TourPolicy,
    TourRatings,
    StaffWaterCleaning,
    EnvironmentFog,
    EnvironmentLights,
    PersonNames,
    Location,
}

pub(super) fn classify_world_definition_source_record(
    value: &str,
) -> Option<WorldDefinitionSourceRecordKind> {
    let value = canonicalize_source_document_record_key(value);
    [
        ("bftypedbinder", WorldDefinitionSourceRecordKind::Container),
        ("bfregistry", WorldDefinitionSourceRecordKind::Container),
        ("bfgmanager", WorldDefinitionSourceRecordKind::Container),
        ("startup", WorldDefinitionSourceRecordKind::Container),
        (
            "timeofdayupdateinterval",
            WorldDefinitionSourceRecordKind::Container,
        ),
        // Source envelopes whose payload is owned by specialized placement or
        // dependency frontends, not by the live world-definition catalogue.
        (
            "bfgbiomeobjects",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("biomeoptions", WorldDefinitionSourceRecordKind::Container),
        ("bfterrain", WorldDefinitionSourceRecordKind::Container),
        ("graph", WorldDefinitionSourceRecordKind::Container),
        // A BFPhysObj document is a component/prefab envelope, not an authored
        // catalogue object definition.  Its children are owned by the scene
        // and physics frontends; treating the envelope as a catalogue object would
        // invent a required object kind for component-only sources such as
        // rope fixtures.
        ("bfphysobj", WorldDefinitionSourceRecordKind::Container),
        ("ztaistaffmgr", WorldDefinitionSourceRecordKind::Container),
        (
            "ztaistaffwatercleaningmgr",
            WorldDefinitionSourceRecordKind::StaffWaterCleaning,
        ),
        (
            "watercleaning",
            WorldDefinitionSourceRecordKind::StaffWaterCleaning,
        ),
        (
            "ztuserstaffaction",
            WorldDefinitionSourceRecordKind::StaffWaterCleaning,
        ),
        ("ztaiguestmgr", WorldDefinitionSourceRecordKind::GuestPolicy),
        ("zttankmgr", WorldDefinitionSourceRecordKind::TankPolicy),
        ("ztaishowmgr", WorldDefinitionSourceRecordKind::ShowPolicy),
        (
            "ztshowtimermanager",
            WorldDefinitionSourceRecordKind::ShowTimer,
        ),
        (
            "ztshowmixermanager",
            WorldDefinitionSourceRecordKind::ShowMixer,
        ),
        ("zttourdata", WorldDefinitionSourceRecordKind::TourValues),
        (
            "zttourdataattr",
            WorldDefinitionSourceRecordKind::TourPolicy,
        ),
        (
            "zttourrecorderdata",
            WorldDefinitionSourceRecordKind::TourRatings,
        ),
        ("ztdiseasemgr", WorldDefinitionSourceRecordKind::Container),
        ("bfgrampagemgr", WorldDefinitionSourceRecordKind::Container),
        (
            "cataloguemanager",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("catalogmanager", WorldDefinitionSourceRecordKind::Container),
        ("binder", WorldDefinitionSourceRecordKind::Container),
        (
            "ztexpansioninfo",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztapp", WorldDefinitionSourceRecordKind::Container),
        ("bfcontext", WorldDefinitionSourceRecordKind::Container),
        ("ztdevconsole", WorldDefinitionSourceRecordKind::Container),
        ("bfuseroptions", WorldDefinitionSourceRecordKind::Container),
        ("bfluaconfig", WorldDefinitionSourceRecordKind::Container),
        (
            "bfuserprofilemgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("bflocalemgr", WorldDefinitionSourceRecordKind::Container),
        ("ztmainmode", WorldDefinitionSourceRecordKind::Container),
        (
            "ztphotomode",
            WorldDefinitionSourceRecordKind::ImmersiveModePolicy,
        ),
        (
            "ztsuperstaffmode",
            WorldDefinitionSourceRecordKind::ImmersiveModePolicy,
        ),
        ("hotkeys", WorldDefinitionSourceRecordKind::Container),
        (
            "bfscriptcontextmgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztadoptionmgr", WorldDefinitionSourceRecordKind::Container),
        (
            "ztadoptionslotmgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        (
            "ztfavoriteanimalmgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        (
            "bfgentitytriggermgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztfeedbackmgr", WorldDefinitionSourceRecordKind::Container),
        (
            "bfginfluencemgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        (
            "bfgpathfindermgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztworldmgr", WorldDefinitionSourceRecordKind::Container),
        (
            "bfgtraversabilitymgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztlocationmgr", WorldDefinitionSourceRecordKind::Container),
        ("ztlocationentry", WorldDefinitionSourceRecordKind::Location),
        ("ztpuzzlemgr", WorldDefinitionSourceRecordKind::Container),
        ("tricks", WorldDefinitionSourceRecordKind::Container),
        (
            "zttankmanipulationui",
            WorldDefinitionSourceRecordKind::Container,
        ),
        (
            "tankmanipulation",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztphotomanager", WorldDefinitionSourceRecordKind::Container),
        (
            "ztphotochallengemgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        ("ztaimgr", WorldDefinitionSourceRecordKind::Container),
        ("zteconomymgr", WorldDefinitionSourceRecordKind::Container),
        ("ztstatus", WorldDefinitionSourceRecordKind::Container),
        (
            "cleanlinesspolicy",
            WorldDefinitionSourceRecordKind::Cleanliness,
        ),
        (
            "maintenancedefinition",
            WorldDefinitionSourceRecordKind::Maintenance,
        ),
        ("maintenance", WorldDefinitionSourceRecordKind::Maintenance),
        (
            "staffjobdefinition",
            WorldDefinitionSourceRecordKind::StaffJob,
        ),
        ("staffjob", WorldDefinitionSourceRecordKind::StaffJob),
        ("staffdefinition", WorldDefinitionSourceRecordKind::Staff),
        ("staff", WorldDefinitionSourceRecordKind::Staff),
        ("guestdefinition", WorldDefinitionSourceRecordKind::Guest),
        ("guest", WorldDefinitionSourceRecordKind::Guest),
        (
            "facilitydefinition",
            WorldDefinitionSourceRecordKind::Facility,
        ),
        ("facility", WorldDefinitionSourceRecordKind::Facility),
        (
            "placeabledefinition",
            WorldDefinitionSourceRecordKind::Placeable,
        ),
        ("placeable", WorldDefinitionSourceRecordKind::Placeable),
        ("fencedefinition", WorldDefinitionSourceRecordKind::Fence),
        ("fence", WorldDefinitionSourceRecordKind::Fence),
        ("pathdefinition", WorldDefinitionSourceRecordKind::Path),
        ("path", WorldDefinitionSourceRecordKind::Path),
        ("biomedefinition", WorldDefinitionSourceRecordKind::Biome),
        ("bfgbiome", WorldDefinitionSourceRecordKind::Biome),
        ("brushdefinition", WorldDefinitionSourceRecordKind::Brush),
        ("brush", WorldDefinitionSourceRecordKind::Brush),
        (
            "diseasedefinition",
            WorldDefinitionSourceRecordKind::Disease,
        ),
        ("disease", WorldDefinitionSourceRecordKind::Disease),
        (
            "treatmentdefinition",
            WorldDefinitionSourceRecordKind::Treatment,
        ),
        ("treatment", WorldDefinitionSourceRecordKind::Treatment),
        (
            "tranquilizerdefinition",
            WorldDefinitionSourceRecordKind::Tranquilizer,
        ),
        (
            "tranquilizer",
            WorldDefinitionSourceRecordKind::Tranquilizer,
        ),
        ("rampagerule", WorldDefinitionSourceRecordKind::Rampage),
        ("catalogueentry", WorldDefinitionSourceRecordKind::Catalogue),
        ("catalogentry", WorldDefinitionSourceRecordKind::Catalogue),
        ("zoopediaentry", WorldDefinitionSourceRecordKind::Zoopedia),
        (
            "researchdefinition",
            WorldDefinitionSourceRecordKind::Research,
        ),
        ("research", WorldDefinitionSourceRecordKind::Research),
        ("unlockdefinition", WorldDefinitionSourceRecordKind::Unlock),
        ("unlock", WorldDefinitionSourceRecordKind::Unlock),
        ("ratingdefinition", WorldDefinitionSourceRecordKind::Rating),
        ("rating", WorldDefinitionSourceRecordKind::Rating),
        ("famethreshold", WorldDefinitionSourceRecordKind::Fame),
        ("fame", WorldDefinitionSourceRecordKind::Fame),
        ("awarddefinition", WorldDefinitionSourceRecordKind::Award),
        ("award", WorldDefinitionSourceRecordKind::Award),
        ("tankdefinition", WorldDefinitionSourceRecordKind::Tank),
        ("tank", WorldDefinitionSourceRecordKind::Tank),
        (
            "aquaticrequirement",
            WorldDefinitionSourceRecordKind::Aquatic,
        ),
        ("aquatic", WorldDefinitionSourceRecordKind::Aquatic),
        (
            "showstagedefinition",
            WorldDefinitionSourceRecordKind::ShowStage,
        ),
        ("showstage", WorldDefinitionSourceRecordKind::ShowStage),
        ("trickdefinition", WorldDefinitionSourceRecordKind::Trick),
        ("trick", WorldDefinitionSourceRecordKind::Trick),
        (
            "showruledefinition",
            WorldDefinitionSourceRecordKind::ShowRule,
        ),
        ("showrule", WorldDefinitionSourceRecordKind::ShowRule),
        (
            "stationdefinition",
            WorldDefinitionSourceRecordKind::Station,
        ),
        ("station", WorldDefinitionSourceRecordKind::Station),
        ("trackdefinition", WorldDefinitionSourceRecordKind::Track),
        ("track", WorldDefinitionSourceRecordKind::Track),
        (
            "vehicledefinition",
            WorldDefinitionSourceRecordKind::Vehicle,
        ),
        ("vehicle", WorldDefinitionSourceRecordKind::Vehicle),
        (
            "tourviewdefinition",
            WorldDefinitionSourceRecordKind::TourView,
        ),
        ("tourview", WorldDefinitionSourceRecordKind::TourView),
        (
            "fossilsetdefinition",
            WorldDefinitionSourceRecordKind::FossilSet,
        ),
        ("fossilset", WorldDefinitionSourceRecordKind::FossilSet),
        (
            "fossilpiecedefinition",
            WorldDefinitionSourceRecordKind::FossilPiece,
        ),
        ("fossilpiece", WorldDefinitionSourceRecordKind::FossilPiece),
        (
            "fossilslotdefinition",
            WorldDefinitionSourceRecordKind::FossilSlot,
        ),
        ("fossilslot", WorldDefinitionSourceRecordKind::FossilSlot),
        ("ztpuzzle", WorldDefinitionSourceRecordKind::FossilPuzzle),
        (
            "modetooldefinition",
            WorldDefinitionSourceRecordKind::ImmersiveModePolicy,
        ),
        (
            "modetool",
            WorldDefinitionSourceRecordKind::ImmersiveModePolicy,
        ),
        (
            "cameratuningdefinition",
            WorldDefinitionSourceRecordKind::Camera,
        ),
        ("camera", WorldDefinitionSourceRecordKind::Camera),
        (
            "environmentdefinition",
            WorldDefinitionSourceRecordKind::Environment,
        ),
        ("environment", WorldDefinitionSourceRecordKind::Environment),
        (
            "bfworldenvironment",
            WorldDefinitionSourceRecordKind::Environment,
        ),
        ("fog", WorldDefinitionSourceRecordKind::EnvironmentFog),
        ("lights", WorldDefinitionSourceRecordKind::EnvironmentLights),
        ("loddistances", WorldDefinitionSourceRecordKind::Container),
        ("skylayers", WorldDefinitionSourceRecordKind::Container),
        ("suspense", WorldDefinitionSourceRecordKind::Container),
        ("water", WorldDefinitionSourceRecordKind::Container),
        ("crowd", WorldDefinitionSourceRecordKind::Container),
        (
            "weatherdefinition",
            WorldDefinitionSourceRecordKind::Weather,
        ),
        ("weather", WorldDefinitionSourceRecordKind::Weather),
        (
            "ambientspawndefinition",
            WorldDefinitionSourceRecordKind::Ambient,
        ),
        ("ambientspawn", WorldDefinitionSourceRecordKind::Ambient),
        (
            "ztaiambientsmgr",
            WorldDefinitionSourceRecordKind::Container,
        ),
        (
            "simulationtimingdefinition",
            WorldDefinitionSourceRecordKind::Timing,
        ),
        ("bftimekeeper", WorldDefinitionSourceRecordKind::Timing),
        ("entitydefinition", WorldDefinitionSourceRecordKind::Object),
        ("objectdefinition", WorldDefinitionSourceRecordKind::Object),
        ("object", WorldDefinitionSourceRecordKind::Object),
    ]
    .into_iter()
    .find_map(|(name, kind)| (value == name).then_some(kind))
}

pub(super) fn is_semantic_world_definition_source_record(record: &RecordView<'_, '_>) -> bool {
    record.is_document_root_record()
        || record
            .source_document_element()
            .attribute_named_any(&["definitionType", "class", "typeName"])
            .is_some()
}
