using System;

namespace AUTD3
{
    public enum ModulationBank : byte
    {
        B0 = 0,
        B1 = 1,
    }

    public sealed class WriteModulationBuffer : ICommand
    {
        public static readonly int MaxLen = (int)NativeModulationOp.autd3_op_write_modulation_buffer_max_len();

        private readonly ModulationBank _bank;
        private readonly uint _offset;
        private readonly ModulationBuffer _buffer;

        public WriteModulationBuffer(ModulationBank bank, uint offset, ModulationBuffer data)
        {
            _bank = bank;
            _offset = offset;
            _buffer = data;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativeModulationOp.autd3_op_write_modulation_buffer((byte)_bank, _offset, _buffer.Handle);
    }

    public sealed class ConfigModulation : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly SamplingConfig _config;
        private readonly uint _size;
        private readonly LoopBehavior _loopBehavior;

        public ConfigModulation(ModulationBank bank, SamplingConfig config, uint size, LoopBehavior loopBehavior)
        {
            _bank = bank;
            _config = config;
            _size = size;
            _loopBehavior = loopBehavior;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativeModulationOp.autd3_op_config_modulation((byte)_bank, _config.DivideForNative(), _size, _loopBehavior.Rep);
    }

    public sealed class ActivateModulationBank : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly TransitionMode _transitionMode;

        public ActivateModulationBank(ModulationBank bank, TransitionMode transitionMode)
        {
            _bank = bank;
            _transitionMode = transitionMode;
        }

        IntPtr ICommand.CreateOp(Geometry geometry) =>
            NativeModulationOp.autd3_op_activate_modulation_bank((byte)_bank, _transitionMode.Mode, _transitionMode.Value);
    }
}
