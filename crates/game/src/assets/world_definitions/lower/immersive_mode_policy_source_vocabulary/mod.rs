use super::world_definition_source_value_reading_and_conversion::enum_value;
use crate::assets::source_document::resolved_source_record_index::BindError;
use openzt2_game_data::world_definitions::immersive_mode_policy::{
    ImmersiveModeActionFlags, ImmersiveModeKind,
};

pub(super) fn immersive_mode_action(value: &str) -> Option<u64> {
    Some(match value {
        "moveforward" => u64::from(ImmersiveModeActionFlags::MOVE_FORWARD.raw_flag_bits()),
        "moveback" => u64::from(ImmersiveModeActionFlags::MOVE_BACK.raw_flag_bits()),
        "strafeleft" => u64::from(ImmersiveModeActionFlags::STRAFE_LEFT.raw_flag_bits()),
        "straferight" => u64::from(ImmersiveModeActionFlags::STRAFE_RIGHT.raw_flag_bits()),
        "look" => u64::from(ImmersiveModeActionFlags::LOOK.raw_flag_bits()),
        "primary" => u64::from(ImmersiveModeActionFlags::PRIMARY.raw_flag_bits()),
        "secondary" => u64::from(ImmersiveModeActionFlags::SECONDARY.raw_flag_bits()),
        "confirm" => u64::from(ImmersiveModeActionFlags::CONFIRM.raw_flag_bits()),
        "cancel" => u64::from(ImmersiveModeActionFlags::CANCEL.raw_flag_bits()),
        "zoom" => u64::from(ImmersiveModeActionFlags::ZOOM.raw_flag_bits()),
        "rotate" => u64::from(ImmersiveModeActionFlags::ROTATE.raw_flag_bits()),
        _ => return None,
    })
}

pub(super) fn immersive_mode_lifecycle_flag(value: &str) -> Option<u64> {
    Some(match value {
        "pausesimulation" => 1 << 0,
        "hidehud" => 1 << 1,
        "locksubject" => 1 << 2,
        "returncameraonexit" => 1 << 3,
        _ => return None,
    })
}

pub(super) fn immersive_mode_kind(value: &str) -> Result<ImmersiveModeKind, BindError> {
    enum_value(
        value,
        &[
            ("guestview", ImmersiveModeKind::GuestView),
            ("firstperson", ImmersiveModeKind::FirstPerson),
            ("training", ImmersiveModeKind::Training),
            ("fossilsearch", ImmersiveModeKind::FossilSearch),
            ("fossilassembly", ImmersiveModeKind::FossilAssembly),
            ("cloning", ImmersiveModeKind::Cloning),
            ("photo", ImmersiveModeKind::Photo),
            ("showedit", ImmersiveModeKind::ShowEdit),
        ],
        "immersive mode kind",
    )
}
