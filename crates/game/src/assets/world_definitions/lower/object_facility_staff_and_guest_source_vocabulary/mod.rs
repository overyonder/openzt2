use super::world_definition_source_value_reading_and_conversion::{enum_value, required};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::facilities_and_maintenance::FacilityServiceKind;
use openzt2_game_data::world_definitions::guest_simulation_definitions::{
    GuestMemoryKind, GuestNeedKind, GuestRarity, GuestVisitPurpose, MemoryReplacement,
};
use openzt2_game_data::world_definitions::object_placement::EntrancePurpose;
use openzt2_game_data::world_definitions::staff_management::{
    StaffJobCapabilityFlags, StaffJobKind, StaffRoleKind,
};
use openzt2_game_data::world_definitions::world_objects::{
    WorldObjectInformationViewClass, WorldObjectKind,
};

pub(super) fn world_object_kind(v: &str) -> Result<WorldObjectKind, BindError> {
    enum_value(
        v,
        &[
            ("scenery", WorldObjectKind::Scenery),
            ("shelter", WorldObjectKind::Shelter),
            ("food", WorldObjectKind::Food),
            ("water", WorldObjectKind::Water),
            ("enrichment", WorldObjectKind::Enrichment),
            ("donationbox", WorldObjectKind::DonationBox),
            ("bin", WorldObjectKind::Bin),
            ("bench", WorldObjectKind::Bench),
            ("facility", WorldObjectKind::Facility),
            ("fence", WorldObjectKind::Fence),
            ("gate", WorldObjectKind::Gate),
            ("path", WorldObjectKind::Path),
            ("staff", WorldObjectKind::Staff),
            ("guest", WorldObjectKind::Guest),
            ("animal", WorldObjectKind::Animal),
            ("tank", WorldObjectKind::Tank),
            ("showstage", WorldObjectKind::ShowStage),
            ("station", WorldObjectKind::Station),
            ("vehicle", WorldObjectKind::Vehicle),
            ("laboratory", WorldObjectKind::Laboratory),
        ],
        "object kind",
    )
}
pub(super) fn world_object_information_view_class(
    record: &RecordView<'_, '_>,
) -> Result<Option<WorldObjectInformationViewClass>, BindError> {
    if let Some(value) = record.value(&["informationViewClass", "viewClass"]) {
        return enum_value(
            value,
            &[
                ("building", WorldObjectInformationViewClass::Building),
                ("buildings", WorldObjectInformationViewClass::Building),
                ("entrance", WorldObjectInformationViewClass::Entrance),
                ("entrances", WorldObjectInformationViewClass::Entrance),
                ("fence", WorldObjectInformationViewClass::Fence),
                ("fences", WorldObjectInformationViewClass::Fence),
                ("curb", WorldObjectInformationViewClass::Curb),
                ("curbs", WorldObjectInformationViewClass::Curb),
                ("zoowall", WorldObjectInformationViewClass::ZooWall),
                ("zoowalls", WorldObjectInformationViewClass::ZooWall),
                ("foliage", WorldObjectInformationViewClass::Foliage),
            ],
            "information view class",
        )
        .map(Some);
    }

    [
        ("entrance", WorldObjectInformationViewClass::Entrance),
        ("curb", WorldObjectInformationViewClass::Curb),
        ("zoowall", WorldObjectInformationViewClass::ZooWall),
        ("foliage", WorldObjectInformationViewClass::Foliage),
        ("fence", WorldObjectInformationViewClass::Fence),
        ("building", WorldObjectInformationViewClass::Building),
    ]
    .into_iter()
    .find_map(|(token, class)| record.has_type_token(token).then_some(class))
    .map_or_else(
        || {
            world_object_kind(required(record, &["kind", "objectType", "category"])?).map(|kind| {
                match kind {
                    WorldObjectKind::Fence | WorldObjectKind::Gate => {
                        Some(WorldObjectInformationViewClass::Fence)
                    }
                    WorldObjectKind::Facility
                    | WorldObjectKind::Shelter
                    | WorldObjectKind::DonationBox
                    | WorldObjectKind::Bin
                    | WorldObjectKind::Bench
                    | WorldObjectKind::ShowStage
                    | WorldObjectKind::Station
                    | WorldObjectKind::Laboratory => {
                        Some(WorldObjectInformationViewClass::Building)
                    }
                    _ => None,
                }
            })
        },
        |class| Ok(Some(class)),
    )
}
pub(super) fn service_kind(v: &str) -> Result<FacilityServiceKind, BindError> {
    enum_value(
        v,
        &[
            ("food", FacilityServiceKind::Food),
            ("drink", FacilityServiceKind::Drink),
            ("toilet", FacilityServiceKind::Toilet),
            ("gift", FacilityServiceKind::Gift),
            ("education", FacilityServiceKind::Education),
            ("adoption", FacilityServiceKind::Adoption),
            ("transport", FacilityServiceKind::Transport),
            ("maintenance", FacilityServiceKind::Maintenance),
            ("show", FacilityServiceKind::Show),
            ("laboratory", FacilityServiceKind::Laboratory),
        ],
        "service kind",
    )
}
pub(super) fn staff_role_kind(v: &str) -> Result<StaffRoleKind, BindError> {
    enum_value(
        v,
        &[
            ("none", StaffRoleKind::None),
            ("keeper", StaffRoleKind::Keeper),
            ("maintenance", StaffRoleKind::Maintenance),
            ("educator", StaffRoleKind::Educator),
            ("veterinarian", StaffRoleKind::Veterinarian),
            ("entertainer", StaffRoleKind::Entertainer),
            ("trainer", StaffRoleKind::Trainer),
            ("presenter", StaffRoleKind::Presenter),
            ("paleontologist", StaffRoleKind::Paleontologist),
            ("recovery", StaffRoleKind::Recovery),
        ],
        "staff kind",
    )
}
pub(super) fn staff_job_kind(v: &str) -> Result<StaffJobKind, BindError> {
    enum_value(
        v,
        &[
            ("feed", StaffJobKind::Feed),
            ("refillwater", StaffJobKind::RefillWater),
            ("cleanhabitat", StaffJobKind::CleanHabitat),
            ("emptybin", StaffJobKind::EmptyBin),
            ("sweeplitter", StaffJobKind::SweepLitter),
            ("repair", StaffJobKind::Repair),
            ("treat", StaffJobKind::Treat),
            ("educate", StaffJobKind::Educate),
            ("entertain", StaffJobKind::Entertain),
            ("tranquilize", StaffJobKind::Tranquilize),
            ("capture", StaffJobKind::Capture),
            ("maintaintank", StaffJobKind::MaintainTank),
            ("operateshow", StaffJobKind::OperateShow),
        ],
        "staff job",
    )
}
pub(super) fn staff_job_bit(v: StaffJobKind) -> u32 {
    StaffJobCapabilityFlags::for_job_kind(v).raw_flag_bits()
}
pub(super) fn guest_need(v: &str) -> Result<GuestNeedKind, BindError> {
    enum_value(
        v,
        &[
            ("hunger", GuestNeedKind::Hunger),
            ("thirst", GuestNeedKind::Thirst),
            ("dessert", GuestNeedKind::Dessert),
            ("gift", GuestNeedKind::Gift),
            ("rest", GuestNeedKind::Energy),
            ("energy", GuestNeedKind::Energy),
            ("bathroom", GuestNeedKind::Restroom),
            ("restroom", GuestNeedKind::Restroom),
            ("social", GuestNeedKind::Social),
        ],
        "guest need",
    )
}
pub(super) fn guest_rarity(v: &str) -> Result<GuestRarity, BindError> {
    enum_value(
        v,
        &[
            ("common", GuestRarity::Common),
            ("uncommon", GuestRarity::Uncommon),
            ("rare", GuestRarity::Rare),
        ],
        "guest rarity",
    )
}
pub(super) fn guest_purpose(v: &str) -> Result<GuestVisitPurpose, BindError> {
    enum_value(
        v,
        &[
            ("view", GuestVisitPurpose::View),
            ("food", GuestVisitPurpose::Food),
            ("drink", GuestVisitPurpose::Drink),
            ("restroom", GuestVisitPurpose::Restroom),
            ("rest", GuestVisitPurpose::Rest),
            ("education", GuestVisitPurpose::Education),
            ("shop", GuestVisitPurpose::Shop),
            ("exit", GuestVisitPurpose::Exit),
            ("show", GuestVisitPurpose::Show),
            ("tour", GuestVisitPurpose::Tour),
        ],
        "guest purpose",
    )
}
pub(super) fn guest_memory(v: &str) -> Result<GuestMemoryKind, BindError> {
    enum_value(
        v,
        &[
            ("animalview", GuestMemoryKind::AnimalView),
            ("facility", GuestMemoryKind::Facility),
            ("education", GuestMemoryKind::Education),
            ("crowding", GuestMemoryKind::Crowding),
            ("litter", GuestMemoryKind::Litter),
            ("scenery", GuestMemoryKind::Scenery),
            ("danger", GuestMemoryKind::Danger),
            ("show", GuestMemoryKind::Show),
            ("tour", GuestMemoryKind::Tour),
        ],
        "guest memory",
    )
}
pub(super) fn memory_replacement(v: &str) -> Result<MemoryReplacement, BindError> {
    enum_value(
        v,
        &[
            ("oldest", MemoryReplacement::Oldest),
            (
                "lowestabsolutevalue",
                MemoryReplacement::LowestAbsoluteValue,
            ),
        ],
        "memory replacement",
    )
}
pub(super) fn entrance_purpose(v: &str) -> Result<EntrancePurpose, BindError> {
    enum_value(
        v,
        &[
            ("guest", EntrancePurpose::Guest),
            ("staff", EntrancePurpose::Staff),
            ("service", EntrancePurpose::Service),
            ("vehicle", EntrancePurpose::Vehicle),
            ("animal", EntrancePurpose::Animal),
        ],
        "entrance purpose",
    )
}

