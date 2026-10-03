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
        Bits8 = 8,
        Bits4 = 4,
    }

    public static class PhaseDepthExt
    {
        public static int MaxCount(this PhaseDepth depth)
        {
            if (NativePattern.autd3_phase_depth_max_count((byte)depth, out var maxCount) != 0)
            {
                throw new Autd3Exception($"unknown phase depth {depth}");
            }
            return (int)maxCount;
        }
    }

    public sealed class WritePatternPhase : ICommand
    {
        private readonly PatternBank _bank;
        private readonly ushort _index;
        private readonly PhaseDepth _depth;
        private readonly Intensity _intensity;
        private readonly PhaseBuffer[] _patterns;

        public WritePatternPhase(PatternBank bank, ushort index, PhaseDepth depth, Intensity intensity, PhaseBuffer[] patterns)
        {
            _bank = bank;
            _index = index;
            _depth = depth;
            _intensity = intensity;
            _patterns = patterns;
        }

        IntPtr ICommand.CreateOp()
        {
            var handles = new SafeHandle[_patterns.Length];
            for (var i = 0; i < _patterns.Length; i++)
            {
                handles[i] = _patterns[i].Handle;
            }
            using var lease = new HandleArray(handles);
            return NativePattern.autd3_op_write_pattern_phase((byte)_bank, _index, (byte)_depth, _intensity.Value, lease.Pointers, (UIntPtr)lease.Pointers.Length);
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

        IntPtr ICommand.CreateOp()
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

        public ConfigPattern(PatternBank bank, SamplingConfig config, uint size, LoopBehavior? loopBehavior = null)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _loopBehavior = loopBehavior ?? LoopBehavior.Infinite;
        }

        IntPtr ICommand.CreateOp()
        {
            var sampling = _config.CreateHandle();
            try
            {
                return NativePattern.autd3_op_config_pattern((byte)_bank, sampling, _size, _loopBehavior.Rep);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }
    }

    public sealed class ConfigFociStm : ICommand
    {
        private readonly PatternBank _bank;
        private readonly SamplingConfig _config;
        private readonly uint _size;
        private readonly byte _numFoci;
        private readonly Velocity _soundSpeed;
        private readonly LoopBehavior _loopBehavior;

        public ConfigFociStm(PatternBank bank, SamplingConfig config, uint size, byte numFoci, Velocity? soundSpeed = null, LoopBehavior? loopBehavior = null)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _numFoci = numFoci;
            _soundSpeed = soundSpeed ?? Velocity.FromMS(340f);
            _loopBehavior = loopBehavior ?? LoopBehavior.Infinite;
        }

        IntPtr ICommand.CreateOp()
        {
            var sampling = _config.CreateHandle();
            try
            {
                return NativePattern.autd3_op_config_foci_stm((byte)_bank, sampling, _size, _numFoci, _soundSpeed.MS, _loopBehavior.Rep);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }
    }

    public sealed class ChangePatternBank : ICommand
    {
        private readonly PatternBank _bank;
        private readonly TransitionMode _transitionMode;

        public ChangePatternBank(PatternBank bank, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _transitionMode = transitionMode ?? TransitionMode.Immediate;
        }

        IntPtr ICommand.CreateOp() =>
            NativePattern.autd3_op_change_pattern_bank((byte)_bank, _transitionMode.Mode, _transitionMode.Value, _transitionMode.MarginNs);
    }
}
