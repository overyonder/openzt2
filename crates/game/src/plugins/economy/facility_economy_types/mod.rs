use bevy::prelude::*;
use openzt2_game_data::{
    world_definitions::{
        facilities_and_maintenance::FacilityPaymentTrigger, staff_management::StaffRoleKind,
    },
    AssetId,
};

use super::money_types::Money;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FacilityPriceIndex(pub(crate) u8);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MaintenanceScheduleIndex(pub(crate) u8);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurrentSellQuote(pub(crate) Money);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Wallet(pub(crate) Money);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Price(pub(crate) Money);

/// The authored recurring cost of one live object.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MonthlyUpkeep(pub(crate) Money);

/// A facility's authored operator class.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequiredFacilityStaff(pub(crate) StaffRoleKind);

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ServiceFacility {
    pub(crate) definition: AssetId,
    pub(crate) capacity: u16,
    pub(crate) occupied: u16,
    /// Payment details carried from the facility
    /// definition; the live service path authenticates against this instead
    /// of re-reading sources.
    pub(crate) payment_trigger: FacilityPaymentTrigger,
}

impl ServiceFacility {
    pub(crate) fn try_acquire(&mut self) -> bool {
        if self.occupied >= self.capacity {
            return false;
        }
        self.occupied += 1;
        true
    }

    pub(crate) fn release(&mut self) {
        self.occupied = self.occupied.saturating_sub(1);
    }
}

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct OperatingSinceDay(pub(crate) u32);

#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FacilityProfit {
    pub(crate) total: Money,
    pub(crate) transactions: u32,
}

impl FacilityProfit {
    pub(crate) fn average(self) -> Money {
        (self.transactions != 0)
            .then(|| Money(self.total.0 / i64::from(self.transactions)))
            .unwrap_or(Money::ZERO)
    }
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inventory {
    pub(crate) available: u16,
    pub(crate) reserved: u16,
    pub(crate) capacity: u16,
}

impl Inventory {
    pub(crate) fn can_reserve(&self, units: u16) -> bool {
        self.available >= units
            && self
                .available
                .checked_add(self.reserved)
                .is_some_and(|total| total <= self.capacity)
    }

    pub(crate) fn reserve(&mut self, units: u16) -> bool {
        if !self.can_reserve(units) {
            return false;
        }
        self.available -= units;
        self.reserved += units;
        true
    }

    pub(crate) fn consume_reserved(&mut self, units: u16) {
        self.reserved = self.reserved.saturating_sub(units);
    }

    pub(crate) fn return_reserved(&mut self, units: u16) {
        let returned = units.min(self.reserved);
        self.reserved -= returned;
        self.available = self
            .available
            .saturating_add(returned)
            .min(self.capacity.saturating_sub(self.reserved));
    }

    pub(crate) fn restock(&mut self, units: u32) {
        let available_limit = self.capacity.saturating_sub(self.reserved);
        self.available = u32::from(self.available)
            .saturating_add(units)
            .min(u32::from(available_limit)) as u16;
    }
}
