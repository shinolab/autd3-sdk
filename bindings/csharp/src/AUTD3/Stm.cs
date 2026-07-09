using System;
using System.Collections.Generic;
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
        private const string Lib = "autd3capi";

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
        internal static extern IntPtr autd3_op_write_foci_buffer(byte bank, uint indexOffset, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities);

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
        private readonly SamplingConfig _sampling;

        private StmConfig(ConfigKind kind, float value, SamplingConfig sampling)
        {
            _kind = kind;
            _value = value;
            _sampling = sampling;
        }

        public StmConfig(Freq freq) : this(ConfigKind.Freq, freq.Hz, default)
        {
        }

        public StmConfig(Nearest<Freq> freq) : this(ConfigKind.FreqNearest, freq.Value.Hz, default)
        {
        }

        public StmConfig(TimeSpan period) : this(ConfigKind.Period, (float)period.TotalSeconds, default)
        {
        }

        public StmConfig(Nearest<TimeSpan> period) : this(ConfigKind.PeriodNearest, (float)period.Value.TotalSeconds, default)
        {
        }

        public StmConfig(SamplingConfig sampling) : this(ConfigKind.Sampling, 0f, sampling)
        {
        }

        internal IntPtr CreateHandle()
        {
            var handle = _kind switch
            {
                ConfigKind.Freq => NativeStm.autd3_stm_config_freq(_value),
                ConfigKind.FreqNearest => NativeStm.autd3_stm_config_freq_nearest(_value),
                ConfigKind.Period => NativeStm.autd3_stm_config_period(_value),
                ConfigKind.PeriodNearest => NativeStm.autd3_stm_config_period_nearest(_value),
                _ => NativeStm.autd3_stm_config_sampling(_sampling.Divide()),
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
        public Velocity SoundSpeed { get; }
        public LoopBehavior LoopBehavior { get; }
        public TransitionMode TransitionMode { get; }

        public FociStmOption() : this(bank: PatternBank.B0)
        {
        }

        public FociStmOption(PatternBank bank = PatternBank.B0, Velocity? soundSpeed = null, LoopBehavior? loopBehavior = null, TransitionMode? transitionMode = null)
        {
            Bank = bank;
            SoundSpeed = soundSpeed ?? Velocity.FromMS(340f);
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

        public PatternStmOption() : this(bank: PatternBank.B0)
        {
        }

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
        private readonly ControlPoints[] _points;
        private readonly FociStmOption _option;

        public FociStm(StmConfig config, ControlPoints[] points, FociStmOption? option = null)
        {
            _config = config;
            _points = points;
            _option = option ?? new FociStmOption();
        }

        IntPtr ICommand.CreateOp()
        {
            if (_points.Length == 0)
            {
                throw new Autd3Exception("FociStm requires at least one sample");
            }
            var numFoci = (byte)_points[0].Points.Length;
            var points = new Autd3StmControlPointNative[_points.Length * numFoci];
            var intensities = new byte[_points.Length];
            for (var i = 0; i < _points.Length; i++)
            {
                if (_points[i].Points.Length != numFoci)
                {
                    throw new Autd3Exception("all FociStm samples must have the same number of foci");
                }
                intensities[i] = _points[i].Intensity.Value;
                for (var j = 0; j < numFoci; j++)
                {
                    var cp = _points[i].Points[j];
                    var p = Coords.Point(cp.Point);
                    points[i * numFoci + j] = new Autd3StmControlPointNative
                    {
                        X = p.X,
                        Y = p.Y,
                        Z = p.Z,
                        PhaseOffset = cp.PhaseOffset.Value,
                    };
                }
            }
            var configHandle = _config.CreateHandle();
            try
            {
                return NativeStm.autd3_op_foci_stm(configHandle, points, (UIntPtr)_points.Length, numFoci, intensities,
                    (byte)_option.Bank, _option.SoundSpeed.MPerS, _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
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

    public sealed class WriteFociBuffer : ICommand
    {
        private readonly PatternBank _bank;
        private readonly uint _indexOffset;
        private readonly ControlPoints[] _points;

        public WriteFociBuffer(PatternBank bank, uint indexOffset, ControlPoints[] points)
        {
            _bank = bank;
            _indexOffset = indexOffset;
            _points = points;
        }

        IntPtr ICommand.CreateOp()
        {
            if (_points.Length == 0)
            {
                throw new Autd3Exception("WriteFociBuffer requires at least one sample");
            }
            var numFoci = (byte)_points[0].Points.Length;
            var points = new Autd3StmControlPointNative[_points.Length * numFoci];
            var intensities = new byte[_points.Length];
            for (var i = 0; i < _points.Length; i++)
            {
                if (_points[i].Points.Length != numFoci)
                {
                    throw new Autd3Exception("all WriteFociBuffer samples must have the same number of foci");
                }
                intensities[i] = _points[i].Intensity.Value;
                for (var j = 0; j < numFoci; j++)
                {
                    var cp = _points[i].Points[j];
                    var p = Coords.Point(cp.Point);
                    points[i * numFoci + j] = new Autd3StmControlPointNative
                    {
                        X = p.X,
                        Y = p.Y,
                        Z = p.Z,
                        PhaseOffset = cp.PhaseOffset.Value,
                    };
                }
            }
            return NativeStm.autd3_op_write_foci_buffer((byte)_bank, _indexOffset, points, (UIntPtr)_points.Length, numFoci, intensities);
        }
    }

    public static class Stm
    {
        private static void Fill(List<ControlPoints> dst, Autd3StmControlPointNative[] points, byte[] intensities)
        {
            dst.Clear();
            for (var i = 0; i < points.Length; i++)
            {
                var cp = new ControlPoint(Coords.FromPointArray(new[] { points[i].X, points[i].Y, points[i].Z }), new Phase(points[i].PhaseOffset));
                dst.Add(new ControlPoints(new[] { cp }, new Intensity(intensities[i])));
            }
        }

        public static void Circle(Vector3 center, Length radius, int numPoints, Vector3 normal, Intensity intensity, List<ControlPoints> dst)
        {
            var outPoints = new Autd3StmControlPointNative[numPoints];
            var outIntensities = new byte[numPoints];
            if (NativeStm.autd3_stm_circle(Coords.PointArray(center), radius.Mm, (UIntPtr)numPoints,
                Coords.DirArray(normal), intensity.Value, outPoints, outIntensities) != 0)
            {
                throw new Autd3Exception("circle failed");
            }
            Fill(dst, outPoints, outIntensities);
        }

        public static void Circle(Vector3 center, Length radius, int numPoints, Vector3 normal, List<ControlPoints> dst) =>
            Circle(center, radius, numPoints, normal, Intensity.Max, dst);

        public static void Line(Vector3 start, Vector3 end, int numPoints, Intensity intensity, List<ControlPoints> dst)
        {
            var outPoints = new Autd3StmControlPointNative[numPoints];
            var outIntensities = new byte[numPoints];
            if (NativeStm.autd3_stm_line(Coords.PointArray(start), Coords.PointArray(end), (UIntPtr)numPoints,
                intensity.Value, outPoints, outIntensities) != 0)
            {
                throw new Autd3Exception("line failed");
            }
            Fill(dst, outPoints, outIntensities);
        }

        public static void Line(Vector3 start, Vector3 end, int numPoints, List<ControlPoints> dst) =>
            Line(start, end, numPoints, Intensity.Max, dst);
    }
}
