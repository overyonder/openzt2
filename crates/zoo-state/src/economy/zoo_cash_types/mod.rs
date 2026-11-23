use bevy::prelude::Component;

use super::money_types::Money;

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooCash(pub Money);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnlimitedZooCash(pub bool);
