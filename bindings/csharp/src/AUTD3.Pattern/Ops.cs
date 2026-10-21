using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    public enum PatternBank : byte
    {
        B0 = 0,
        B1 = 1,
    }

    public enum PhaseDepth : byte
    {
        Bits8 = 0,
        Bits4 = 1,
    }

    public static class PhaseDepthExt
    {
        public static int MaxCount(this PhaseDepth depth)
        {
            if (NativePattern.autd3_phase_depth_max_count(depth.ToBits(), out var maxCount) != 0)
            {
                throw new Autd3Exception($"unknown phase depth {depth}", Autd3ErrorCode.InvalidArgument);
            }
            return (int)maxCount;
        }

        internal static byte ToBits(this PhaseDepth depth) => depth switch
        {
            PhaseDepth.Bits8 => 8,
            PhaseDepth.Bits4 => 4,
            _ => 0,
        };
    }

    public sealed class WritePatternPhase : ICommand
    {
        private readonly PatternBank _bank;
        private readonly ushort _startIndex;
        private readonly PhaseDepth _depth;
        private readonly Intensity _intensity;
        private readonly PhaseBuffer[] _patterns;

        public WritePatternPhase(PatternBank bank, ushort startIndex, PhaseDepth depth, Intensity intensity, PhaseBuffer[] patterns)
        {
            _bank = bank;
            _startIndex = startIndex;
            _depth = depth;
            _intensity = intensity;
            _patterns = patterns;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            var handles = new SafeHandle[_patterns.Length];
            for (var i = 0; i < _patterns.Length; i++)
            {
                handles[i] = _patterns[i].Handle;
            }
            using var lease = new HandleArray(handles);
            return NativePattern.autd3_op_write_pattern_phase((byte)_bank, _startIndex, _depth.ToBits(), _intensity.Value, lease.Pointers, (UIntPtr)lease.Pointers.Length);
        }
    }

    public sealed class WritePatternBuffer : ICommand
    {
        private readonly PatternBank _bank;
        private readonly ushort _index;
        private readonly PhaseBuffer _phases;
        private readonly PatternIntensity _intensities;

        public WritePatternBuffer(PatternBank bank, ushort index, PhaseBuffer phases, PatternIntensity intensities)
        {
            _bank = bank;
            _index = index;
            _phases = phases;
            _intensities = intensities;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            using var intensityLease = new HandleLease(_intensities.Buffer?.Handle);
            return NativePattern.autd3_op_write_pattern_buffer((byte)_bank, _index, _phases.Handle, intensityLease.Pointer, _intensities.Uniform);
        }
    }

    public sealed class ConfigPattern : ICommand
    {
        private readonly PatternBank _bank;
        private readonly SamplingConfig _config;
        private readonly uint _size;
        private readonly LoopBehavior _loopBehavior;

        public ConfigPattern(PatternBank bank, SamplingConfig config, uint size, LoopBehavior loopBehavior)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _loopBehavior = loopBehavior;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativePattern.autd3_op_config_pattern((byte)_bank, _config.DivideForNative(), _size, _loopBehavior.Rep);
    }

    public sealed class ConfigFociStm : ICommand
    {
        private readonly PatternBank _bank;
        private readonly SamplingConfig _config;
        private readonly uint _size;
        private readonly byte _numFoci;
        private readonly Velocity _soundSpeed;
        private readonly LoopBehavior _loopBehavior;

        public ConfigFociStm(PatternBank bank, SamplingConfig config, uint size, byte numFoci, Velocity soundSpeed, LoopBehavior loopBehavior)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _numFoci = numFoci;
            _soundSpeed = soundSpeed;
            _loopBehavior = loopBehavior;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativePattern.autd3_op_config_foci_stm((byte)_bank, _config.DivideForNative(), _size, _numFoci, _soundSpeed.MS, _loopBehavior.Rep);
    }

    public sealed class ActivatePatternBank : ICommand
    {
        private readonly PatternBank _bank;
        private readonly TransitionMode _transitionMode;

        public ActivatePatternBank(PatternBank bank, TransitionMode transitionMode)
        {
            _bank = bank;
            _transitionMode = transitionMode;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativePattern.autd3_op_activate_pattern_bank((byte)_bank, _transitionMode.Mode, _transitionMode.Value);
    }
}
