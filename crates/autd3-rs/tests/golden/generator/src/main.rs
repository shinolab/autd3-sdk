use std::fmt::Write as _;
use std::num::NonZeroU16;
use std::time::Duration;

use autd3::core::datagram::{Datagram, DeviceMask};
use autd3::core::devices::AUTD3;
use autd3::core::environment::Environment;
use autd3::core::ethercat::DcSysTime;
use autd3::core::firmware::transition_mode::{Ext, GPIO, Immediate, Later, SyncIdx, SysTime};
use autd3::core::firmware::{
    Drive, GPIOIn, GPIOOut, Intensity, Phase, PulseWidth, SamplingConfig, Segment,
};
use autd3::core::geometry::{Device, Point3, Transducer, UnitQuaternion};
use autd3::core::link::{MsgId, TxMessage};
use autd3::driver::datagram::{
    Clear, EmulateGPIOIn, FirmwareVersionType, FixedCompletionSteps, FixedCompletionTime,
    FixedUpdateRate, FociSTM, ForceFan, GPIOOutputType, GPIOOutputs, GainSTM, GainSTMMode,
    GainSTMOption, Nop, OutputMask, PhaseCorrection, PulseWidthEncoder, ReadsFPGAState, Silencer,
    SwapSegmentFociSTM, SwapSegmentGain, SwapSegmentGainSTM, SwapSegmentModulation, Synchronize,
    WithFiniteLoop, WithSegment,
};
use autd3::driver::datagram::{ControlPoint, ControlPoints};
use autd3::driver::error::AUTDDriverError;
use autd3::driver::firmware::operation::{Operation, OperationGenerator, OperationHandler};
use autd3::driver::geometry::Geometry;
use autd3::gain::Custom as CustomGain;
use autd3::modulation::Custom as CustomModulation;

const FRAME_BYTES: usize = 626;

fn geometry() -> Geometry {
    Geometry::new(
        (0..2)
            .map(|i| {
                AUTD3::new(
                    Point3::new(200.0 * i as f32, 0.0, 0.0),
                    UnitQuaternion::identity(),
                )
                .into()
            })
            .collect(),
    )
}

fn drive(i: usize) -> Drive {
    let i = (i % 256) as u8;
    Drive {
        phase: Phase(i),
        intensity: Intensity(255 - i),
    }
}

fn divide(v: u16) -> SamplingConfig {
    SamplingConfig::new(NonZeroU16::new(v).unwrap())
}

fn frame_bytes(tx: &TxMessage) -> [u8; FRAME_BYTES] {
    assert_eq!(FRAME_BYTES, size_of::<TxMessage>());
    // SAFETY: `TxMessage` is `#[repr(C)]` over a 4-byte header and a `[u16; 311]` payload,
    // so it is exactly `FRAME_BYTES` bytes of initialized memory with no padding.
    let raw = unsafe {
        std::slice::from_raw_parts(
            (tx as *const TxMessage).cast::<u8>(),
            size_of::<TxMessage>(),
        )
    };
    let mut out = [0u8; FRAME_BYTES];
    out.copy_from_slice(raw);
    out[0] = 0;
    out
}

fn emit<'a, D>(out: &mut String, geometry: &'a Geometry, case: &str, d: D)
where
    D: Datagram<'a>,
    AUTDDriverError: From<D::Error>,
    D::G: OperationGenerator<'a>,
    AUTDDriverError: From<<<D::G as OperationGenerator<'a>>::O1 as Operation<'a>>::Error>
        + From<<<D::G as OperationGenerator<'a>>::O2 as Operation<'a>>::Error>,
{
    let env = Environment::new();
    let mut g = d
        .operation_generator(geometry, &env, &DeviceMask::AllEnabled)
        .unwrap_or_else(|e| panic!("{case}: {}", AUTDDriverError::from(e)));
    let mut ops = geometry
        .iter()
        .map(|dev| g.generate(dev))
        .collect::<Vec<_>>();

    let mut round = 0usize;
    let mut msg_id = MsgId::new(0);
    while !OperationHandler::is_done(&ops) {
        let mut tx = vec![TxMessage::new(); geometry.num_devices()];
        msg_id.increment();
        OperationHandler::pack(msg_id, &mut ops, geometry, &mut tx, false)
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        for (device, tx) in tx.iter().enumerate() {
            write!(out, "{case}\t{round}\t{device}\t").unwrap();
            for b in frame_bytes(tx) {
                write!(out, "{b:02x}").unwrap();
            }
            out.push('\n');
        }
        round += 1;
    }
}

macro_rules! gain {
    ($offset:expr) => {{
        let offset: usize = $offset;
        CustomGain::new(move |_dev: &Device| move |tr: &Transducer| drive(tr.idx() + offset))
    }};
}

