use super::object_facility_staff_and_guest_source_vocabulary::{staff_job_kind, staff_role_kind};
use super::world_definition_lowering_tables::WorldDefinitionLoweringTables;
use super::world_definition_source_flag_vocabulary::staff_job_flag;
use super::world_definition_source_value_reading_and_conversion::{
    asset, asset_list, flags, id, number_or, required, required_number,
};
use crate::assets::source_document::resolved_source_record_index::{BindError, RecordView};
use openzt2_game_data::world_definitions::staff_management::{
    StaffJobCapabilityFlags, StaffJobDefinition, StaffJobEffect, StaffJobKind, StaffRoleDefinition,
};
use openzt2_game_data::AssetId;

pub(super) fn bind_staff(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let jobs = record
        .value(&["jobOverrides", "jobs"])
        .map(asset_list)
        .unwrap_or_default();
    let job_overrides = jobs;
    let name_pool = record
        .value(&["namePool"])
        .map(id)
        .unwrap_or_else(|| id(&format!("personname:{}", record.key)));
    output.document.staff.push(StaffRoleDefinition {
        id: id(record.key),
        object: asset(record, &["object", "entity"]),
        name_pool,
        role: staff_role_kind(required(record, &["role", "staffType"])?)?,
        model_animation_set: AssetId::default(),
        initial_animation_clip_asset_key: None,
        initial_animation_loops: false,
        wage_cents_per_month: number_or(record, &["wageCentsPerMonth", "wage", "salary"], 0)?,
        move_speed_mps: required_number(record, &["moveSpeedMps", "moveSpeed", "speed"])?,
        navigation_radius_m: required_number(record, &["navigationRadiusM", "radiusM", "radius"])?,
        permitted_jobs: StaffJobCapabilityFlags::from_raw_flag_bits(flags(
            record.value(&["permittedJobs", "jobs"]),
            staff_job_flag,
        )? as u32)
        .ok_or_else(|| {
            BindError::record(record, "staff role contains unknown job capability bits")
        })?,
        job_overrides,
        presentation_variants: Vec::new(),
    });
    Ok(())
}

pub(super) fn bind_staff_job(
    record: &RecordView<'_, '_>,
    output: &mut WorldDefinitionLoweringTables,
) -> Result<(), BindError> {
    let kind = staff_job_kind(required(record, &["kind", "jobType"])?)?;
    let effect = match kind {
        StaffJobKind::Feed | StaffJobKind::RefillWater => StaffJobEffect::Fill {
            amount: number_or(record, &["amount", "fillAmount"], 1)?,
        },
        StaffJobKind::CleanHabitat
        | StaffJobKind::EmptyBin
        | StaffJobKind::SweepLitter
        | StaffJobKind::MaintainTank => StaffJobEffect::Clean {
            amount: number_or(record, &["amount", "cleanAmount"], 1)?,
        },
        StaffJobKind::Repair => StaffJobEffect::Repair {
            amount: number_or(record, &["amount", "repairAmount"], 1)?,
        },
        StaffJobKind::Treat => StaffJobEffect::Treat {
            treatment: asset(record, &["treatment"]),
        },
        StaffJobKind::Educate | StaffJobKind::Entertain => StaffJobEffect::Educate {
            amount: number_or(record, &["amount"], 1)?,
        },
        StaffJobKind::Tranquilize => StaffJobEffect::Tranquilize {
            definition: asset(record, &["tranquilizer"]),
        },
        StaffJobKind::Capture => StaffJobEffect::Capture,
        StaffJobKind::OperateShow => StaffJobEffect::Operate,
    };
    output.document.staff_jobs.push(StaffJobDefinition {
        id: id(record.key),
        kind,
        duration_ticks: required_number(record, &["durationTicks"])?,
        interaction_radius_cm: number_or(record, &["interactionRadiusCm", "radiusCm"], 0)?,
        capability: StaffJobCapabilityFlags::for_job_kind(kind),
        effect,
        priority: number_or(record, &["priority"], 0)?,
    });
    Ok(())
}
