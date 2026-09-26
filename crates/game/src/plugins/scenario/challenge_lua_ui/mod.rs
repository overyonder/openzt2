//! Narrow native UI calls used by the shipped `showchallengepanel` Lua helper.

use std::cell::RefCell;

use mlua::{Lua, Scope, Table};
use openzt2_game_data::AssetId;

use super::challenge_offer_types::ScenarioChallengePanelRequest;

pub(super) fn create_challenge_ui_root<'scope, 'env: 'scope>(
    lua: &Lua,
    scope: &'scope Scope<'scope, 'env>,
    scenario: AssetId,
    requested: &'env RefCell<Option<ScenarioChallengePanelRequest>>,
) -> mlua::Result<Table> {
    let text = lua.create_table()?;
    text.set(
        "UI_SET_LOCID",
        scope.create_function(move |_, (_, key): (Table, String)| {
            requested
                .borrow_mut()
                .get_or_insert(ScenarioChallengePanelRequest {
                    scenario,
                    show: false,
                    text: None,
                })
                .text = Some(AssetId::from_key(&key));
            Ok(())
        })?,
    )?;
    let panel = lua.create_table()?;
    let panel_text = text.clone();
    panel.set(
        "UI_GET_CHILD",
        scope.create_function(move |_, (_, name): (Table, String)| {
            if name == "challenge text" {
                Ok(panel_text.clone())
            } else {
                Err(mlua::Error::runtime(format!(
                    "challenge UI child {name} is unsupported"
                )))
            }
        })?,
    )?;
    panel.set(
        "UI_SHOW",
        scope.create_function(move |_, _: Table| {
            requested
                .borrow_mut()
                .get_or_insert(ScenarioChallengePanelRequest {
                    scenario,
                    show: false,
                    text: None,
                })
                .show = true;
            Ok(())
        })?,
    )?;
    let root = lua.create_table()?;
    root.set(
        "UI_GET_CHILD",
        scope.create_function(move |_, (_, name): (Table, String)| {
            if name == "challenge layout" {
                Ok(panel.clone())
            } else if name == "challenge text" {
                Ok(text.clone())
            } else {
                Err(mlua::Error::runtime(format!(
                    "scenario UI surface {name} is unsupported"
                )))
            }
        })?,
    )?;
    Ok(root)
}
