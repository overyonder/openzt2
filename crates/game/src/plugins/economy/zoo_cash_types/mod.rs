use bevy::prelude::Resource;

use super::money_types::Money;

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ZooCash(pub(crate) Money);

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UnlimitedZooCash;
