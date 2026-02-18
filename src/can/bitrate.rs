//! Helpers for building CAN bitrate settings.
//!
//! This module provides a single builder that supports two input modes:
//! - target `bitrate` (+ optional `sample_point`)
//! - direct timing parameters (`brp`, `tseg1`, `tseg2`, optional `sjw`)
//!
//! Optionally, a CAN-FD data phase can be configured with
//! `data_bitrate` (+ optional `data_sample_point`, `data_sjw`).
//!
//! If no sample point is provided in bitrate mode, the Linux-style defaults are used:
//! - bitrate > 800_000: sample point 0.750
//! - bitrate > 500_000: sample point 0.800
//! - otherwise: sample point 0.875
//!
//! The resulting [`BitrateConfig`] contains:
//! - the adapter-facing values (`brp`, `tseg1`, `tseg2`, `sjw`)
//! - the resulting `bitrate` and `sample_point`

use thiserror::Error;

const CAN_SYNC_SEG: u32 = 1;
const CAN_CALC_MAX_ERROR: u32 = 50; // 0.50% in one-hundredth percent units
const SAMPLE_POINT_SCALE: f64 = 1000.0;
const DEFAULT_SAMPLE_POINT_HIGH_BITRATE_THRESHOLD: u32 = 800_000;
const DEFAULT_SAMPLE_POINT_MEDIUM_BITRATE_THRESHOLD: u32 = 500_000;
const DEFAULT_SAMPLE_POINT_HIGH_BITRATE: u32 = 750;
const DEFAULT_SAMPLE_POINT_MEDIUM_BITRATE: u32 = 800;
const DEFAULT_SAMPLE_POINT_LOW_BITRATE: u32 = 875;

/// Hardware limits used to calculate and validate CAN bit timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BitTimingConst {
    /// CAN controller input clock in Hz.
    pub clock_hz: u32,
    /// Minimum value of `tseg1`.
    pub tseg1_min: u32,
    /// Maximum value of `tseg1`.
    pub tseg1_max: u32,
    /// Minimum value of `tseg2`.
    pub tseg2_min: u32,
    /// Maximum value of `tseg2`.
    pub tseg2_max: u32,
    /// Maximum supported SJW.
    pub sjw_max: u32,
    /// Minimum bitrate prescaler.
    pub brp_min: u32,
    /// Maximum bitrate prescaler.
    pub brp_max: u32,
    /// Prescaler increment step.
    pub brp_inc: u32,
}

/// Adapter timing constants for nominal CAN and optional CAN-FD data phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdapterTimingConst {
    /// Nominal/arbitration phase timing limits.
    pub nominal: BitTimingConst,
    /// Optional CAN-FD data phase timing limits.
    ///
    /// If this is `None`, the adapter does not support setting a data bitrate.
    pub data: Option<BitTimingConst>,
}

/// Generic timing values typically needed by CAN adapter drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AdapterBitTiming {
    /// Bitrate prescaler.
    pub brp: u32,
    /// Time segment 1 (`prop_seg + phase_seg1`).
    pub tseg1: u32,
    /// Time segment 2 (`phase_seg2`).
    pub tseg2: u32,
    /// Synchronization jump width.
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
    /// Optional CAN-FD data phase adapter-facing timing values.
    pub data_timing: Option<AdapterBitTiming>,
    /// Optional CAN-FD data phase bitrate in bits per second.
    pub data_bitrate: Option<u32>,
    /// Optional CAN-FD data phase sample point in normalized form (`0.0..1.0`).
    pub data_sample_point: Option<f64>,
}

impl BitrateConfig {
    /// Duration of one bit in time quanta.
    pub fn bit_time_tq(&self) -> u32 {
        CAN_SYNC_SEG + self.timing.tseg1 + self.timing.tseg2
    }

    /// Duration of one CAN-FD data phase bit in time quanta.
    pub fn data_bit_time_tq(&self) -> Option<u32> {
        self.data_timing
            .map(|timing| CAN_SYNC_SEG + timing.tseg1 + timing.tseg2)
    }
}

