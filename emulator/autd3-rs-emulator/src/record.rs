#![allow(clippy::cast_possible_truncation)]

use autd3_rs_core::common::ULTRASOUND_PERIOD;
use autd3_rs_core::geometry::Point3;
use autd3_rs_core::params::ULTRASOUND_FREQ_HZ;
use autd3_rs_core::value::{DcSysTime, PULSE_WIDTH_PERIOD};

pub(crate) const ULTRASOUND_PERIOD_COUNT: usize = PULSE_WIDTH_PERIOD as usize;
pub(crate) const OUTPUT_VOLTAGE: f32 = 12.0;
pub(crate) const TS: f32 = 1.0 / (ULTRASOUND_FREQ_HZ as f32 * ULTRASOUND_PERIOD_COUNT as f32);
pub(crate) const T4010A1_AMPLITUDE: f32 = 275.574_25 * 200.0;

#[cfg(feature = "polars")]
use polars::frame::DataFrame;
#[cfg(feature = "polars")]
use polars::prelude::Column;

pub(crate) struct TransducerRecord {
    pub(crate) pulse_width: Vec<u16>,
    pub(crate) phase: Vec<u8>,
    pub(crate) position: Point3<f32>,
}

pub struct Record {
    pub(crate) records: Vec<TransducerRecord>,
    pub(crate) aabb: crate::aabb::Aabb,
    start_ns: u64,
    end_ns: u64,
}

fn sample_time_ns(col: usize) -> u64 {
    col as u64 * ULTRASOUND_PERIOD.as_nanos() as u64
}

#[cfg(feature = "polars")]
pub(crate) fn output_df(label: &str, rows: usize, per_tr: &[Vec<f32>]) -> DataFrame {
    let cols = per_tr.first().map_or(0, Vec::len);
    let columns = (0..cols)
        .map(|c| {
            let data: Vec<f32> = (0..rows).map(|r| per_tr[r][c]).collect();
            Column::new(format!("{label}@{c}[25us/512]").into(), data.as_slice())
        })
        .collect::<Vec<_>>();
    DataFrame::new(rows, columns).unwrap()
}

impl Record {
    pub(crate) fn new(records: Vec<TransducerRecord>, start_ns: u64, end_ns: u64) -> Self {
        let aabb = crate::aabb::Aabb::from_points(records.iter().map(|tr| tr.position));
        Self {
            records,
            aabb,
            start_ns,
            end_ns,
        }
    }

    pub(crate) fn start_ns(&self) -> u64 {
        self.start_ns
    }

    pub(crate) fn end_ns(&self) -> u64 {
        self.end_ns
    }

    #[must_use]
    pub fn start(&self) -> DcSysTime {
        DcSysTime::from_nanos(self.start_ns)
    }

    #[must_use]
    pub fn end(&self) -> DcSysTime {
        DcSysTime::from_nanos(self.end_ns)
    }

    #[must_use]
    pub fn num_transducers(&self) -> usize {
        self.records.len()
    }

    #[must_use]
    pub fn num_samples(&self) -> usize {
        self.records.first().map_or(0, |r| r.pulse_width.len())
    }

    #[must_use]
    pub fn phase_of(&self, transducer: usize) -> &[u8] {
        &self.records[transducer].phase
    }

    #[must_use]
    pub fn pulse_width_of(&self, transducer: usize) -> &[u16] {
        &self.records[transducer].pulse_width
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn phase(&self) -> DataFrame {
        let rows = self.num_transducers();
        let columns = (0..self.num_samples())
            .map(|col| {
                let t = sample_time_ns(col);
                let data: Vec<u8> = (0..rows).map(|row| self.records[row].phase[col]).collect();
                Column::new(format!("phase@{t}[ns]").into(), data.as_slice())
            })
            .collect::<Vec<_>>();
        DataFrame::new(rows, columns).unwrap()
    }

    #[cfg(feature = "polars")]
    #[must_use]
    pub fn pulse_width(&self) -> DataFrame {
        let rows = self.num_transducers();
        let columns = (0..self.num_samples())
            .map(|col| {
                let t = sample_time_ns(col);
                let data: Vec<u16> = (0..rows)
                    .map(|row| self.records[row].pulse_width[col])
                    .collect();
                Column::new(format!("pulse_width@{t}[ns]").into(), data.as_slice())
            })
            .collect::<Vec<_>>();
        DataFrame::new(rows, columns).unwrap()
    }
}
