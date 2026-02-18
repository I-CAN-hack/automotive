//! Helpers for building CAN bitrate settings.
//!
//! This module provides a single builder that supports two input modes:
//! - target `bitrate` (+ optional `sample_point`)
//! - direct timing parameters (`brp`, `tseg1`, `tseg2`, optional `sjw`)
//!
//! The resulting [`BitrateConfig`] contains:
//! - the adapter-facing values (`brp`, `tseg1`, `tseg2`, `sjw`)
//! - the resulting `bitrate` and `sample_point`

use thiserror::Error;

const CAN_SYNC_SEG: u32 = 1;
const CAN_CALC_MAX_ERROR: u32 = 50; // 0.50% in one-hundredth percent units
const SAMPLE_POINT_SCALE: f64 = 1000.0;

/// Hardware limits used to calculate and validate CAN bit timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BitTimingConst {
    pub clock_hz: u32,
    pub tseg1_min: u32,
    pub tseg1_max: u32,
    pub tseg2_min: u32,
    pub tseg2_max: u32,
    pub sjw_max: u32,
    pub brp_min: u32,
    pub brp_max: u32,
    pub brp_inc: u32,
}

/// Generic timing values typically needed by CAN adapter drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdapterBitTiming {
    pub brp: u32,
    pub tseg1: u32,
    pub tseg2: u32,
    pub sjw: u32,
}

/// Resolved bitrate configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BitrateConfig {
    /// Adapter-facing timing values.
    pub timing: AdapterBitTiming,
    /// Actual bitrate in bits per second.
    pub bitrate: u32,
    /// Actual sample point in normalized form (`0.0..1.0`).
    pub sample_point: f64,
    /// Time quantum in nanoseconds.
    pub tq_ns: u32,
}

impl BitrateConfig {
    /// Duration of one bit in time quanta.
    pub fn bit_time_tq(&self) -> u32 {
        CAN_SYNC_SEG + self.timing.tseg1 + self.timing.tseg2
    }
}

#[derive(Debug, Clone, Copy)]
struct SamplePointCandidate {
    sample_point: u32,
    sample_point_error: u32,
    tseg1: u32,
    tseg2: u32,
}

/// Error type returned by [`BitrateBuilder::build`].
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum BitrateError {
    #[error("clock_hz must be greater than 0")]
    InvalidClock,
    #[error("bitrate must be greater than 0")]
    InvalidBitrate,
    #[error("sample_point must be in range [0.0, 1.0)")]
    InvalidSamplePoint,
    #[error("cannot mix bitrate-based and direct timing configuration")]
    MixedConfiguration,
    #[error("sample_point can only be used in bitrate mode")]
    SamplePointRequiresBitrate,
    #[error("no bitrate configuration provided")]
    MissingConfiguration,
    #[error("missing direct timing field: {0}")]
    MissingDirectField(&'static str),
    #[error("brp {brp} is out of range [{min}, {max}]")]
    BrpOutOfRange { brp: u32, min: u32, max: u32 },
    #[error("brp {brp} must align with brp_inc {brp_inc}")]
    InvalidBrpIncrement { brp: u32, brp_inc: u32 },
    #[error("tseg1 {tseg1} is out of range [{min}, {max}]")]
    Tseg1OutOfRange { tseg1: u32, min: u32, max: u32 },
    #[error("tseg2 {tseg2} is out of range [{min}, {max}]")]
    Tseg2OutOfRange { tseg2: u32, min: u32, max: u32 },
    #[error("sjw {sjw} is greater than max sjw {max_sjw}")]
    SjwGreaterThanMax { sjw: u32, max_sjw: u32 },
    #[error("sjw {sjw} is greater than phase-seg1 {phase_seg1}")]
    SjwGreaterThanPhaseSeg1 { sjw: u32, phase_seg1: u32 },
    #[error("sjw {sjw} is greater than phase-seg2 {phase_seg2}")]
    SjwGreaterThanPhaseSeg2 { sjw: u32, phase_seg2: u32 },
    #[error(
        "bitrate error too high: {error_hundredth_percent} (1/100 percent), max {max_hundredth_percent}"
    )]
    BitrateErrorTooHigh {
        error_hundredth_percent: u32,
        max_hundredth_percent: u32,
    },
    #[error("unable to find a valid timing solution for bitrate {bitrate}")]
    NoSolution { bitrate: u32 },
}

