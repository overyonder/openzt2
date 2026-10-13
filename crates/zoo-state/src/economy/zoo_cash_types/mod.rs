use bevy::prelude::Resource;

use super::money_types::Money;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZooCash(pub Money);

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnlimitedZooCash;
