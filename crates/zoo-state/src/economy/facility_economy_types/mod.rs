use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        facilities_and_maintenance::FacilityPaymentTrigger, staff_management::StaffRoleKind,
    },
    AssetId,
};

use super::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FacilityPriceIndex(pub u8);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaintenanceScheduleIndex(pub u8);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrentSellQuote(pub Money);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Wallet(pub Money);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Price(pub Money);

/// The authored recurring cost of one live object.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MonthlyUpkeep(pub Money);

/// A facility's authored operator class.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredFacilityStaff(pub StaffRoleKind);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceFacility {
    pub definition: AssetId,
    pub capacity: u16,
    pub occupied: u16,
    /// Payment details carried from the facility
    /// definition; the live service path authenticates against this instead
    /// of re-reading sources.
    pub payment_trigger: FacilityPaymentTrigger,
}

impl ServiceFacility {
    pub fn try_acquire(&mut self) -> bool {
        if self.occupied >= self.capacity {
            return false;
        }
        self.occupied += 1;
        true
    }

    pub fn release(&mut self) {
        self.occupied = self.occupied.saturating_sub(1);
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OperatingSinceDay(pub u32);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FacilityProfit {
    pub total: Money,
    pub transactions: u32,
}

impl FacilityProfit {
    pub fn average(self) -> Money {
        (self.transactions != 0)
            .then(|| Money(self.total.0 / i64::from(self.transactions)))
            .unwrap_or(Money::ZERO)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inventory {
    pub available: u16,
    pub reserved: u16,
    pub capacity: u16,
}

impl Inventory {
    pub fn can_reserve(&self, units: u16) -> bool {
        self.available >= units
            && self
                .available
                .checked_add(self.reserved)
                .is_some_and(|total| total <= self.capacity)
    }

    pub fn reserve(&mut self, units: u16) -> bool {
        if !self.can_reserve(units) {
            return false;
        }
        self.available -= units;
        self.reserved += units;
        true
    }

    pub fn consume_reserved(&mut self, units: u16) {
        self.reserved = self.reserved.saturating_sub(units);
    }

    pub fn return_reserved(&mut self, units: u16) {
        let returned = units.min(self.reserved);
        self.reserved -= returned;
        self.available = self
            .available
            .saturating_add(returned)
            .min(self.capacity.saturating_sub(self.reserved));
    }

    pub fn restock(&mut self, units: u32) {
        let available_limit = self.capacity.saturating_sub(self.reserved);
        self.available = u32::from(self.available)
            .saturating_add(units)
            .min(u32::from(available_limit)) as u16;
    }
}