/// Builder for CAN bitrate settings.
///
/// ## Bitrate mode
///
/// ```rust
/// use automotive::can::bitrate::{BitrateBuilder, BitTimingConst};
///
/// let btc = BitTimingConst {
///     clock_hz: 80_000_000,
///     tseg1_min: 1,
///     tseg1_max: 16,
///     tseg2_min: 1,
///     tseg2_max: 8,
///     sjw_max: 4,
///     brp_min: 1,
///     brp_max: 1024,
///     brp_inc: 1,
/// };
///
/// let cfg = BitrateBuilder::new(btc)
///     .bitrate(500_000)
///     .sample_point(0.8)
///     .build()
///     .unwrap();
///
/// assert_eq!(cfg.bitrate, 500_000);
/// assert!((cfg.sample_point - 0.8).abs() < 1e-9);
/// ```
///
/// ## Direct timing mode
///
/// ```rust
/// use automotive::can::bitrate::{BitrateBuilder, BitTimingConst};
///
/// let btc = BitTimingConst {
///     clock_hz: 80_000_000,
///     tseg1_min: 1,
///     tseg1_max: 16,
///     tseg2_min: 1,
///     tseg2_max: 8,
///     sjw_max: 4,
///     brp_min: 1,
///     brp_max: 1024,
///     brp_inc: 1,
/// };
///
/// let cfg = BitrateBuilder::new(btc)
///     .brp(8)
///     .tseg1(15)
///     .tseg2(4)
///     .sjw(1)
///     .build()
///     .unwrap();
///
/// assert_eq!(cfg.bitrate, 500_000);
/// assert!((cfg.sample_point - 0.8).abs() < 1e-9);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct BitrateBuilder {
    timing_const: BitTimingConst,
    bitrate: Option<u32>,
    sample_point: Option<f64>,
    brp: Option<u32>,
    tseg1: Option<u32>,
    tseg2: Option<u32>,
    sjw: Option<u32>,
    max_bitrate_error: u32,
}

impl BitrateBuilder {
    pub fn new(timing_const: BitTimingConst) -> Self {
        Self {
            timing_const,
            bitrate: None,
            sample_point: None,
            brp: None,
            tseg1: None,
            tseg2: None,
            sjw: None,
            max_bitrate_error: CAN_CALC_MAX_ERROR,
        }
    }

    /// Target bitrate in bits per second.
    pub fn bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Target sample point in normalized form (`0.0..1.0`).
    pub fn sample_point(mut self, sample_point: f64) -> Self {
        self.sample_point = Some(sample_point);
        self
    }

    /// Direct bit-rate prescaler.
    pub fn brp(mut self, brp: u32) -> Self {
        self.brp = Some(brp);
        self
    }

    /// Direct `tseg1` value.
    pub fn tseg1(mut self, tseg1: u32) -> Self {
        self.tseg1 = Some(tseg1);
        self
    }

    /// Direct `tseg2` value.
    pub fn tseg2(mut self, tseg2: u32) -> Self {
        self.tseg2 = Some(tseg2);
        self
    }

    /// Optional direct SJW override.
    pub fn sjw(mut self, sjw: u32) -> Self {
        self.sjw = Some(sjw);
        self
    }

    /// Maximum allowed bitrate error in one-hundredth of a percent.
    ///
    /// Default is `0.50%`
    pub fn max_bitrate_error(mut self, max_bitrate_error: u32) -> Self {
        self.max_bitrate_error = max_bitrate_error;
        self
    }

    pub fn build(self) -> Result<BitrateConfig, BitrateError> {
        if self.timing_const.clock_hz == 0 {
            return Err(BitrateError::InvalidClock);
        }
        if self.timing_const.brp_inc == 0 {
            return Err(BitrateError::InvalidBrpIncrement { brp: 0, brp_inc: 0 });
        }

        let has_bitrate_mode = self.bitrate.is_some();
        let has_direct_timing_fields = self.brp.is_some()
            || self.tseg1.is_some()
            || self.tseg2.is_some()
            || self.sjw.is_some();

        if has_bitrate_mode && has_direct_timing_fields {
            return Err(BitrateError::MixedConfiguration);
        }

        if has_bitrate_mode {
            self.build_from_bitrate_mode()
        } else {
            self.build_from_direct_mode()
        }
    }

