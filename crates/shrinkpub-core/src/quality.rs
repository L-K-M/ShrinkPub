/// A compression tier, from "barely touch it" to "scorched earth".
///
/// Each tier maps to a JPEG encode quality, a PNG quantization quality range,
/// and (for the lower tiers) a maximum image width — images wider than the cap
/// are downscaled before re-encoding, since huge page scans dominate EPUB size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Terrible,
    Atrocious,
}

impl Quality {
    pub const ALL: [Quality; 7] = [
        Quality::VeryHigh,
        Quality::High,
        Quality::Medium,
        Quality::Low,
        Quality::VeryLow,
        Quality::Terrible,
        Quality::Atrocious,
    ];

    /// Stable machine identifier, used over IPC and in saved settings.
    pub fn id(self) -> &'static str {
        match self {
            Quality::VeryHigh => "veryhigh",
            Quality::High => "high",
            Quality::Medium => "medium",
            Quality::Low => "low",
            Quality::VeryLow => "verylow",
            Quality::Terrible => "terrible",
            Quality::Atrocious => "atrocious",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Quality::VeryHigh => "Very High",
            Quality::High => "High",
            Quality::Medium => "Medium",
            Quality::Low => "Low",
            Quality::VeryLow => "Very Low",
            Quality::Terrible => "Terrible",
            Quality::Atrocious => "Atrocious",
        }
    }

    pub fn from_id(id: &str) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.id() == id)
    }

    pub(crate) fn jpeg_quality(self) -> u8 {
        match self {
            Quality::VeryHigh => 90,
            Quality::High => 80,
            Quality::Medium => 70,
            Quality::Low => 55,
            Quality::VeryLow => 40,
            Quality::Terrible => 25,
            Quality::Atrocious => 10,
        }
    }

    /// (min, max) quality for imagequant, on its 0–100 scale. The min is a
    /// floor below which quantization *fails* and the original is kept — high
    /// tiers refuse to butcher hard-to-quantize (photographic) PNGs, low
    /// tiers accept whatever they can get.
    pub(crate) fn png_quality(self) -> (u8, u8) {
        match self {
            Quality::VeryHigh => (70, 95),
            Quality::High => (50, 85),
            Quality::Medium => (30, 70),
            Quality::Low => (15, 55),
            Quality::VeryLow => (10, 40),
            Quality::Terrible => (5, 25),
            Quality::Atrocious => (0, 15),
        }
    }

    pub(crate) fn max_width(self) -> Option<u32> {
        match self {
            Quality::VeryHigh | Quality::High | Quality::Medium => None,
            Quality::Low => Some(1600),
            Quality::VeryLow => Some(1200),
            Quality::Terrible => Some(900),
            Quality::Atrocious => Some(600),
        }
    }
}

impl std::fmt::Display for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for q in Quality::ALL {
            assert_eq!(Quality::from_id(q.id()), Some(q));
        }
        assert_eq!(Quality::from_id("bogus"), None);
    }

    #[test]
    fn tiers_get_monotonically_harsher() {
        for pair in Quality::ALL.windows(2) {
            assert!(pair[0].jpeg_quality() > pair[1].jpeg_quality());
            assert!(pair[0].png_quality().1 > pair[1].png_quality().1);
            let cap = |q: Quality| q.max_width().unwrap_or(u32::MAX);
            assert!(cap(pair[0]) >= cap(pair[1]));
        }
    }
}
