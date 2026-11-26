//! The `ZTPhotoChallengesComponent` calls that shipped challenge scripts and
//! `photoutil.lua` make for a scenario's current photo challenge set.

use mlua::{Lua, Scope, Table};
use openzt2_game_data::AssetId;

use super::scenario_lua_types::{ScenarioLuaPhotoChallengeMemberStars, ScenarioLuaQueryFacts};

pub(super) fn create_scenario_lua_photo_challenge_component<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    queries: ScenarioLuaQueryFacts<'env>,
) -> mlua::Result<Table> {
    let select = move |requested| {
        queries
            .photo_set
            .zip(queries.world)
            .ok_or_else(|| mlua::Error::runtime("photo challenge catalogue is unavailable"))
            .and_then(|(selection, world)| selection(world, requested))
    };
    let members = move || {
        select(None)?
            .and_then(|entity| queries.read_photo_challenge_member_stars(entity))
            .ok_or_else(|| mlua::Error::runtime("no photo challenge set is current"))
    };
    let member = move |index: usize| -> mlua::Result<ScenarioLuaPhotoChallengeMemberStars> {
        members()?.get(index).copied().ok_or_else(|| {
            mlua::Error::runtime(format!("photo challenge set has no member {index}"))
        })
    };
    let component = lua.create_table()?;
    component.set(
        "ZT_PHOTOEVENT_SET_CURRENT_CHALLENGE",
        scope.create_function(move |_, (_, set): (Table, String)| {
            select(Some(AssetId::from_key(&set)))?;
            Ok(())
        })?,
    )?;
    component.set(
        "ZT_PHOTOEVENT_GET_NUM_CHALLENGE_PHOTOS",
        scope.create_function(move |_, _: Table| Ok(members()?.len()))?,
    )?;
    // `numPhotoRequirementsMet` indexes the members from zero.
    component.set(
        "ZT_PHOTOEVENT_GET_REQUIRED_CHALLENGE_STARS",
        scope.create_function(move |_, (_, index): (Table, usize)| Ok(member(index)?.required))?,
    )?;
    component.set(
        "ZT_PHOTOEVENT_GET_OBTAINED_CHALLENGE_STARS",
        scope.create_function(move |_, (_, index): (Table, usize)| {
            Ok(f64::from(member(index)?.obtained_half_stars) / 2.0)
        })?,
    )?;
    // Complete when every member has a photo rated at least its difficulty,
    // the condition `numPhotoRequirementsMet` counts.
    component.set(
        "ZT_PHOTOEVENT_GET_CHALLENGES_COMPLETED",
        scope.create_function(move |_, _: Table| {
            Ok(members()?
                .iter()
                .all(|member| member.obtained_half_stars >= member.required.saturating_mul(2)))
        })?,
    )?;
    Ok(component)
}