#[derive(Debug, Clone, Copy)]
struct SamplePointCandidate {
    sample_point: u32,
    sample_point_error: u32,
    tseg1: u32,
    tseg2: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PhaseBitrateConfig {
    timing: AdapterBitTiming,
    bitrate: u32,
    sample_point: f64,
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
    #[error("data_sample_point can only be used with data_bitrate")]
    DataSamplePointRequiresDataBitrate,
    #[error("data_sjw can only be used with data_bitrate")]
    DataSjwRequiresDataBitrate,
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
    #[error(
        "CAN-FD data bitrate {data_bitrate} must be >= arbitration bitrate {arbitration_bitrate}"
    )]
    DataBitrateLowerThanNominal {
        data_bitrate: u32,
        arbitration_bitrate: u32,
    },
    #[error("CAN-FD data bitrate requested, but adapter does not provide CAN-FD timing constants")]
    DataBitrateNotSupported,
}

/// Builder for CAN bitrate settings.
///
/// ## Bitrate mode
///
/// ```rust
/// use automotive::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateBuilder};
/// use automotive::can::{CanAdapter, Frame};
/// use std::collections::VecDeque;
///
/// const TIMING: AdapterTimingConst = AdapterTimingConst {
///     nominal: BitTimingConst {
///         clock_hz: 80_000_000,
///         tseg1_min: 1,
///         tseg1_max: 16,
///         tseg2_min: 1,
///         tseg2_max: 8,
///         sjw_max: 4,
///         brp_min: 1,
///         brp_max: 1024,
///         brp_inc: 1,
///     },
///     data: None,
/// };
///
/// struct DummyAdapter;
/// impl CanAdapter for DummyAdapter {
///     fn send(&mut self, _frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
///         unreachable!()
///     }
///
///     fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
///         unreachable!()
///     }
///
///     fn timing_const() -> AdapterTimingConst
///     where
///         Self: Sized,
///     {
///         TIMING
///     }
/// }
///
/// let cfg = BitrateBuilder::new::<DummyAdapter>()
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
/// use automotive::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateBuilder};
/// use automotive::can::{CanAdapter, Frame};
/// use std::collections::VecDeque;
///
/// const TIMING: AdapterTimingConst = AdapterTimingConst {
///     nominal: BitTimingConst {
///         clock_hz: 80_000_000,
///         tseg1_min: 1,
///         tseg1_max: 16,
///         tseg2_min: 1,
///         tseg2_max: 8,
///         sjw_max: 4,
///         brp_min: 1,
///         brp_max: 1024,
///         brp_inc: 1,
///     },
///     data: None,
/// };
///
/// struct DummyAdapter;
/// impl CanAdapter for DummyAdapter {
///     fn send(&mut self, _frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
///         unreachable!()
///     }
///
///     fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
///         unreachable!()
///     }
///
///     fn timing_const() -> AdapterTimingConst
///     where
///         Self: Sized,
///     {
///         TIMING
///     }
/// }
///
/// let cfg = BitrateBuilder::new::<DummyAdapter>()
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
///
/// ## CAN-FD data phase
///
/// ```rust
/// use automotive::can::bitrate::{AdapterTimingConst, BitTimingConst, BitrateBuilder};
/// use automotive::can::{CanAdapter, Frame};
/// use std::collections::VecDeque;
///
/// const TIMING: AdapterTimingConst = AdapterTimingConst {
///     nominal: BitTimingConst {
///         clock_hz: 80_000_000,
///         tseg1_min: 1,
///         tseg1_max: 256,
///         tseg2_min: 1,
///         tseg2_max: 128,
///         sjw_max: 128,
///         brp_min: 1,
///         brp_max: 1024,
///         brp_inc: 1,
///     },
///     data: Some(BitTimingConst {
///         clock_hz: 80_000_000,
///         tseg1_min: 1,
///         tseg1_max: 32,
///         tseg2_min: 1,
///         tseg2_max: 16,
///         sjw_max: 16,
///         brp_min: 1,
///         brp_max: 1024,
///         brp_inc: 1,
///     }),
/// };
///
/// struct DummyAdapter;
/// impl CanAdapter for DummyAdapter {
///     fn send(&mut self, _frames: &mut VecDeque<Frame>) -> automotive::Result<()> {
///         unreachable!()
///     }
///
///     fn recv(&mut self) -> automotive::Result<Vec<Frame>> {
///         unreachable!()
///     }
///
///     fn timing_const() -> AdapterTimingConst
///     where
///         Self: Sized,
///     {
///         TIMING
///     }
/// }
///
/// let cfg = BitrateBuilder::new::<DummyAdapter>()
///     .bitrate(500_000)
///     .data_bitrate(2_000_000)
///     .build()
///     .unwrap();
///
/// assert_eq!(cfg.data_bitrate, Some(2_000_000));
/// ```
#[derive(Debug, Clone, Copy)]
pub struct BitrateBuilder {
    timing_const: AdapterTimingConst,
    bitrate: Option<u32>,
    sample_point: Option<f64>,
    brp: Option<u32>,
    tseg1: Option<u32>,
    tseg2: Option<u32>,
    sjw: Option<u32>,
    data_bitrate: Option<u32>,
    data_sample_point: Option<f64>,
    data_sjw: Option<u32>,
    max_bitrate_error: u32,
}