pub(super) fn authored_world_object_kind(record: &RecordView<'_, '_>) -> WorldObjectKind {
    [
        ("entrance", WorldObjectKind::Gate),
        ("fence", WorldObjectKind::Fence),
        ("path", WorldObjectKind::Path),
        ("shelter", WorldObjectKind::Shelter),
        ("food", WorldObjectKind::Food),
        ("water", WorldObjectKind::Water),
        ("enrichment", WorldObjectKind::Enrichment),
        ("donationbox", WorldObjectKind::DonationBox),
        ("bin", WorldObjectKind::Bin),
        ("bench", WorldObjectKind::Bench),
        ("facility", WorldObjectKind::Facility),
        ("tank", WorldObjectKind::Tank),
        ("showstage", WorldObjectKind::ShowStage),
        ("station", WorldObjectKind::Station),
        ("vehicle", WorldObjectKind::Vehicle),
        ("laboratory", WorldObjectKind::Laboratory),
    ]
    .into_iter()
    .find_map(|(token, kind)| record.has_type_token(token).then_some(kind))
    .unwrap_or(WorldObjectKind::Scenery)
}
pub(super) fn authored_world_object_information_view_class(
    record: &RecordView<'_, '_>,
    object_kind: WorldObjectKind,
) -> Option<WorldObjectInformationViewClass> {
    if record.has_type_token("entrance") {
        Some(WorldObjectInformationViewClass::Entrance)
    } else if record.has_type_token("fence") {
        Some(WorldObjectInformationViewClass::Fence)
    } else if record.has_type_token("foliage") {
        Some(WorldObjectInformationViewClass::Foliage)
    } else if matches!(
        object_kind,
        WorldObjectKind::Facility
            | WorldObjectKind::Shelter
            | WorldObjectKind::DonationBox
            | WorldObjectKind::Bin
            | WorldObjectKind::Bench
            | WorldObjectKind::ShowStage
            | WorldObjectKind::Station
            | WorldObjectKind::Laboratory
    ) {
        Some(WorldObjectInformationViewClass::Building)
    } else {
        None
    }
}
