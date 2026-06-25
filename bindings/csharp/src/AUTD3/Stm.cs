using System;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    public enum PatternStmMode : byte
    {
        PhaseIntensityFull = 0,
        PhaseFull = 1,
        PhaseHalf = 2,
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct Autd3StmControlPointNative
    {
        public float X;
        public float Y;
        public float Z;
        public byte PhaseOffset;
    }

    internal static class NativeStm
    {
        private const string Lib = "autd3";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_stm_config_freq(float hz);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_stm_config_freq_nearest(float hz);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_stm_config_period(float secs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_stm_config_period_nearest(float secs);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_stm_config_sampling(ushort divide);

        [DllImport(Lib)]
        internal static extern void autd3_stm_config_free(IntPtr config);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_foci_stm(IntPtr config, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities, byte bank, float soundSpeedMS, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_op_pattern_stm(IntPtr config, IntPtr[] patterns, UIntPtr numPatterns, byte bank, byte mode, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib)]
        internal static extern int autd3_stm_circle(float[] center, float radiusMm, UIntPtr numPoints, float[] normal, byte intensity, Autd3StmControlPointNative[] outPoints, byte[] outIntensities);

        [DllImport(Lib)]
        internal static extern int autd3_stm_line(float[] start, float[] end, UIntPtr numPoints, byte intensity, Autd3StmControlPointNative[] outPoints, byte[] outIntensities);
    }

    public readonly struct StmConfig
    {
        private enum ConfigKind : byte
        {
            Freq,
            FreqNearest,
            Period,
            PeriodNearest,
            Sampling,
        }

        private readonly ConfigKind _kind;
        private readonly float _value;
        private readonly ushort _divide;

        private StmConfig(ConfigKind kind, float value, ushort divide)
        {
            _kind = kind;
            _value = value;
            _divide = divide;
        }

        public static StmConfig Freq(float hz) => new StmConfig(ConfigKind.Freq, hz, 0);
        public static StmConfig FreqNearest(float hz) => new StmConfig(ConfigKind.FreqNearest, hz, 0);
        public static StmConfig Period(float secs) => new StmConfig(ConfigKind.Period, secs, 0);
        public static StmConfig PeriodNearest(float secs) => new StmConfig(ConfigKind.PeriodNearest, secs, 0);
        public static StmConfig Sampling(ushort divide) => new StmConfig(ConfigKind.Sampling, 0, divide);

        internal IntPtr CreateHandle()
        {
            var handle = _kind switch
            {
                ConfigKind.Freq => NativeStm.autd3_stm_config_freq(_value),
                ConfigKind.FreqNearest => NativeStm.autd3_stm_config_freq_nearest(_value),
                ConfigKind.Period => NativeStm.autd3_stm_config_period(_value),
                ConfigKind.PeriodNearest => NativeStm.autd3_stm_config_period_nearest(_value),
                _ => NativeStm.autd3_stm_config_sampling(_divide),
            };
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create stm config");
            }
            return handle;
        }
    }

    public readonly struct ControlPoint
    {
        public Vector3 Point { get; }
        public Phase PhaseOffset { get; }

        public ControlPoint(Vector3 point, Phase? phaseOffset = null)
        {
            Point = point;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }
    }

    public readonly struct ControlPoints
    {
        public ControlPoint[] Points { get; }
        public Intensity Intensity { get; }

        public ControlPoints(ControlPoint[] points, Intensity? intensity = null)
        {
            Points = points;
            Intensity = intensity ?? Intensity.Max;
        }
    }

    public readonly struct FociStmOption
    {
        public PatternBank Bank { get; }
        public float SoundSpeedMS { get; }
        public LoopBehavior LoopBehavior { get; }
        public TransitionMode TransitionMode { get; }

        public FociStmOption(PatternBank bank = PatternBank.B0, float soundSpeedMS = 340.0f, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
        {
            Bank = bank;
            SoundSpeedMS = soundSpeedMS;
            LoopBehavior = loopBehavior ?? LoopBehavior.Infinite;
            TransitionMode = transitionMode ?? TransitionMode.Immediate;
        }
    }

    public readonly struct PatternStmOption
    {
        public PatternBank Bank { get; }
        public PatternStmMode Mode { get; }
        public LoopBehavior LoopBehavior { get; }
        public TransitionMode TransitionMode { get; }

        public PatternStmOption(PatternBank bank = PatternBank.B0, PatternStmMode mode = PatternStmMode.PhaseIntensityFull, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
        {
            Bank = bank;
            Mode = mode;
            LoopBehavior = loopBehavior ?? LoopBehavior.Infinite;
            TransitionMode = transitionMode ?? TransitionMode.Immediate;
        }
    }

    public sealed class FociStm : ICommand
    {
        private readonly StmConfig _config;
        private readonly ControlPoints[] _samples;
        private readonly FociStmOption _option;

        public FociStm(StmConfig config, ControlPoints[] samples, FociStmOption? option = null)
        {
            _config = config;
            _samples = samples;
            _option = option ?? new FociStmOption(soundSpeedMS: 340.0f);
        }

        IntPtr ICommand.CreateOp()
        {
            if (_samples.Length == 0)
            {
                throw new Autd3Exception("FociStm requires at least one sample");
            }
            var numFoci = (byte)_samples[0].Points.Length;
            var points = new Autd3StmControlPointNative[_samples.Length * numFoci];
            var intensities = new byte[_samples.Length];
            for (var i = 0; i < _samples.Length; i++)
            {
                if (_samples[i].Points.Length != numFoci)
                {
                    throw new Autd3Exception("all FociStm samples must have the same number of foci");
                }
                intensities[i] = _samples[i].Intensity.Value;
                for (var j = 0; j < numFoci; j++)
                {
                    var cp = _samples[i].Points[j];
                    points[i * numFoci + j] = new Autd3StmControlPointNative
                    {
                        X = cp.Point.X,
                        Y = cp.Point.Y,
                        Z = cp.Point.Z,
                        PhaseOffset = cp.PhaseOffset.Value,
                    };
                }
            }
            var configHandle = _config.CreateHandle();
            try
            {
                return NativeStm.autd3_op_foci_stm(configHandle, points, (UIntPtr)_samples.Length, numFoci, intensities,
                    (byte)_option.Bank, _option.SoundSpeedMS, _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
            }
            finally
            {
                NativeStm.autd3_stm_config_free(configHandle);
            }
        }
    }

    public sealed class PatternStm : ICommand
    {
        private readonly StmConfig _config;
        private readonly PatternBuffer[] _patterns;
        private readonly PatternStmOption _option;

        public PatternStm(StmConfig config, PatternBuffer[] patterns, PatternStmOption? option = null)
        {
            _config = config;
            _patterns = patterns;
            _option = option ?? new PatternStmOption(PatternBank.B0);
        }

        IntPtr ICommand.CreateOp()
        {
            var handles = new IntPtr[_patterns.Length];
            for (var i = 0; i < _patterns.Length; i++)
            {
                handles[i] = _patterns[i].Handle;
            }
            var configHandle = _config.CreateHandle();
            try
            {
                return NativeStm.autd3_op_pattern_stm(configHandle, handles, (UIntPtr)handles.Length,
                    (byte)_option.Bank, (byte)_option.Mode, _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
            }
            finally
            {
                NativeStm.autd3_stm_config_free(configHandle);
            }
        }
    }

    public static class Stm
    {
        private static ControlPoints[] Convert(Autd3StmControlPointNative[] points, byte[] intensities)
        {
            var result = new ControlPoints[points.Length];
            for (var i = 0; i < points.Length; i++)
            {
                var cp = new ControlPoint(new Vector3(points[i].X, points[i].Y, points[i].Z), new Phase(points[i].PhaseOffset));
                result[i] = new ControlPoints(new[] { cp }, new Intensity(intensities[i]));
            }
            return result;
        }

        public static ControlPoints[] Circle(Vector3 center, float radiusMm, int numPoints, Vector3 normal, Intensity? intensity = null)
        {
            var outPoints = new Autd3StmControlPointNative[numPoints];
            var outIntensities = new byte[numPoints];
            if (NativeStm.autd3_stm_circle(new[] { center.X, center.Y, center.Z }, radiusMm, (UIntPtr)numPoints,
                new[] { normal.X, normal.Y, normal.Z }, (intensity ?? Intensity.Max).Value, outPoints, outIntensities) != 0)
            {
                throw new Autd3Exception("circle failed");
            }
            return Convert(outPoints, outIntensities);
        }

        public static ControlPoints[] Line(Vector3 start, Vector3 end, int numPoints, Intensity? intensity = null)
        {
            var outPoints = new Autd3StmControlPointNative[numPoints];
            var outIntensities = new byte[numPoints];
            if (NativeStm.autd3_stm_line(new[] { start.X, start.Y, start.Z }, new[] { end.X, end.Y, end.Z }, (UIntPtr)numPoints,
                (intensity ?? Intensity.Max).Value, outPoints, outIntensities) != 0)
            {
                throw new Autd3Exception("line failed");
            }
            return Convert(outPoints, outIntensities);
        }
    }
}