impl BitrateBuilder {
    /// Create a builder using static timing metadata from a blocking CAN adapter type.
    ///
    /// This does not require constructing an adapter instance.
    pub fn new<T: crate::can::CanAdapter>() -> Self {
        Self::with_timing_const(T::timing_const())
    }

    fn with_timing_const(timing_const: AdapterTimingConst) -> Self {
        Self {
            timing_const,
            bitrate: None,
            sample_point: None,
            brp: None,
            tseg1: None,
            tseg2: None,
            sjw: None,
            data_bitrate: None,
            data_sample_point: None,
            data_sjw: None,
            max_bitrate_error: CAN_CALC_MAX_ERROR,
        }
    }

    /// Target bitrate in bits per second.
    pub fn bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = Some(bitrate);
        self
    }

    /// Target sample point in normalized form (`0.0..1.0`).
    ///
    /// If omitted, the default depends on bitrate:
    /// - `bitrate > 800_000`: `0.750`
    /// - `bitrate > 500_000`: `0.800`
    /// - otherwise: `0.875`
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

    /// Optional SJW override for nominal phase.
    ///
    /// This can be used both in bitrate mode and direct timing mode.
    /// If omitted, SJW is derived using the Linux default heuristic:
    /// `max(1, min(phase_seg1, phase_seg2 / 2))`.
    pub fn sjw(mut self, sjw: u32) -> Self {
        self.sjw = Some(sjw);
        self
    }

    /// Optional CAN-FD data phase target bitrate in bits per second.
    ///
    /// Requires [`AdapterTimingConst::data`] to be present.
    pub fn data_bitrate(mut self, bitrate: u32) -> Self {
        self.data_bitrate = Some(bitrate);
        self
    }

    /// Optional CAN-FD data phase sample point in normalized form (`0.0..1.0`).
    ///
    /// If omitted, the default depends on `data_bitrate`:
    /// - `data_bitrate > 800_000`: `0.750`
    /// - `data_bitrate > 500_000`: `0.800`
    /// - otherwise: `0.875`
    pub fn data_sample_point(mut self, sample_point: f64) -> Self {
        self.data_sample_point = Some(sample_point);
        self
    }

    /// Optional CAN-FD data phase SJW override.
    ///
    /// If omitted, the same Linux default heuristic is used for the data phase:
    /// `max(1, min(phase_seg1, phase_seg2 / 2))`, with `phase_seg*` derived
    /// from the resolved data-phase `tseg1`/`tseg2`.
    pub fn data_sjw(mut self, sjw: u32) -> Self {
        self.data_sjw = Some(sjw);
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
        validate_timing_const(&self.timing_const.nominal)?;

        if self.data_sample_point.is_some() && self.data_bitrate.is_none() {
            return Err(BitrateError::DataSamplePointRequiresDataBitrate);
        }
        if self.data_sjw.is_some() && self.data_bitrate.is_none() {
            return Err(BitrateError::DataSjwRequiresDataBitrate);
        }

        let has_bitrate_mode = self.bitrate.is_some();
        let has_direct_timing_fields =
            self.brp.is_some() || self.tseg1.is_some() || self.tseg2.is_some();

        if has_bitrate_mode && has_direct_timing_fields {
            return Err(BitrateError::MixedConfiguration);
        }

        let nominal = if has_bitrate_mode {
            self.build_from_bitrate_mode()?
        } else {
            self.build_from_direct_mode()?
        };

        let (data_timing, data_bitrate, data_sample_point) =
            if let Some(data_bitrate_target) = self.data_bitrate {
                let data_timing_const = self
                    .timing_const
                    .data
                    .ok_or(BitrateError::DataBitrateNotSupported)?;
                validate_timing_const(&data_timing_const)?;

                let data = solve_bitrate_mode(
                    &data_timing_const,
                    data_bitrate_target,
                    self.data_sample_point,
                    self.data_sjw,
                    self.max_bitrate_error,
                )?;

                if data.bitrate < nominal.bitrate {
                    return Err(BitrateError::DataBitrateLowerThanNominal {
                        data_bitrate: data.bitrate,
                        arbitration_bitrate: nominal.bitrate,
                    });
                }

                (
                    Some(data.timing),
                    Some(data.bitrate),
                    Some(data.sample_point),
                )
            } else {
                (None, None, None)
            };

        Ok(BitrateConfig {
            timing: nominal.timing,
            bitrate: nominal.bitrate,
            sample_point: nominal.sample_point,
            data_timing,
            data_bitrate,
            data_sample_point,
        })
    }

    fn build_from_bitrate_mode(self) -> Result<PhaseBitrateConfig, BitrateError> {
        let bitrate = self.bitrate.ok_or(BitrateError::MissingConfiguration)?;

        solve_bitrate_mode(
            &self.timing_const.nominal,
            bitrate,
            self.sample_point,
            self.sjw,
            self.max_bitrate_error,
        )
    }

    fn build_from_direct_mode(self) -> Result<PhaseBitrateConfig, BitrateError> {
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

        solve_direct_mode(&self.timing_const.nominal, brp, tseg1, tseg2, self.sjw)
    }
}

