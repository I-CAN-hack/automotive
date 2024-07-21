use crate::vector::error::Error;

#[derive(Clone)]
pub enum BitTimingKind {
    Standard(BitTiming),
    Extended(BitTimingFd),
}

#[derive(Clone)]
pub struct BitTiming {
    pub f_clock: u32,
    pub brp: u8,
    pub tseg1: u8,
    pub tseg2: u8,
    pub sjw: u8,
    pub nof_sample_points: u32,
    strict: bool,
}

impl BitTiming {
    pub fn new(
        f_clock: u32,
        brp: u8,
        tseg1: u8,
        tseg2: u8,
        sjw: u8,
        nof_sample_points: u32,
        strict: bool,
    ) -> Result<Self, Error> {
        if brp < 1 || brp > 64 {
            return Err(Error::BitTimingError("BRP must be between 1 and 64".to_string()));
        }

        if tseg1 < 1 || tseg1 > 16 {
            return Err(Error::BitTimingError("TSEG1 must be between 1 and 16".to_string()));
        }

        if tseg2 < 1 || tseg2 > 8 {
            return Err(Error::BitTimingError("TSEG2 must be between 1 and 8".to_string()));
        }

        if sjw < 1 || sjw > 4 {
            return Err(Error::BitTimingError("SJW must be between 1 and 4".to_string()));
        }

        if sjw > tseg2 {
            return Err(Error::BitTimingError(
                "SJW must be less than or equal to TSEG2".to_string(),
            ));
        }

        if _sample_point(tseg1, tseg2) < 50.0 {
            return Err(Error::BitTimingError(
                "Sample point must be greater than or equal to 50%".to_string(),
            ));
        }

        match nof_sample_points {
            1 | 3 => (),
            _ => {
                return Err(Error::BitTimingError(
                    "Number of sample points must be 1 or 3".to_string(),
                ))
            }
        }

        if strict {
            let nbt = _nbt(tseg1, tseg2);
            let bitrate = _bitrate(f_clock, brp, _nbt(tseg1, tseg2));

            if nbt < 8 || nbt > 25 {
                return Err(Error::BitTimingError("NBT must be between 8 and 25".to_string()));
            }

            if brp < 1 || brp > 32 {
                return Err(Error::BitTimingError("BRP must be between 1 and 32".to_string()));
            }

            if bitrate < 5_000 || bitrate > 1_000_000 {
                return Err(Error::BitTimingError(
                    "Bitrate must be between 5_000 and 1_000_000".to_string(),
                ));
            }
        }

        Ok(Self {
            f_clock,
            brp,
            tseg1,
            tseg2,
            sjw,
            nof_sample_points,
            strict,
        })
    }

    /// Bit timing register 0 for SJA1000
    pub fn btr0(&self) -> u8 {
        return (self.sjw - 1) << 6 | self.brp - 1;
    }

    /// Bit timing register 1 for SJA1000
    pub fn btr1(&self) -> u8 {
        let sam = match self.nof_sample_points {
            3 => 1,
            _ => 0,
        };

        return sam << 7 | (self.tseg2 - 1) << 4 | self.tseg1 - 1;
    }
}

#[derive(Clone)]
pub struct BitTimingFd {
    pub f_clock: u32,
    pub nom_brp: u32,
    pub nom_tseg1: u32,
    pub nom_tseg2: u32,
    pub nom_sjw: u32,
    pub data_brp: u32,
    pub data_tseg1: u32,
    pub data_tseg2: u32,
    pub data_sjw: u32,
    strict: bool,
}

