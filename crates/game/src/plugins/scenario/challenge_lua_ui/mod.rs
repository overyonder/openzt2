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
    let photo_challenge_layout = create_photo_challenge_layout(lua)?;
    let root = lua.create_table()?;
    root.set(
        "UI_GET_CHILD",
        scope.create_function(move |_, (_, name): (Table, String)| match name.as_str() {
            "challenge layout" => Ok(offer_panel.clone()),
            "instant challenge layout" => Ok(instant_panel.clone()),
            "photo challenges layout" => Ok(photo_challenge_layout.clone()),
            "challenge text" => Ok(offer_text.clone()),
            _ => Err(mlua::Error::runtime(format!(
                "scenario UI surface {name} is unsupported"
            ))),
        })?,
    )?;
    Ok(root)
}

/// `setchallengeactive` shows, and `resetchallengeover` hides, the Photo mode
/// decline button for the active challenge. Photo mode projects its whole
/// `photo challenges layout` hidden while it has no selected photo challenge,
/// so these calls change nothing that the player can see.
fn create_photo_challenge_layout(lua: &Lua) -> mlua::Result<Table> {
    let decline = lua.create_table()?;
    decline.set("UI_SHOW", lua.create_function(|_, _: Table| Ok(()))?)?;
    decline.set("UI_HIDE", lua.create_function(|_, _: Table| Ok(()))?)?;
    let layout = lua.create_table()?;
    layout.set(
        "UI_GET_CHILD",
        lua.create_function(move |_, (_, name): (Table, String)| {
            if name == "decline" {
                Ok(decline.clone())
            } else {
                Err(mlua::Error::runtime(format!(
                    "photo challenge UI child {name} is unsupported"
                )))
            }
        })?,
    )?;
    Ok(layout)
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
        scenario,
        show: None,
        text: None,
        description: None,
    }
}