fn validate_timing_const(btc: &BitTimingConst) -> Result<(), BitrateError> {
    if btc.clock_hz == 0 {
        return Err(BitrateError::InvalidClock);
    }
    if btc.brp_inc == 0 {
        return Err(BitrateError::InvalidBrpIncrement { brp: 0, brp_inc: 0 });
    }
    Ok(())
}

fn solve_bitrate_mode(
    btc: &BitTimingConst,
    bitrate: u32,
    sample_point: Option<f64>,
    sjw: Option<u32>,
    max_bitrate_error: u32,
) -> Result<PhaseBitrateConfig, BitrateError> {
    if bitrate == 0 {
        return Err(BitrateError::InvalidBitrate);
    }

    let sample_point_reference = if let Some(sample_point) = sample_point {
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

        let candidate = update_sample_point(btc, sample_point_reference, tseg / 2);
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

        if bitrate_error_hundredth_percent > max_bitrate_error {
            return Err(BitrateError::BitrateErrorTooHigh {
                error_hundredth_percent: bitrate_error_hundredth_percent,
                max_hundredth_percent: max_bitrate_error,
            });
        }
    }

    let candidate = update_sample_point(btc, sample_point_reference, best_tseg);
    let sjw = sjw.unwrap_or_else(|| calc_default_sjw(candidate.tseg1, candidate.tseg2));
    check_ranges(btc, best_brp, candidate.tseg1, candidate.tseg2)?;
    check_sjw(btc, sjw, candidate.tseg1, candidate.tseg2)?;

    let bit_time_tq = CAN_SYNC_SEG + candidate.tseg1 + candidate.tseg2;
    let actual_bitrate = btc.clock_hz / (best_brp * bit_time_tq);
    Ok(PhaseBitrateConfig {
        timing: AdapterBitTiming {
            brp: best_brp,
            tseg1: candidate.tseg1,
            tseg2: candidate.tseg2,
            sjw,
        },
        bitrate: actual_bitrate,
        sample_point: sample_point_to_float(candidate.sample_point),
    })
}

