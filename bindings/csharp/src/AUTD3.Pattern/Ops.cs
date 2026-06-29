using System;

namespace AUTD3
{
    public enum PatternBank : byte
    {
        B0 = 0,
        B1 = 1,
    }

    public readonly struct PatternDataType
    {
        internal byte Kind { get; }
        internal byte NumFoci { get; }
        internal ushort SoundSpeed { get; }

        private PatternDataType(byte kind, byte numFoci, ushort soundSpeed)
        {
            Kind = kind;
            NumFoci = numFoci;
            SoundSpeed = soundSpeed;
        }

        public static PatternDataType Raw => new PatternDataType(0, 0, 0);

        public static PatternDataType Foci(byte numFoci, ushort soundSpeed) =>
            new PatternDataType(1, numFoci, soundSpeed);
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
        private readonly PatternDataType _dataType;
        private readonly LoopBehavior _loopBehavior;

        public ConfigPattern(PatternBank bank, SamplingConfig config, uint size, PatternDataType dataType, LoopBehavior? loopBehavior = null)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _dataType = dataType;
            _loopBehavior = loopBehavior ?? LoopBehavior.Infinite;
        }

        IntPtr ICommand.CreateOp()
        {
            var sampling = _config.CreateHandle();
            try
            {
                return NativePattern.autd3_op_config_pattern((byte)_bank, sampling, _size, _dataType.Kind, _dataType.NumFoci, _dataType.SoundSpeed, _loopBehavior.Rep);
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
