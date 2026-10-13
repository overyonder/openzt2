use bevy::prelude::*;

use openzt2_zoo_state::economy::money_types::Money;

#[derive(Component)]
pub(super) struct CursorMoneyContainer;

#[derive(Component)]
pub(super) struct CursorMoneyPreviewFragmentOwner;

#[derive(Component, Debug, Clone, Copy)]
pub(super) struct CursorMoneySpendFragmentOwner {
    pub(super) cost: Money,
    pub(super) screen_position: Vec2,
    pub(super) animation_started: bool,
}