    fn build_from_bitrate_mode(self) -> Result<BitrateConfig, BitrateError> {
        let bitrate = self.bitrate.ok_or(BitrateError::MissingConfiguration)?;
        if bitrate == 0 {
            return Err(BitrateError::InvalidBitrate);
        }

        let btc = self.timing_const;
        let sample_point_reference = if let Some(sample_point) = self.sample_point {
            sample_point_to_int(sample_point)?
        } else {
            calc_default_sample_point_nrz(bitrate)
        };

        let mut best_bitrate_error = u32::MAX;
        let mut best_sample_point_error = u32::MAX;
        let mut best_tseg = 0;
        let mut best_brp = 0;

        let max_tseg = (btc.tseg1_max + btc.tseg2_max) * 2 + 1;
        let min_tseg = (btc.tseg1_min + btc.tseg2_min) * 2;

        for tseg in (min_tseg..=max_tseg).rev() {
            let tsegall = CAN_SYNC_SEG + tseg / 2;
            let denom = (tsegall as u64) * (bitrate as u64);
            if denom == 0 {
                continue;
            }

            let mut brp = (btc.clock_hz as u64 / denom) as u32 + tseg % 2;
            brp = (brp / btc.brp_inc) * btc.brp_inc;
            if brp < btc.brp_min || brp > btc.brp_max {
                continue;
            }

            let calc_bitrate = btc.clock_hz / (brp * tsegall);
            let bitrate_error = bitrate.abs_diff(calc_bitrate);

            if bitrate_error > best_bitrate_error {
                continue;
            }

            if bitrate_error < best_bitrate_error {
                best_sample_point_error = u32::MAX;
            }

            let candidate = update_sample_point(&btc, sample_point_reference, tseg / 2);
            if candidate.sample_point_error >= best_sample_point_error {
                continue;
            }

            best_bitrate_error = bitrate_error;
            best_sample_point_error = candidate.sample_point_error;
            best_tseg = tseg / 2;
            best_brp = brp;

            if bitrate_error == 0 && candidate.sample_point_error == 0 {
                break;
            }
        }

        if best_brp == 0 {
            return Err(BitrateError::NoSolution { bitrate });
        }

        if best_bitrate_error != 0 {
            let mut bitrate_error_hundredth_percent =
                ((best_bitrate_error as u64) * 10_000 / (bitrate as u64)) as u32;
            bitrate_error_hundredth_percent = bitrate_error_hundredth_percent.max(1);

            if bitrate_error_hundredth_percent > self.max_bitrate_error {
                return Err(BitrateError::BitrateErrorTooHigh {
                    error_hundredth_percent: bitrate_error_hundredth_percent,
                    max_hundredth_percent: self.max_bitrate_error,
                });
            }
        }

        let candidate = update_sample_point(&btc, sample_point_reference, best_tseg);
        let sjw = calc_default_sjw(candidate.tseg1, candidate.tseg2);
        check_ranges(&btc, best_brp, candidate.tseg1, candidate.tseg2)?;
        check_sjw(&btc, sjw, candidate.tseg1, candidate.tseg2)?;

        let bit_time_tq = CAN_SYNC_SEG + candidate.tseg1 + candidate.tseg2;
        let actual_bitrate = btc.clock_hz / (best_brp * bit_time_tq);
        let tq_ns = ((best_brp as u64) * 1_000_000_000 / (btc.clock_hz as u64)) as u32;

        Ok(BitrateConfig {
            timing: AdapterBitTiming {
                brp: best_brp,
                tseg1: candidate.tseg1,
                tseg2: candidate.tseg2,
                sjw,
            },
            bitrate: actual_bitrate,
            sample_point: sample_point_to_float(candidate.sample_point),
            tq_ns,
        })
    }

