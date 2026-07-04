using System;

namespace AUTD3
{
    public enum PatternBank : byte
    {
        B0 = 0,
        B1 = 1,
    }

    public enum PatternCompression : byte
    {
        PhaseFull = 1,
        PhaseHalf = 2,
    }

    public sealed class WritePatternCompressed : ICommand
    {
        private readonly PatternBank _bank;
        private readonly uint _index;
        private readonly PatternCompression _format;
        private readonly PatternBuffer[] _patterns;

        public WritePatternCompressed(PatternBank bank, uint index, PatternCompression format, PatternBuffer[] patterns)
        {
            if (patterns.Length == 0 || patterns.Length > 4)
            {
                throw new Autd3Exception("WritePatternCompressed expects 1..=4 pattern buffers");
            }
            _bank = bank;
            _index = index;
            _format = format;
            _patterns = patterns;
        }

        IntPtr ICommand.CreateOp()
        {
            var handles = new IntPtr[_patterns.Length];
            for (var i = 0; i < _patterns.Length; i++)
            {
                handles[i] = _patterns[i].Handle;
            }
            return NativePattern.autd3_op_write_pattern_compressed((byte)_bank, _index, (byte)_format, handles, (UIntPtr)handles.Length);
        }
    }

    public sealed class WritePatternBuffer : ICommand
    {
        private readonly PatternBank _bank;
        private readonly ushort _index;
        private readonly PatternBuffer _buffer;

        public WritePatternBuffer(PatternBank bank, ushort index, PatternBuffer buffer)
        {
            _bank = bank;
            _index = index;
            _buffer = buffer;
        }

        IntPtr ICommand.CreateOp() =>
            NativePattern.autd3_op_write_pattern_buffer((byte)_bank, _index, _buffer.Handle);
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
                return NativePattern.autd3_op_config_foci_stm((byte)_bank, sampling, _size, _numFoci, _soundSpeed.MPerS, _loopBehavior.Rep);
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
            NativePattern.autd3_op_change_pattern_bank((byte)_bank, _transitionMode.Mode, _transitionMode.Value);
    }
}
