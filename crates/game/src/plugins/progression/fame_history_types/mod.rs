use bevy::prelude::*;

/// One canonical end-of-update fame value for a zoo calendar month.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FameHistorySample {
    pub month_index: u32,
    pub half_stars: u8,
}

/// Progression-owned fame history indexed by the original zero-based zoo month.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct FameHistory {
    samples: Vec<FameHistorySample>,
}

impl FameHistory {
    pub fn samples(&self) -> &[FameHistorySample] {
        &self.samples
    }

    pub fn value(&self, month_index: u32) -> Option<u8> {
        self.samples
            .binary_search_by_key(&month_index, |sample| sample.month_index)
            .ok()
            .map(|index| self.samples[index].half_stars)
    }

    pub(crate) fn clear(&mut self) {
        self.samples.clear();
    }

    /// Replaces the canonical series after persistence has decoded a saved section.
    /// Invalid or duplicate month indexes leave the current series untouched.
    pub(crate) fn replace_saved(&mut self, samples: Vec<FameHistorySample>) -> bool {
        if !samples
            .windows(2)
            .all(|pair| pair[0].month_index < pair[1].month_index)
        {
            return false;
        }
        self.samples = samples;
        true
    }

    pub(crate) fn record_monthly_value(&mut self, month_index: u32, half_stars: u8) {
        match self.samples.last().copied() {
            Some(sample)
                if sample.month_index == month_index && sample.half_stars == half_stars => {}
            Some(sample) if sample.month_index == month_index => {
                if let Some(sample) = self.samples.last_mut() {
                    sample.half_stars = half_stars;
                }
            }
            Some(sample) if sample.month_index > month_index => {
                self.samples.clear();
                self.samples.push(FameHistorySample {
                    month_index,
                    half_stars,
                });
            }
            _ => self.samples.push(FameHistorySample {
                month_index,
                half_stars,
            }),
        }
    }
}
