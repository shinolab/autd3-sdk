using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    public enum ModulationBank : byte
    {
        B0 = 0,
        B1 = 1,
    }

    internal static class NativeModulationOp
    {
        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_write_modulation_buffer(byte bank, uint offset, IntPtr modulationBuffer);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_config_modulation(byte bank, IntPtr samplingConfig, uint size, ushort rep);

        [DllImport("autd3capi")]
        internal static extern IntPtr autd3_op_change_modulation_bank(byte bank, byte transitionMode, ulong transitionValue);
    }

    public sealed class WriteModulationBuffer : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly uint _offset;
        private readonly ModulationBuffer _buffer;

        public WriteModulationBuffer(ModulationBank bank, uint offset, ModulationBuffer buffer)
        {
            _bank = bank;
            _offset = offset;
            _buffer = buffer;
        }

        IntPtr ICommand.CreateOp() =>
            NativeModulationOp.autd3_op_write_modulation_buffer((byte)_bank, _offset, _buffer.Handle);
    }

    public sealed class ConfigModulation : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly SamplingConfig _config;
        private readonly uint _size;
        private readonly LoopBehavior _loopBehavior;

        public ConfigModulation(ModulationBank bank, SamplingConfig config, uint size, LoopBehavior? loopBehavior = null)
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
                return NativeModulationOp.autd3_op_config_modulation((byte)_bank, sampling, _size, _loopBehavior.Rep);
            }
            finally
            {
                NativeCore.autd3_core_sampling_config_free(sampling);
            }
        }
    }

    public sealed class ChangeModulationBank : ICommand
    {
        private readonly ModulationBank _bank;
        private readonly TransitionMode _transitionMode;

        public ChangeModulationBank(ModulationBank bank, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _transitionMode = transitionMode ?? TransitionMode.Immediate;
        }

        IntPtr ICommand.CreateOp() =>
            NativeModulationOp.autd3_op_change_modulation_bank((byte)_bank, _transitionMode.Mode, _transitionMode.Value);
    }
}
