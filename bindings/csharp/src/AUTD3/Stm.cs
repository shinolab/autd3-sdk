using System;
using System.Collections.Generic;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct Autd3StmControlPointNative
    {
        public float X;
        public float Y;
        public float Z;
        public byte PhaseOffset;
    }

    internal static class FociPoints
    {
        internal static (Autd3StmControlPointNative[] Points, byte[] Intensities, byte NumFoci) Flatten(IReadOnlyList<ControlPoints> samples, string command)
        {
            if (samples.Count == 0)
            {
                throw new Autd3Exception($"{command} requires at least one sample", Autd3ErrorCode.InvalidArgument);
            }
            var firstFoci = samples[0].Points?.Length ?? 0;
            if (firstFoci < 1 || firstFoci > Params.NumFociMax)
            {
                throw new Autd3Exception($"number of foci per sample must be 1..={Params.NumFociMax}", Autd3ErrorCode.InvalidArgument);
            }
            var numFoci = (byte)firstFoci;
            var points = new Autd3StmControlPointNative[samples.Count * numFoci];
            var intensities = new byte[samples.Count];
            for (var i = 0; i < samples.Count; i++)
            {
                if ((samples[i].Points?.Length ?? 0) != numFoci)
                {
                    throw new Autd3Exception($"all {command} samples must have the same number of foci", Autd3ErrorCode.InvalidArgument);
                }
                intensities[i] = samples[i].Intensity.Value;
                for (var j = 0; j < numFoci; j++)
                {
                    var cp = samples[i].Points[j];
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
            return (points, intensities, numFoci);
        }
    }

    internal static class NativeStm
    {
        private const string Lib = "autd3capi";

        static NativeStm() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_stm_config_freq(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_stm_config_freq_nearest(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_stm_config_period(ulong periodNs);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_stm_config_period_nearest(ulong periodNs);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_stm_config_sampling(ushort divide);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_stm_config_into_sampling_config(IntPtr config, UIntPtr size, out ushort @out, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_stm_config_free(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_foci_stm(IntPtr config, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities, byte bank, float soundSpeedMS, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_foci_buffer(byte bank, uint indexOffset, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_pattern_stm(IntPtr config, IntPtr[] phases, UIntPtr numPatterns, byte intensityKind, IntPtr[] intensities, UIntPtr numIntensities, byte uniformIntensity, byte bank, byte phaseDepth, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_foci_stm_option_default_sound_speed();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_stm_circle(float[] center, float radiusMm, UIntPtr numPoints, float[] normal, byte intensity, Autd3StmControlPointNative[] outPoints, byte[] outIntensities);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
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
        private readonly ulong _periodNs;
        private readonly SamplingConfig _sampling;

        private StmConfig(ConfigKind kind, float value, ulong periodNs, SamplingConfig sampling)
        {
            _kind = kind;
            _value = value;
            _periodNs = periodNs;
            _sampling = sampling;
        }

        private static ulong PeriodNanos(TimeSpan period)
        {
            if (period < TimeSpan.Zero)
            {
                throw new Autd3Exception("an STM period must not be negative", Autd3ErrorCode.InvalidArgument);
            }
            return OptionNative.ToNanos(period);
        }

        public StmConfig(Freq freq) : this(ConfigKind.Freq, freq.Hz, 0, default)
        {
        }

        public StmConfig(Nearest<Freq> freq) : this(ConfigKind.FreqNearest, freq.Value.Hz, 0, default)
        {
        }

        public StmConfig(TimeSpan period) : this(ConfigKind.Period, 0f, PeriodNanos(period), default)
        {
        }

        public StmConfig(Nearest<TimeSpan> period) : this(ConfigKind.PeriodNearest, 0f, PeriodNanos(period.Value), default)
        {
        }

        public StmConfig(SamplingConfig sampling) : this(ConfigKind.Sampling, 0f, 0, sampling)
        {
        }

        public static implicit operator StmConfig(Freq freq) => new StmConfig(freq);

        public static implicit operator StmConfig(Nearest<Freq> freq) => new StmConfig(freq);

        public static implicit operator StmConfig(TimeSpan period) => new StmConfig(period);

        public static implicit operator StmConfig(Nearest<TimeSpan> period) => new StmConfig(period);

        public static implicit operator StmConfig(SamplingConfig sampling) => new StmConfig(sampling);

        public SamplingConfig IntoSamplingConfig(int size)
        {
            var handle = CreateHandle();
            try
            {
                var err = new byte[NativeAbi.ErrorBufferLength];
                if (NativeStm.autd3_stm_config_into_sampling_config(handle, (UIntPtr)Math.Max(size, 1), out var divide, err, (UIntPtr)err.Length) != 0)
                {
                    throw new Autd3Exception(NativeUtil.Utf8(err), Autd3ErrorCode.InvalidArgument);
                }
                return new SamplingConfig(divide);
            }
            finally
            {
                NativeStm.autd3_stm_config_free(handle);
            }
        }

        internal IntPtr CreateHandle()
        {
            var handle = _kind switch
            {
                ConfigKind.Freq => NativeStm.autd3_stm_config_freq(_value),
                ConfigKind.FreqNearest => NativeStm.autd3_stm_config_freq_nearest(_value),
                ConfigKind.Period => NativeStm.autd3_stm_config_period(_periodNs),
                ConfigKind.PeriodNearest => NativeStm.autd3_stm_config_period_nearest(_periodNs),
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

        public ControlPoint(Vector3 point, Phase phaseOffset)
        {
            Point = point;
            PhaseOffset = phaseOffset;
        }

        public static explicit operator ControlPoint(Vector3 point) => new ControlPoint(point, Phase.Zero);
    }

    public readonly struct ControlPoints
    {
        public ControlPoint[] Points { get; }
        public Intensity Intensity { get; }

        public ControlPoints(ControlPoint[] points, Intensity intensity)
        {
            Points = points;
            Intensity = intensity;
        }

        public static explicit operator ControlPoints(Vector3 point) => (ControlPoints)(ControlPoint)point;

        public static explicit operator ControlPoints(ControlPoint point) => new ControlPoints(new[] { point }, Intensity.Max);

        public static explicit operator ControlPoints(Vector3[] points)
        {
            if (points == null)
            {
                throw new ArgumentNullException(nameof(points));
            }
            var controlPoints = new ControlPoint[points.Length];
            for (var i = 0; i < points.Length; i++)
            {
                controlPoints[i] = (ControlPoint)points[i];
            }
            return new ControlPoints(controlPoints, Intensity.Max);
        }
    }

    internal static class FociStmDefaults
    {
        internal static readonly Velocity SoundSpeed = Velocity.FromMS(NativeStm.autd3_foci_stm_option_default_sound_speed());
    }

    public readonly struct FociStmOption
    {
        public PatternBank Bank { get; init; }
        private readonly Velocity? _soundSpeed;
        public Velocity SoundSpeed { get => _soundSpeed ?? FociStmDefaults.SoundSpeed; init => _soundSpeed = value; }
        public LoopBehavior LoopBehavior { get; init; }
        public TransitionMode TransitionMode { get; init; }
    }

    public readonly struct PatternStmOption
    {
        public PatternBank Bank { get; init; }
        public PhaseDepth PhaseDepth { get; init; }
        public LoopBehavior LoopBehavior { get; init; }
        public TransitionMode TransitionMode { get; init; }
    }

    public sealed class FociStm : ICommand
    {
        private readonly StmConfig _config;
        private readonly IReadOnlyList<ControlPoints> _points;
        private readonly FociStmOption _option;

        public FociStm(StmConfig config, IReadOnlyList<ControlPoints> points, FociStmOption option)
        {
            _config = config;
            _points = points;
            _option = option;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var (points, intensities, numFoci) = FociPoints.Flatten(_points, "FociStm");
            var configHandle = _config.CreateHandle();
            try
            {
                return NativeStm.autd3_op_foci_stm(configHandle, points, (UIntPtr)_points.Count, numFoci, intensities,
                    (byte)_option.Bank, _option.SoundSpeed.MS, _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
            }
            finally
            {
                NativeStm.autd3_stm_config_free(configHandle);
            }
        }
    }

    public readonly struct StmIntensity
    {
        internal const byte KindUniform = 0;
        internal const byte KindShared = 1;
        internal const byte KindPerIndex = 2;

        private readonly IntensityBuffer[]? _buffers;
        private readonly Intensity _uniform;
        private readonly byte _kind;

        public StmIntensity(Intensity uniform)
        {
            _buffers = null;
            _uniform = uniform;
            _kind = KindUniform;
        }

        public StmIntensity(IntensityBuffer shared)
        {
            _buffers = new[] { shared ?? throw new ArgumentNullException(nameof(shared)) };
            _uniform = default;
            _kind = KindShared;
        }

        public StmIntensity(IntensityBuffer[] perIndex)
        {
            _buffers = perIndex ?? throw new ArgumentNullException(nameof(perIndex));
            _uniform = default;
            _kind = KindPerIndex;
        }

        internal IntensityBuffer[] Buffers => _buffers ?? Array.Empty<IntensityBuffer>();

        internal byte Kind => _kind;

        internal byte Uniform => _uniform.Value;

        public static implicit operator StmIntensity(Intensity uniform) => new StmIntensity(uniform);

        public static implicit operator StmIntensity(IntensityBuffer shared) => new StmIntensity(shared);

        public static implicit operator StmIntensity(IntensityBuffer[] perIndex) => new StmIntensity(perIndex);
    }

    public sealed class PatternStm : ICommand
    {
        private readonly StmConfig _config;
        private readonly PhaseBuffer[] _phases;
        private readonly StmIntensity _intensities;
        private readonly PatternStmOption _option;

        public PatternStm(StmConfig config, PhaseBuffer[] phases, StmIntensity intensities, PatternStmOption option)
        {
            _config = config;
            _phases = phases ?? throw new ArgumentNullException(nameof(phases));
            _intensities = intensities;
            _option = option;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var buffers = _intensities.Buffers;
            var phaseHandles = new SafeHandle[_phases.Length];
            var intensityHandles = new SafeHandle[buffers.Length];
            for (var i = 0; i < _phases.Length; i++)
            {
                phaseHandles[i] = _phases[i].Handle;
            }
            for (var i = 0; i < buffers.Length; i++)
            {
                intensityHandles[i] = buffers[i].Handle;
            }
            using var phaseLease = new HandleArray(phaseHandles);
            using var intensityLease = new HandleArray(intensityHandles);
            var configHandle = _config.CreateHandle();
            try
            {
                return NativeStm.autd3_op_pattern_stm(configHandle, phaseLease.Pointers, (UIntPtr)phaseLease.Pointers.Length,
                    _intensities.Kind, intensityLease.Pointers, (UIntPtr)intensityLease.Pointers.Length, _intensities.Uniform,
                    (byte)_option.Bank, _option.PhaseDepth.ToBits(), _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
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
        private readonly IReadOnlyList<ControlPoints> _points;

        public WriteFociBuffer(PatternBank bank, uint indexOffset, IReadOnlyList<ControlPoints> points)
        {
            _bank = bank;
            _indexOffset = indexOffset;
            _points = points;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var (points, intensities, numFoci) = FociPoints.Flatten(_points, "WriteFociBuffer");
            return NativeStm.autd3_op_write_foci_buffer((byte)_bank, _indexOffset, points, (UIntPtr)_points.Count, numFoci, intensities);
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
                throw new Autd3Exception("circle failed", Autd3ErrorCode.InvalidArgument);
            }
            Fill(dst, outPoints, outIntensities);
        }

        public static void Line(Vector3 start, Vector3 end, int numPoints, Intensity intensity, List<ControlPoints> dst)
        {
            var outPoints = new Autd3StmControlPointNative[numPoints];
            var outIntensities = new byte[numPoints];
            if (NativeStm.autd3_stm_line(Coords.PointArray(start), Coords.PointArray(end), (UIntPtr)numPoints,
                intensity.Value, outPoints, outIntensities) != 0)
            {
                throw new Autd3Exception("line failed", Autd3ErrorCode.InvalidArgument);
            }
            Fill(dst, outPoints, outIntensities);
        }
    }
}