impl BitTimingFd {
    pub fn new(
        f_clock: u32,
        nom_brp: u32,
        nom_tseg1: u32,
        nom_tseg2: u32,
        nom_sjw: u32,
        data_brp: u32,
        data_tseg1: u32,
        data_tseg2: u32,
        data_sjw: u32,
        strict: bool,
    ) -> Result<Self, Error> {
        if nom_brp < 1 {
            return Err(Error::BitTimingError("Nominal BRP must be at least 1".to_string()));
        }

        if data_brp < 1 {
            return Err(Error::BitTimingError("Data BRP must be at least 1".to_string()));
        }

        let nbt = _nbt_fd(nom_tseg1, nom_tseg2);
        let dbt = Self::_dbt(data_tseg1, data_tseg2);

        if Self::_data_bitrate(f_clock, data_brp, dbt) < Self::_nom_bitrate(f_clock, nom_brp, nbt) {
            return Err(Error::BitTimingError(
                "Data bitrate must be greater than or equal to nominal bitrate".to_string(),
            ));
        }

        if nom_sjw > nom_tseg2 {
            return Err(Error::BitTimingError(
                "Nominal SJW must be less than or equal to Nominal TSEG2".to_string(),
            ));
        }

        if data_sjw > data_tseg2 {
            return Err(Error::BitTimingError(
                "Data SJW must be less than or equal to Data TSEG2".to_string(),
            ));
        }

        if _sample_point_fd(nom_tseg1, nom_tseg2) < 50.0 {
            return Err(Error::BitTimingError(
                "Nominal sample point must be greater than or equal to 50%".to_string(),
            ));
        }

        if Self::_data_sample_point(data_tseg1, data_tseg2) < 50.0 {
            return Err(Error::BitTimingError(
                "Data sample point must be greater than or equal to 50%".to_string(),
            ));
        }

        if strict {
            if nbt < 8 || nbt > 80 {
                return Err(Error::BitTimingError("NBT must be between 8 and 80".to_string()));
            }

            if dbt < 5 || dbt > 25 {
                return Err(Error::BitTimingError("DBT must be between 5 and 25".to_string()));
            }

            // TODO: DO more checks based on: https://github.com/hardbyte/python-can/blob/4a41409de8e1eefaa1aa003da7e4f84f018c6791/can/bit_timing.py#L632
        }

        Ok(Self {
            f_clock,
            nom_brp,
            nom_tseg1,
            nom_tseg2,
            nom_sjw,
            data_brp,
            data_tseg1,
            data_tseg2,
            data_sjw,
            strict,
        })
    }

    pub fn nom_bitrate(&self) -> u32 {
        Self::_nom_bitrate(self.f_clock, self.nom_brp, _nbt_fd(self.nom_tseg1, self.nom_tseg2))
    }

    pub fn data_bitrate(&self) -> u32 {
        Self::_data_bitrate(
            self.f_clock,
            self.data_brp,
            Self::_dbt(self.data_tseg1, self.data_tseg2),
        )
    }

    fn _nom_bitrate(f_clock: u32, nom_brp: u32, nom_nbt: u32) -> u32 {
        _bitrate_fd(f_clock, nom_brp, nom_nbt)
    }

    fn _data_bitrate(f_clock: u32, data_brp: u32, dbt: u32) -> u32 {
        f_clock / (data_brp * dbt)
    }

    fn _data_sample_point(data_tseg1: u32, data_tseg2: u32) -> f32 {
        return 100.0 * (1 + data_tseg1) as f32 / (1 + data_tseg1 + data_tseg2) as f32;
    }

    fn _dbt(data_tseg1: u32, data_tseg2: u32) -> u32 {
        1 + data_tseg1 + data_tseg2
    }
}

/// Calculate the sample point in percent
fn _sample_point(tseg1: u8, tseg2: u8) -> f32 {
    return 100.0 * (1 + tseg1) as f32 / (1 + tseg1 + tseg2) as f32;
}

/// Calculate the sample point in percent
fn _sample_point_fd(tseg1: u32, tseg2: u32) -> f32 {
    return 100.0 * (1 + tseg1) as f32 / (1 + tseg1 + tseg2) as f32;
}

fn _nbt_fd(tseg1: u32, tseg2: u32) -> u32 {
    return 1 + tseg1 + tseg2;
}

/// Normal Bit Time
fn _nbt(tseg1: u8, tseg2: u8) -> u8 {
    return 1 + tseg1 + tseg2;
}

fn _bitrate_fd(f_clock: u32, brp: u32, nbt: u32) -> u32 {
    return f_clock / (brp * nbt) as u32;
}

fn _bitrate(f_clock: u32, brp: u8, nbt: u8) -> u32 {
    return f_clock / (brp * nbt) as u32;
}