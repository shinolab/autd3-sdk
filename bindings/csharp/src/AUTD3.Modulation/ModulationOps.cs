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
        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_write_modulation_buffer(byte bank, uint offset, IntPtr modulationBuffer);

        [DllImport("autd3")]
        internal static extern IntPtr autd3_op_config_modulation(byte bank, ushort divider, uint size, ushort rep);

        [DllImport("autd3")]
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
        private readonly ushort _divider;
        private readonly uint _size;
        private readonly LoopBehavior _loopBehavior;

        public ConfigModulation(ModulationBank bank, ushort divider, uint size, LoopBehavior? loopBehavior = null)
        {
            _bank = bank;
            _divider = divider;
            _size = size;
            _loopBehavior = loopBehavior ?? LoopBehavior.Infinite;
        }

        IntPtr ICommand.CreateOp() =>
            NativeModulationOp.autd3_op_config_modulation((byte)_bank, _divider, _size, _loopBehavior.Rep);
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