fn foci_single() -> Vec<ControlPoints<1>> {
    (0..300)
        .map(|i| {
            ControlPoints::new(
                [ControlPoint::new(
                    Point3::new(100.0, 60.0, 140.0 + i as f32 * 0.1),
                    Phase::ZERO,
                )],
                Intensity(0x90),
            )
        })
        .collect()
}

fn foci_dual() -> Vec<ControlPoints<2>> {
    (0..120)
        .map(|i| {
            ControlPoints::new(
                [
                    ControlPoint::new(Point3::new(90.0, 60.0, 140.0 + i as f32), Phase::ZERO),
                    ControlPoint::new(Point3::new(110.0, 60.0, 140.0 + i as f32), Phase(0x40)),
                ],
                Intensity(0x80),
            )
        })
        .collect()
}

fn foci_octuple() -> Vec<ControlPoints<8>> {
    (0..12)
        .map(|i| {
            ControlPoints::new(
                std::array::from_fn(|j| {
                    ControlPoint::new(
                        Point3::new(
                            80.0 + j as f32 * 5.0,
                            60.0 + i as f32,
                            150.0 + (i * 8 + j) as f32 * 0.25,
                        ),
                        Phase((j * 16) as u8),
                    )
                }),
                Intensity(0x70),
            )
        })
        .collect()
}