    fn build_from_direct_mode(self) -> Result<BitrateConfig, BitrateError> {
        if self.sample_point.is_some() {
            return Err(BitrateError::SamplePointRequiresBitrate);
        }

        let has_direct_timing_fields = self.brp.is_some()
            || self.tseg1.is_some()
            || self.tseg2.is_some()
            || self.sjw.is_some();
        if !has_direct_timing_fields {
            return Err(BitrateError::MissingConfiguration);
        }

        let brp = self.brp.ok_or(BitrateError::MissingDirectField("brp"))?;
        let tseg1 = self
            .tseg1
            .ok_or(BitrateError::MissingDirectField("tseg1"))?;
        let tseg2 = self
            .tseg2
            .ok_or(BitrateError::MissingDirectField("tseg2"))?;

        let btc = self.timing_const;
        check_ranges(&btc, brp, tseg1, tseg2)?;

        let sjw = self.sjw.unwrap_or_else(|| calc_default_sjw(tseg1, tseg2));
        check_sjw(&btc, sjw, tseg1, tseg2)?;

        let bit_time_tq = CAN_SYNC_SEG + tseg1 + tseg2;
        let bitrate = btc.clock_hz / (brp * bit_time_tq);
        let sample_point = sample_point_to_float(1000 * (CAN_SYNC_SEG + tseg1) / bit_time_tq);
        let tq_ns = ((brp as u64) * 1_000_000_000 / (btc.clock_hz as u64)) as u32;

        Ok(BitrateConfig {
            timing: AdapterBitTiming {
                brp,
                tseg1,
                tseg2,
                sjw,
            },
            bitrate,
            sample_point,
            tq_ns,
        })
    }
}

fn check_ranges(
    btc: &BitTimingConst,
    brp: u32,
    tseg1: u32,
    tseg2: u32,
) -> Result<(), BitrateError> {
    if brp < btc.brp_min || brp > btc.brp_max {
        return Err(BitrateError::BrpOutOfRange {
            brp,
            min: btc.brp_min,
            max: btc.brp_max,
        });
    }
    if brp % btc.brp_inc != 0 {
        return Err(BitrateError::InvalidBrpIncrement {
            brp,
            brp_inc: btc.brp_inc,
        });
    }

    if tseg1 < btc.tseg1_min || tseg1 > btc.tseg1_max {
        return Err(BitrateError::Tseg1OutOfRange {
            tseg1,
            min: btc.tseg1_min,
            max: btc.tseg1_max,
        });
    }
    if tseg2 < btc.tseg2_min || tseg2 > btc.tseg2_max {
        return Err(BitrateError::Tseg2OutOfRange {
            tseg2,
            min: btc.tseg2_min,
            max: btc.tseg2_max,
        });
    }

    Ok(())
}

fn check_sjw(btc: &BitTimingConst, sjw: u32, tseg1: u32, tseg2: u32) -> Result<(), BitrateError> {
    let phase_seg1 = tseg1 - tseg1 / 2;
    let phase_seg2 = tseg2;

    if sjw > btc.sjw_max {
        return Err(BitrateError::SjwGreaterThanMax {
            sjw,
            max_sjw: btc.sjw_max,
        });
    }
    if sjw > phase_seg1 {
        return Err(BitrateError::SjwGreaterThanPhaseSeg1 { sjw, phase_seg1 });
    }
    if sjw > phase_seg2 {
        return Err(BitrateError::SjwGreaterThanPhaseSeg2 { sjw, phase_seg2 });
    }
    Ok(())
}

fn calc_default_sjw(tseg1: u32, tseg2: u32) -> u32 {
    let phase_seg1 = tseg1 - tseg1 / 2;
    std::cmp::max(1, std::cmp::min(phase_seg1, tseg2 / 2))
}

fn calc_default_sample_point_nrz(bitrate: u32) -> u32 {
    if bitrate > 800_000 {
        750
    } else if bitrate > 500_000 {
        800
    } else {
        875
    }
}

fn sample_point_to_int(sample_point: f64) -> Result<u32, BitrateError> {
    if !sample_point.is_finite() || !(0.0..1.0).contains(&sample_point) {
        return Err(BitrateError::InvalidSamplePoint);
    }

    Ok((sample_point * SAMPLE_POINT_SCALE) as u32)
}

fn sample_point_to_float(sample_point: u32) -> f64 {
    (sample_point as f64) / SAMPLE_POINT_SCALE
}

