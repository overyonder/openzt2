use std::collections::BTreeMap;

use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::staff_management::{StaffJobCapabilityFlags, StaffRoleDefinition},
    AssetId,
};

use crate::behavior::behavior_asset_types::{
    BehaviorDocumentAsset, LoadedBehaviorDocumentCollection,
};

use super::world_definition_asset_set_state_and_borrowing_queries::WorldDefinitionsView;

/// Supported staff job kinds declared by the winning authored behavior tasks.
#[derive(Resource, Default, PartialEq, Eq)]
pub struct LoadedStaffBehaviorCapabilities {
    jobs_by_subject: BTreeMap<AssetId, StaffJobCapabilityFlags>,
}

impl LoadedStaffBehaviorCapabilities {
    pub fn for_employee(
        &self,
        definitions: WorldDefinitionsView<'_>,
        staff: &StaffRoleDefinition,
    ) -> StaffJobCapabilityFlags {
        let Some(object) = definitions.find_object(staff.object) else {
            return staff.permitted_jobs;
        };
        object
            .behavior_type_identifiers
            .iter()
            .filter_map(|subject| self.jobs_by_subject.get(subject).copied())
            .fold(
                staff.permitted_jobs,
                StaffJobCapabilityFlags::with_additional_capabilities,
            )
    }
}

pub(super) fn publish_staff_behavior_capabilities(
    documents: Option<Res<LoadedBehaviorDocumentCollection>>,
    assets: Option<Res<Assets<BehaviorDocumentAsset>>>,
    mut capabilities: ResMut<LoadedStaffBehaviorCapabilities>,
) {
    let (Some(documents), Some(assets)) = (documents, assets) else {
        return;
    };
    if !documents.is_changed() && !assets.is_changed() {
        return;
    }
    let Some(view) = documents.create_declaration_index_view(&assets) else {
        return;
    };
    let mut jobs_by_subject = BTreeMap::<AssetId, StaffJobCapabilityFlags>::new();
    for (_, _, task) in view.behavior_tasks() {
        let mut jobs = StaffJobCapabilityFlags::default();
        for token in &task.objects {
            jobs = jobs.with_additional_capabilities(capability_for_authored_token(token));
        }
        for target in &task.targets {
            let capability = match target.trim().to_ascii_lowercase().as_str() {
                "educator_podium_df" | "fossileducationcenter" => StaffJobCapabilityFlags::EDUCATE,
                "stageentertainer" => StaffJobCapabilityFlags::ENTERTAIN
                    .with_additional_capabilities(StaffJobCapabilityFlags::OPERATE_SHOW),
                "fossilsite" | "fossilmarker" => StaffJobCapabilityFlags::CAPTURE,
                _ => continue,
            };
            jobs = jobs.with_additional_capabilities(capability);
        }
        if jobs == StaffJobCapabilityFlags::default() {
            continue;
        }
        for subject in &task.subjects {
            let id = AssetId::from_key(&subject.trim().to_ascii_lowercase());
            let capabilities = jobs_by_subject.entry(id).or_default();
            *capabilities = capabilities.with_additional_capabilities(jobs);
        }
    }
    capabilities.set_if_neq(LoadedStaffBehaviorCapabilities { jobs_by_subject });
}

fn capability_for_authored_token(token: &str) -> StaffJobCapabilityFlags {
    match token.trim().to_ascii_lowercase().as_str() {
        "t_fillfoodcontainer" | "t_feedanimal" => StaffJobCapabilityFlags::FEED,
        "t_wateranimal" => StaffJobCapabilityFlags::REFILL_WATER,
        "t_rakepoo" | "t_cleananimal" => StaffJobCapabilityFlags::CLEAN_HABITAT,
        "t_emptytrash" | "t_emptyrecyclingbin" => StaffJobCapabilityFlags::EMPTY_BIN,
        "t_sweeptrash" => StaffJobCapabilityFlags::SWEEP_LITTER,
        "t_cureanimal" | "t_healanimal" => StaffJobCapabilityFlags::TREAT,
        "t_entertainerguest" => StaffJobCapabilityFlags::ENTERTAIN,
        "t_tranqdino" => StaffJobCapabilityFlags::TRANQUILIZE,
        "t_crateanimal" | "t_cratedinosaur" | "t_distractdino" => StaffJobCapabilityFlags::CAPTURE,
        "t_cleanwaterfilter" | "t_cleantank" => StaffJobCapabilityFlags::MAINTAIN_TANK,
        "t_trainanimal" | "t_mcexecuteshow" => StaffJobCapabilityFlags::OPERATE_SHOW,
        _ => StaffJobCapabilityFlags::default(),
    }
}