fn modulation(len: usize) -> CustomModulation<SamplingConfig> {
    CustomModulation::new(
        (0..len).map(|i| drive(i).phase.0).collect::<Vec<_>>(),
        divide(10),
    )
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../legacy_v38_pack.tsv".to_owned());
    let geometry = geometry();
    let g = &geometry;
    let mut out = String::new();
    let o = &mut out;

    emit(o, g, "nop", Nop);
    emit(o, g, "clear", Clear::new());
    emit(o, g, "sync", Synchronize::new());
    emit(o, g, "force_fan_on", ForceFan::new(|_dev| true));
    emit(o, g, "force_fan_off", ForceFan::new(|_dev| false));
    emit(
        o,
        g,
        "reads_fpga_state_on",
        ReadsFPGAState::new(|_dev| true),
    );
    emit(
        o,
        g,
        "reads_fpga_state_off",
        ReadsFPGAState::new(|_dev| false),
    );

    emit(o, g, "info_cpu_major", FirmwareVersionType::CPUMajor);
    emit(o, g, "info_cpu_minor", FirmwareVersionType::CPUMinor);
    emit(o, g, "info_fpga_major", FirmwareVersionType::FPGAMajor);
    emit(o, g, "info_fpga_minor", FirmwareVersionType::FPGAMinor);
    emit(
        o,
        g,
        "info_fpga_functions",
        FirmwareVersionType::FPGAFunctions,
    );
    emit(o, g, "info_clear", FirmwareVersionType::Clear);

    let nz = |v| NonZeroU16::new(v).unwrap();
    emit(
        o,
        g,
        "silencer_steps",
        Silencer {
            config: FixedCompletionSteps {
                intensity: nz(0x12),
                phase: nz(0x34),
                strict: true,
            },
        },
    );
    emit(
        o,
        g,
        "silencer_steps_loose",
        Silencer {
            config: FixedCompletionSteps {
                strict: false,
                ..Default::default()
            },
        },
    );
    emit(
        o,
        g,
        "silencer_rate",
        Silencer {
            config: FixedUpdateRate {
                intensity: nz(0x1234),
                phase: nz(0x5678),
            },
        },
    );
    emit(
        o,
        g,
        "silencer_time",
        Silencer {
            config: FixedCompletionTime {
                intensity: Duration::from_micros(250),
                phase: Duration::from_millis(1),
                strict: true,
            },
        },
    );

    emit(o, g, "gain", gain!(0));
    emit(
        o,
        g,
        "gain_s1_later",
        WithSegment::new(gain!(0), Segment::S1, Later),
    );

    emit(o, g, "modulation_short", modulation(100));
    emit(o, g, "modulation_long", modulation(3000));
    emit(o, g, "modulation_min", modulation(2));
    emit(o, g, "modulation_odd", modulation(101));
    emit(o, g, "modulation_head_full", modulation(254));
    emit(o, g, "modulation_head_plus_one", modulation(255));
    emit(o, g, "modulation_frame_boundary", modulation(254 + 618));
    emit(
        o,
        g,
        "modulation_s1_finite",
        WithFiniteLoop::new(modulation(100), nz(5), Segment::S1, SyncIdx),
    );
    emit(
        o,
        g,
        "modulation_s1_later",
        WithSegment::new(modulation(100), Segment::S1, Later),
    );
    emit(
        o,
        g,
        "modulation_s1_ext",
        WithSegment::new(modulation(100), Segment::S1, Ext),
    );

    emit(
        o,
        g,
        "foci_stm_1",
        FociSTM::new(foci_single(), divide(0xFFFF)),
    );
    emit(
        o,
        g,
        "foci_stm_s1_later",
        WithSegment::new(
            FociSTM::new(foci_single(), divide(0xFFFF)),
            Segment::S1,
            Later,
        ),
    );
    emit(
        o,
        g,
        "foci_stm_s1_sys_time",
        WithFiniteLoop::new(
            FociSTM::new(foci_single(), divide(0xFFFF)),
            nz(7),
            Segment::S1,
            SysTime(DcSysTime::new(0x0123_4567_89AB_CDEF)),
        ),
    );
    emit(
        o,
        g,
        "foci_stm_s1_finite",
        WithFiniteLoop::new(
            FociSTM::new(foci_single(), divide(0xFFFF)),
            nz(3),
            Segment::S1,
            SyncIdx,
        ),
    );
    emit(
        o,
        g,
        "foci_stm_2",
        FociSTM::new(foci_dual(), divide(0xFFFF)),
    );
    emit(
        o,
        g,
        "foci_stm_8",
        FociSTM::new(foci_octuple(), divide(0xFFFF)),
    );

    let patterns = || (0..5).map(|k| gain!(k * 7)).collect::<Vec<_>>();
    for (case, mode) in [
        ("gain_stm_full", GainSTMMode::PhaseIntensityFull),
        ("gain_stm_phase_full", GainSTMMode::PhaseFull),
        ("gain_stm_phase_half", GainSTMMode::PhaseHalf),
    ] {
        emit(
            o,
            g,
            case,
            GainSTM::new(patterns(), divide(0xFFFF), GainSTMOption { mode }),
        );
    }
    emit(
        o,
        g,
        "gain_stm_s1_later",
        WithSegment::new(
            GainSTM::new(patterns(), divide(0xFFFF), GainSTMOption::default()),
            Segment::S1,
            Later,
        ),
    );
    emit(
        o,
        g,
        "gain_stm_s1_gpio",
        WithFiniteLoop::new(
            GainSTM::new(patterns(), divide(0xFFFF), GainSTMOption::default()),
            nz(4),
            Segment::S1,
            GPIO(GPIOIn::I1),
        ),
    );
    emit(
        o,
        g,
        "gain_stm_s1_finite",
        WithFiniteLoop::new(
            GainSTM::new(patterns(), divide(0xFFFF), GainSTMOption::default()),
            nz(2),
            Segment::S1,
            SyncIdx,
        ),
    );
    emit(
        o,
        g,
        "gain_stm_phase_half_remainder",
        GainSTM::new(
            (0..7).map(|k| gain!(k * 7)).collect::<Vec<_>>(),
            divide(0xFFFF),
            GainSTMOption {
                mode: GainSTMMode::PhaseHalf,
            },
        ),
    );

    emit(o, g, "swap_gain", SwapSegmentGain(Segment::S1));
    emit(
        o,
        g,
        "swap_modulation",
        SwapSegmentModulation(Segment::S1, Immediate),
    );
    emit(
        o,
        g,
        "swap_foci_stm",
        SwapSegmentFociSTM(Segment::S1, SysTime(DcSysTime::new(0x0123_4567_89AB_CDEF))),
    );
    emit(
        o,
        g,
        "swap_gain_stm",
        SwapSegmentGainSTM(Segment::S0, GPIO(GPIOIn::I2)),
    );

    emit(
        o,
        g,
        "output_mask",
        OutputMask::new(|_dev: &Device| |tr: &Transducer| !tr.idx().is_multiple_of(3)),
    );
    emit(
        o,
        g,
        "output_mask_s1",
        OutputMask::with_segment(
            |_dev: &Device| |tr: &Transducer| !tr.idx().is_multiple_of(3),
            Segment::S1,
        ),
    );
    emit(
        o,
        g,
        "phase_correction",
        PhaseCorrection::new(|_dev: &Device| |tr: &Transducer| Phase((tr.idx() * 3) as u8)),
    );
    emit(
        o,
        g,
        "pulse_width_encoder",
        PulseWidthEncoder::new(|_dev: &Device| |i: Intensity| PulseWidth::new(i.0 as u16 * 2)),
    );
    emit(
        o,
        g,
        "fpga_gpio_out",
        GPIOOutputs::new(|dev: &Device, gpio: GPIOOut| match gpio {
            GPIOOut::O0 => Some(GPIOOutputType::BaseSignal),
            GPIOOut::O1 => Some(GPIOOutputType::ModIdx(0x1234)),
            GPIOOut::O2 => Some(GPIOOutputType::PwmOut(&dev[3])),
            GPIOOut::O3 => Some(GPIOOutputType::SysTimeEq(DcSysTime::new(
                0x0000_1234_5678_9AB0,
            ))),
        }),
    );
    emit(
        o,
        g,
        "emulate_gpio_in",
        EmulateGPIOIn::new(|_dev: &Device| |gpio: GPIOIn| matches!(gpio, GPIOIn::I0 | GPIOIn::I2)),
    );

    emit(o, g, "clear_sync", (Clear::new(), Synchronize::new()));

    std::fs::write(&path, out).unwrap();
    eprintln!("wrote {path}");
}
