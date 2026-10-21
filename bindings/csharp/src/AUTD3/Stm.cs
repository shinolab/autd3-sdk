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
        internal static extern IntPtr autd3_op_foci_stm(ushort configDivide, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities, byte bank, float soundSpeedMS, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_foci_buffer(byte bank, uint startIndex, Autd3StmControlPointNative[] points, UIntPtr numSamples, byte numFoci, byte[] intensities);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_pattern_stm(ushort configDivide, IntPtr[] phases, UIntPtr numPatterns, byte intensityKind, IntPtr[] intensities, UIntPtr numIntensities, byte uniformIntensity, byte bank, byte phaseDepth, ushort loopRep, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_foci_stm_option_default_sound_speed();
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
        private readonly SamplingConfig _config;
        private readonly IReadOnlyList<ControlPoints> _points;
        private readonly FociStmOption _option;

        public FociStm(SamplingConfig config, IReadOnlyList<ControlPoints> points, FociStmOption option)
        {
            _config = config;
            _points = points;
            _option = option;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var (points, intensities, numFoci) = FociPoints.Flatten(_points, "FociStm");
            return NativeStm.autd3_op_foci_stm(_config.DivideForNative(), points, (UIntPtr)_points.Count, numFoci, intensities,
                (byte)_option.Bank, _option.SoundSpeed.MS, _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
        }
    }

    public readonly struct StmIntensity
    {
        internal const byte KindUniform = 0;
        internal const byte KindSameForAllIndices = 1;
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

        public StmIntensity(IntensityBuffer sameForAllIndices)
        {
            _buffers = new[] { sameForAllIndices ?? throw new ArgumentNullException(nameof(sameForAllIndices)) };
            _uniform = default;
            _kind = KindSameForAllIndices;
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

        public static implicit operator StmIntensity(IntensityBuffer sameForAllIndices) => new StmIntensity(sameForAllIndices);

        public static implicit operator StmIntensity(IntensityBuffer[] perIndex) => new StmIntensity(perIndex);
    }

    public sealed class PatternStm : ICommand
    {
        private readonly SamplingConfig _config;
        private readonly PhaseBuffer[] _phases;
        private readonly StmIntensity _intensities;
        private readonly PatternStmOption _option;

        public PatternStm(SamplingConfig config, PhaseBuffer[] phases, StmIntensity intensities, PatternStmOption option)
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
            return NativeStm.autd3_op_pattern_stm(_config.DivideForNative(), phaseLease.Pointers, (UIntPtr)phaseLease.Pointers.Length,
                _intensities.Kind, intensityLease.Pointers, (UIntPtr)intensityLease.Pointers.Length, _intensities.Uniform,
                (byte)_option.Bank, _option.PhaseDepth.ToBits(), _option.LoopBehavior.Rep, _option.TransitionMode.Mode, _option.TransitionMode.Value);
        }
    }

    public sealed class WriteFociBuffer : ICommand
    {
        private readonly PatternBank _bank;
        private readonly uint _startIndex;
        private readonly IReadOnlyList<ControlPoints> _points;

        public WriteFociBuffer(PatternBank bank, uint startIndex, IReadOnlyList<ControlPoints> points)
        {
            _bank = bank;
            _startIndex = startIndex;
            _points = points;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var (points, intensities, numFoci) = FociPoints.Flatten(_points, "WriteFociBuffer");
            return NativeStm.autd3_op_write_foci_buffer((byte)_bank, _startIndex, points, (UIntPtr)_points.Count, numFoci, intensities);
        }
    }
}
