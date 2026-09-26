use super::{StaffJobCapabilityFlags, StaffJobKind};

impl StaffJobCapabilityFlags {
    pub const FEED: Self = Self(1 << 0);
    pub const REFILL_WATER: Self = Self(1 << 1);
    pub const CLEAN_HABITAT: Self = Self(1 << 2);
    pub const EMPTY_BIN: Self = Self(1 << 3);
    pub const SWEEP_LITTER: Self = Self(1 << 4);
    pub const REPAIR: Self = Self(1 << 5);
    pub const TREAT: Self = Self(1 << 6);
    pub const EDUCATE: Self = Self(1 << 7);
    pub const ENTERTAIN: Self = Self(1 << 8);
    pub const TRANQUILIZE: Self = Self(1 << 9);
    pub const CAPTURE: Self = Self(1 << 10);
    pub const MAINTAIN_TANK: Self = Self(1 << 11);
    pub const OPERATE_SHOW: Self = Self(1 << 12);

    const ALL_BITS: u32 = 1 << 0
        | 1 << 1
        | 1 << 2
        | 1 << 3
        | 1 << 4
        | 1 << 5
        | 1 << 6
        | 1 << 7
        | 1 << 8
        | 1 << 9
        | 1 << 10
        | 1 << 11
        | 1 << 12;

    #[must_use]
    pub const fn raw_flag_bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn from_raw_flag_bits(flag_bits: u32) -> Option<Self> {
        if flag_bits & !Self::ALL_BITS == 0 {
            Some(Self(flag_bits))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn with_additional_capabilities(self, additional_capabilities: Self) -> Self {
        Self(self.0 | additional_capabilities.0)
    }

    #[must_use]
    pub const fn contains_all(self, requested_capabilities: Self) -> bool {
        self.0 & requested_capabilities.0 == requested_capabilities.0
    }

    #[must_use]
    pub const fn for_job_kind(job_kind: StaffJobKind) -> Self {
        match job_kind {
            StaffJobKind::Feed => Self::FEED,
            StaffJobKind::RefillWater => Self::REFILL_WATER,
            StaffJobKind::CleanHabitat => Self::CLEAN_HABITAT,
            StaffJobKind::EmptyBin => Self::EMPTY_BIN,
            StaffJobKind::SweepLitter => Self::SWEEP_LITTER,
            StaffJobKind::Repair => Self::REPAIR,
            StaffJobKind::Treat => Self::TREAT,
            StaffJobKind::Educate => Self::EDUCATE,
            StaffJobKind::Entertain => Self::ENTERTAIN,
            StaffJobKind::Tranquilize => Self::TRANQUILIZE,
            StaffJobKind::Capture => Self::CAPTURE,
            StaffJobKind::MaintainTank => Self::MAINTAIN_TANK,
            StaffJobKind::OperateShow => Self::OPERATE_SHOW,
        }
    }
}
