//! Narrow native UI calls used by the shipped `showchallengepanel` and
//! `showinstantchallengepanel` Lua helpers.

use std::cell::RefCell;

use mlua::{Lua, Scope, Table};
use openzt2_game_assets::source_document::source_document_semantic_name::canonicalize_source_document_record_key;
use openzt2_game_data::AssetId;

use super::challenge_offer_types::{ScenarioChallengePanelRequest, ScenarioChallengePanelSurface};

type ScenarioChallengePanelRequestField =
    fn(&mut ScenarioChallengePanelRequest) -> &mut Option<AssetId>;

pub(super) fn create_challenge_ui_root<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    scenario: AssetId,
    requested: &'env RefCell<Option<ScenarioChallengePanelRequest>>,
) -> mlua::Result<Table> {
    let offer_text = create_challenge_ui_text_node(lua, scope, scenario, requested, |request| {
        &mut request.text
    })?;
    let offer_panel = create_challenge_ui_panel(
        lua,
        scope,
        scenario,
        requested,
        ScenarioChallengePanelSurface::Offer,
        vec![("challenge text", offer_text.clone())],
    )?;
    let instant_panel = create_challenge_ui_panel(
        lua,
        scope,
        scenario,
        requested,
        ScenarioChallengePanelSurface::InstantOffer,
        vec![
            (
                "instant challenge description text",
                create_challenge_ui_text_node(lua, scope, scenario, requested, |request| {
                    &mut request.description
                })?,
            ),
            (
                "instant challenge text",
                create_challenge_ui_text_node(lua, scope, scenario, requested, |request| {
                    &mut request.text
                })?,
            ),
        ],
    )?;
    let root = lua.create_table()?;
    root.set(
        "UI_GET_CHILD",
        scope.create_function(move |_, (_, name): (Table, String)| match name.as_str() {
            "challenge layout" => Ok(offer_panel.clone()),
            "instant challenge layout" => Ok(instant_panel.clone()),
            "challenge text" => Ok(offer_text.clone()),
            _ => Err(mlua::Error::runtime(format!(
                "scenario UI surface {name} is unsupported"
            ))),
        })?,
    )?;
    Ok(root)
}

fn create_challenge_ui_panel<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    scenario: AssetId,
    requested: &'env RefCell<Option<ScenarioChallengePanelRequest>>,
    surface: ScenarioChallengePanelSurface,
    children: Vec<(&'static str, Table)>,
) -> mlua::Result<Table> {
    let panel = lua.create_table()?;
    panel.set(
        "UI_GET_CHILD",
        scope.create_function(move |_, (_, name): (Table, String)| {
            children
                .iter()
                .find(|(child, _)| *child == name)
                .map(|(_, node)| node.clone())
                .ok_or_else(|| {
                    mlua::Error::runtime(format!("challenge UI child {name} is unsupported"))
                })
        })?,
    )?;
    panel.set(
        "UI_SHOW",
        scope.create_function(move |_, _: Table| {
            let mut requested = requested.borrow_mut();
            requested
                .get_or_insert_with(|| empty_challenge_panel_request(scenario))
                .show = Some(surface);
            Ok(())
        })?,
    )?;
    Ok(panel)
}

fn create_challenge_ui_text_node<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    scenario: AssetId,
    requested: &'env RefCell<Option<ScenarioChallengePanelRequest>>,
    field: ScenarioChallengePanelRequestField,
) -> mlua::Result<Table> {
    let text = lua.create_table()?;
    text.set(
        "UI_SET_LOCID",
        scope.create_function(move |_, (_, key): (Table, String)| {
            let mut requested = requested.borrow_mut();
            // Localization entries are keyed by their canonical identifier.
            *field(requested.get_or_insert_with(|| empty_challenge_panel_request(scenario))) = Some(
                AssetId::from_key(&canonicalize_source_document_record_key(&key)),
            );
            Ok(())
        })?,
    )?;
    Ok(text)
}

const fn empty_challenge_panel_request(scenario: AssetId) -> ScenarioChallengePanelRequest {
    ScenarioChallengePanelRequest {
        source: None,
        scenario,
        show: None,
        text: None,
        description: None,
    }
}