fn solve_direct_mode(
    btc: &BitTimingConst,
    brp: u32,
    tseg1: u32,
    tseg2: u32,
    sjw: Option<u32>,
) -> Result<PhaseBitrateConfig, BitrateError> {
    check_ranges(btc, brp, tseg1, tseg2)?;

    let sjw = sjw.unwrap_or_else(|| calc_default_sjw(tseg1, tseg2));
    check_sjw(btc, sjw, tseg1, tseg2)?;

    let bit_time_tq = CAN_SYNC_SEG + tseg1 + tseg2;
    let bitrate = btc.clock_hz / (brp * bit_time_tq);
    let sample_point = sample_point_to_float(1000 * (CAN_SYNC_SEG + tseg1) / bit_time_tq);
    Ok(PhaseBitrateConfig {
        timing: AdapterBitTiming {
            brp,
            tseg1,
            tseg2,
            sjw,
        },
        bitrate,
        sample_point,
    })
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
    if bitrate > DEFAULT_SAMPLE_POINT_HIGH_BITRATE_THRESHOLD {
        DEFAULT_SAMPLE_POINT_HIGH_BITRATE
    } else if bitrate > DEFAULT_SAMPLE_POINT_MEDIUM_BITRATE_THRESHOLD {
        DEFAULT_SAMPLE_POINT_MEDIUM_BITRATE
    } else {
        DEFAULT_SAMPLE_POINT_LOW_BITRATE
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
    use crate::can::{CanAdapter, Frame};
    use std::collections::VecDeque;

    const PEAK_NOMINAL_BTC: BitTimingConst = BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 1,
        tseg1_max: 1 << 8,
        tseg2_min: 1,
        tseg2_max: 1 << 7,
        sjw_max: 1 << 7,
        brp_min: 1,
        brp_max: 1 << 10,
        brp_inc: 1,
    };

    const PEAK_FD_DATA_BTC: BitTimingConst = BitTimingConst {
        clock_hz: 80_000_000,
        tseg1_min: 1,
        tseg1_max: 1 << 5,
        tseg2_min: 1,
        tseg2_max: 1 << 4,
        sjw_max: 1 << 4,
        brp_min: 1,
        brp_max: 1 << 10,
        brp_inc: 1,
    };

    const PEAK_TIMING_WITH_FD: AdapterTimingConst = AdapterTimingConst {
        nominal: PEAK_NOMINAL_BTC,
        data: Some(PEAK_FD_DATA_BTC),
    };

    const PEAK_TIMING_NO_FD: AdapterTimingConst = AdapterTimingConst {
        nominal: PEAK_NOMINAL_BTC,
        data: None,
    };

    struct DummyTimingAdapter;
    impl CanAdapter for DummyTimingAdapter {
        fn send(&mut self, _frames: &mut VecDeque<Frame>) -> crate::Result<()> {
            unreachable!()
        }

        fn recv(&mut self) -> crate::Result<Vec<Frame>> {
            unreachable!()
        }

        fn timing_const() -> AdapterTimingConst
        where
            Self: Sized,
        {
            PEAK_TIMING_WITH_FD
        }
    }

    struct DummyNoFdTimingAdapter;
    impl CanAdapter for DummyNoFdTimingAdapter {
        fn send(&mut self, _frames: &mut VecDeque<Frame>) -> crate::Result<()> {
            unreachable!()
        }

        fn recv(&mut self) -> crate::Result<Vec<Frame>> {
            unreachable!()
        }

        fn timing_const() -> AdapterTimingConst
        where
            Self: Sized,
        {
            PEAK_TIMING_NO_FD
        }
    }

    #[test]
    fn bitrate_mode_500k_800() {
        let cfg = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(0.8)
            .build()
            .unwrap();

        assert_eq!(cfg.bitrate, 500_000);
        assert!((cfg.sample_point - 0.8).abs() < 1e-9);
        assert!(cfg.timing.brp >= PEAK_NOMINAL_BTC.brp_min);
        assert!(cfg.timing.brp <= PEAK_NOMINAL_BTC.brp_max);
    }

    #[test]
    fn from_adapter_type_constructor() {
        let cfg = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(0.8)
            .build()
            .unwrap();

        assert_eq!(cfg.bitrate, 500_000);
    }

    #[test]
    fn bitrate_mode_default_sample_point() {
        let cfg_high_default = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(2_000_000)
            .build()
            .unwrap();
        let cfg_high_explicit = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(2_000_000)
            .sample_point(0.75)
            .build()
            .unwrap();

        let cfg_medium_default = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(625_000)
            .build()
            .unwrap();
        let cfg_medium_explicit = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(625_000)
            .sample_point(0.8)
            .build()
            .unwrap();

        let cfg_low_default = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .build()
            .unwrap();
        let cfg_low_explicit = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(0.875)
            .build()
            .unwrap();

        assert_eq!(cfg_high_default, cfg_high_explicit);
        assert_eq!(cfg_medium_default, cfg_medium_explicit);
        assert_eq!(cfg_low_default, cfg_low_explicit);
    }

    #[test]
    fn bitrate_mode_allows_sjw_override() {
        let cfg = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(0.8)
            .sjw(1)
            .build()
            .unwrap();

        assert_eq!(cfg.timing.sjw, 1);
    }

    #[test]
    fn direct_mode() {
        let cfg = BitrateBuilder::new::<DummyTimingAdapter>()
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
        let err = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .tseg1(15)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::MixedConfiguration);
    }

    #[test]
    fn invalid_sample_point_rejected() {
        let err = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(1.0)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::InvalidSamplePoint);
    }

    #[test]
    fn can_fd_data_phase_bitrate_and_sample_point() {
        let cfg = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .sample_point(0.8)
            .data_bitrate(2_000_000)
            .data_sample_point(0.75)
            .build()
            .unwrap();

        assert_eq!(cfg.bitrate, 500_000);
        assert!((cfg.sample_point - 0.8).abs() < 1e-9);

        assert_eq!(cfg.data_bitrate, Some(2_000_000));
        assert!(cfg.data_timing.is_some());
        assert!((cfg.data_sample_point.unwrap() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn data_sample_point_requires_data_bitrate() {
        let err = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .data_sample_point(0.75)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::DataSamplePointRequiresDataBitrate);
    }

    #[test]
    fn data_sjw_requires_data_bitrate() {
        let err = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .data_sjw(1)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::DataSjwRequiresDataBitrate);
    }

    #[test]
    fn data_bitrate_must_not_be_lower_than_nominal() {
        let err = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(500_000)
            .data_bitrate(250_000)
            .build()
            .unwrap_err();

        match err {
            BitrateError::DataBitrateLowerThanNominal {
                data_bitrate,
                arbitration_bitrate,
            } => {
                assert_eq!(arbitration_bitrate, 500_000);
                assert!(data_bitrate < arbitration_bitrate);
            }
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn data_bitrate_not_supported_by_adapter() {
        let err = BitrateBuilder::new::<DummyNoFdTimingAdapter>()
            .bitrate(500_000)
            .data_bitrate(2_000_000)
            .build()
            .unwrap_err();

        assert_eq!(err, BitrateError::DataBitrateNotSupported);
    }

    #[test]
    fn round_trip_bitrate_to_direct_keeps_bitrate_and_sample_point() {
        let cfg_from_bitrate = BitrateBuilder::new::<DummyTimingAdapter>()
            .bitrate(625_000)
            .sample_point(0.82)
            .build()
            .unwrap();

        let cfg_from_direct = BitrateBuilder::new::<DummyTimingAdapter>()
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
