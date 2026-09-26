use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
pub struct UiNodeRegionDefinition {
    pub metrics: [UiNodeRegionMetric; 4],
    pub alignment: [UiNodeRegionAlignment; 4],
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq)]
pub enum UiNodeRegionMetric {
    Pixels(f32),
    OutsideTop,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UiNodeRegionAlignment {
    Minimum,
    Maximum,
    Length,
    Middle,
    PercentageFromMinimum,
    PercentageFromMaximum,
}

impl Default for UiNodeRegionDefinition {
    fn default() -> Self {
        Self {
            metrics: [UiNodeRegionMetric::Pixels(0.0); 4],
            alignment: [
                UiNodeRegionAlignment::Minimum,
                UiNodeRegionAlignment::Minimum,
                UiNodeRegionAlignment::Length,
                UiNodeRegionAlignment::Length,
            ],
        }
    }
}