fn update_sample_point(
    btc: &BitTimingConst,
    sample_point_reference: u32,
    tseg: u32,
) -> SamplePointCandidate {
    let mut best_sample_point_error = u32::MAX;
    let mut best_sample_point = 0;
    let mut best_tseg1 = 0;
    let mut best_tseg2 = 0;

    for i in 0..=1 {
        let mut tseg2 =
            tseg + CAN_SYNC_SEG - (sample_point_reference * (tseg + CAN_SYNC_SEG)) / 1000 - i;
        tseg2 = tseg2.clamp(btc.tseg2_min, btc.tseg2_max);

        let mut tseg1 = tseg - tseg2;
        if tseg1 > btc.tseg1_max {
            tseg1 = btc.tseg1_max;
            tseg2 = tseg - tseg1;
        }

        let sample_point = 1000 * (tseg + CAN_SYNC_SEG - tseg2) / (tseg + CAN_SYNC_SEG);
        let sample_point_error = sample_point_reference.abs_diff(sample_point);

        if sample_point <= sample_point_reference && sample_point_error < best_sample_point_error {
            best_sample_point = sample_point;
            best_sample_point_error = sample_point_error;
            best_tseg1 = tseg1;
            best_tseg2 = tseg2;
        }
    }

    SamplePointCandidate {
        sample_point: best_sample_point,
        sample_point_error: best_sample_point_error,
        tseg1: best_tseg1,
        tseg2: best_tseg2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BTC: BitTimingConst = BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 1,
        tseg1_max: 16,
        tseg2_min: 1,
        tseg2_max: 8,
        sjw_max: 4,
        brp_min: 1,
        brp_max: 1024,
        brp_inc: 1,
    };

    #[test]
    fn bitrate_mode_500k_800() {
        let cfg = BitrateBuilder::new(BTC)
            .bitrate(500_000)
            .sample_point(0.8)
            .build()
            .unwrap();

        assert_eq!(
            cfg.timing,
            AdapterBitTiming {
                brp: 8,
                tseg1: 15,
                tseg2: 4,
                sjw: 2,
            }
        );
        assert_eq!(cfg.bitrate, 500_000);
        assert!((cfg.sample_point - 0.8).abs() < 1e-9);
    }

    #[test]
    fn bitrate_mode_default_sample_point() {
        let cfg = BitrateBuilder::new(BTC).bitrate(2_000_000).build().unwrap();

        assert_eq!(cfg.bitrate, 2_000_000);
        assert!((cfg.sample_point - 0.75).abs() < 1e-9);
    }

    #[test]
    fn direct_mode() {
        let cfg = BitrateBuilder::new(BTC)
            .brp(8)
            .tseg1(15)
            .tseg2(4)
            .build()
            .unwrap();

        assert_eq!(cfg.bitrate, 500_000);
        assert!((cfg.sample_point - 0.8).abs() < 1e-9);
        assert_eq!(cfg.timing.sjw, 2);
    }

    #[test]
    fn mixed_modes_fail() {
        let err = BitrateBuilder::new(BTC)
            .bitrate(500_000)
            .tseg1(15)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::MixedConfiguration);
    }

    #[test]
    fn invalid_sample_point_rejected() {
        let err = BitrateBuilder::new(BTC)
            .bitrate(500_000)
            .sample_point(1.0)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::InvalidSamplePoint);
    }

    #[test]
    fn round_trip_bitrate_to_direct_keeps_bitrate_and_sample_point() {
        let cfg_from_bitrate = BitrateBuilder::new(BTC)
            .bitrate(625_000)
            .sample_point(0.82)
            .build()
            .unwrap();

        let cfg_from_direct = BitrateBuilder::new(BTC)
            .brp(cfg_from_bitrate.timing.brp)
            .tseg1(cfg_from_bitrate.timing.tseg1)
            .tseg2(cfg_from_bitrate.timing.tseg2)
            .sjw(cfg_from_bitrate.timing.sjw)
            .build()
            .unwrap();

        assert_eq!(cfg_from_direct.bitrate, cfg_from_bitrate.bitrate);
        assert!((cfg_from_direct.sample_point - cfg_from_bitrate.sample_point).abs() < 1e-9);
    }
}
