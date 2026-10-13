use std::collections::{BTreeMap, BTreeSet};

use mlua::Lua;

use crate::plugins::economy::guest_admission_types::AdmissionPrice;
use crate::plugins::economy::scenario_economy_command_types::ScenarioEconomyOperation;
use crate::plugins::economy::zoo_cash_types::ZooCash;
use crate::plugins::progression::fame_types::Fame;
use crate::plugins::simulation_time::simulation_clock_types::ZooCalendar;
use crate::plugins::simulation_time::simulation_clock_types::ZooClock;
use openzt2_game_assets::lua_script::lua_script_asset_loading::LuaScriptAsset;
use openzt2_game_assets::world_scenario::world_scenario_document_asset_and_dependency_handles::WorldScenarioDocumentAsset;

pub(super) struct ScenarioLuaScriptResult {
    /// The photo challenge set named by `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`.
    pub(super) current_photo_challenge_set: Option<openzt2_game_data::AssetId>,
    /// The error that ended the call early. Native calls made before it have
    /// already acted and are carried in the fields below.
    pub(super) failure: Option<mlua::Error>,
    pub(super) available_content: Vec<openzt2_game_data::AssetId>,
    pub(super) shown_rules: Vec<super::scenario_objective_types::ShowScenarioRuleRequest>,
    pub(super) condition_status: super::scenario_objective_types::ScenarioObjectiveStatus,
    pub(super) economy_operations: Vec<ScenarioEconomyOperation>,
    pub(super) award_point_adjustments: Vec<i32>,
    pub(super) challenge_offer_source_paths: Vec<String>,
    pub(super) challenge_panel: Option<super::challenge_offer_types::ScenarioChallengePanelRequest>,
}

#[derive(Clone, Copy)]
pub(super) struct ScenarioLuaQueryFacts<'a> {
    pub(super) zoo_cash: &'a ZooCash,
    pub(super) guest_admission_price: &'a AdmissionPrice,
    pub(super) finance_history:
        &'a crate::plugins::economy::monthly_finance_types::MonthlyFinanceHistory,
    pub(super) zoo_fame: &'a Fame,
    pub(super) zoo_clock: Option<&'a ZooClock>,
    pub(super) zoo_calendar: Option<&'a ZooCalendar>,
    /// Months since the calendar epoch, the first day of every new zoo.
    pub(super) zoo_months_since_calendar_epoch: Option<i64>,
    pub(super) zoo_month_progress:
        Option<super::scenario_lua_query_fact_collection::ScenarioZooMonthProgress>,
    pub(super) live_world_entity_counts_by_definition_identifier:
        &'a BTreeMap<openzt2_game_data::AssetId, u32>,
    pub(super) live_world_animals: &'a ScenarioLuaLiveAnimalQuery<'a, 'a>,
    pub(super) scenario_document_assets: &'a bevy::prelude::Assets<WorldScenarioDocumentAsset>,
    pub(super) current_photo_challenge_sets: &'a ScenarioLuaCurrentPhotoChallengeSetQuery<'a, 'a>,
    pub(super) photo_challenges: &'a ScenarioLuaPhotoChallengeQuery<'a, 'a>,
    pub(super) selected_entity: Option<bevy::prelude::Entity>,
}

/// Animals in the live world, with whether each is pregnant.
pub(super) type ScenarioLuaLiveAnimalQuery<'w, 's> = bevy::prelude::Query<
    'w,
    's,
    (
        bevy::prelude::Entity,
        bevy::prelude::Has<crate::plugins::animal_lifecycle::types::Pregnancy>,
    ),
    (
        bevy::prelude::With<crate::plugins::animal_lifecycle::types::Animal>,
        bevy::prelude::With<crate::plugins::world_spawn::world_membership_types::WorldMember>,
    ),
>;

#[derive(Default)]
pub(super) struct ScenarioLuaVirtualMachineContexts {
    pub(super) virtual_machines_by_scenario_identifier:
        BTreeMap<openzt2_game_data::AssetId, ScenarioLuaVirtualMachine>,
}

pub(super) struct ScenarioLuaVirtualMachine {
    pub(super) lua_virtual_machine: Lua,
    pub(super) scenario_document_asset_identifier: bevy::asset::AssetId<WorldScenarioDocumentAsset>,
    pub(super) loaded_lua_script_asset_identifiers: BTreeSet<bevy::asset::AssetId<LuaScriptAsset>>,
}

/// The photo challenge set selected by `ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE`.
pub(super) type ScenarioLuaCurrentPhotoChallengeSetQuery<'w, 's> = bevy::prelude::Query<
    'w,
    's,
    &'static crate::plugins::photos::photo_challenge_types::PhotoChallengeSet,
    bevy::prelude::With<crate::plugins::photos::photo_challenge_types::CurrentPhotoChallengeSet>,
>;

/// Live photo challenges and their progress.
pub(super) type ScenarioLuaPhotoChallengeQuery<'w, 's> = bevy::prelude::Query<
    'w,
    's,
    (
        &'static crate::plugins::photos::photo_challenge_types::PhotoChallenge,
        &'static crate::plugins::photos::photo_challenge_types::PhotoChallengeProgress,
    ),
>;

/// Stars one member of the current photo challenge set requires and has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ScenarioLuaPhotoChallengeMemberStars {
    /// The member's authored `difficulty`.
    pub(super) required: u8,
    /// The rating of the photo that completed it, or 0.
    pub(super) obtained: u8,
}

impl ScenarioLuaQueryFacts<'_> {
    /// The current photo challenge set's members in authored order, or `None`
    /// when no set is current.
    pub(super) fn read_current_photo_challenge_member_stars(
        &self,
    ) -> Option<Vec<ScenarioLuaPhotoChallengeMemberStars>> {
        let set = self.current_photo_challenge_sets.iter().next()?;
        let asset = self.scenario_document_assets.get(&set.source)?;
        let record = asset.document.photo_challenge_sets.get(set.row as usize)?;
        record
            .members
            .iter()
            .map(|member| {
                let (challenge, progress) = self
                    .photo_challenges
                    .iter()
                    .find(|(challenge, _)| challenge.id == member.challenge)?;
                let rule = self
                    .scenario_document_assets
                    .get(&challenge.source)?
                    .document
                    .photo_challenges
                    .get(challenge.row as usize)?;
                Some(ScenarioLuaPhotoChallengeMemberStars {
                    required: rule.difficulty,
                    obtained: if progress.completed {
                        progress.rating_stars
                    } else {
                        0
                    },
                })
            })
            .collect()
    }
}
