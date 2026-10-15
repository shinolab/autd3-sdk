using System;

namespace AUTD3
{
    public enum GpioIn : byte
    {
        I0 = 0,
        I1 = 1,
        I2 = 2,
        I3 = 3,
    }

    public readonly struct LoopBehavior : IEquatable<LoopBehavior>
    {
        private readonly ushort _count;

        private LoopBehavior(ushort count)
        {
            _count = count;
        }

        public ushort Rep => _count == 0 ? (ushort)0xFFFF : (ushort)(_count - 1);

        public static LoopBehavior Infinite => default;

        public static LoopBehavior Once => new LoopBehavior(1);

        public static LoopBehavior Finite(ushort count)
        {
            if (count == 0)
            {
                throw new Autd3Exception("loop count must be >= 1", Autd3ErrorCode.InvalidArgument);
            }
            return new LoopBehavior(count);
        }

        public bool Equals(LoopBehavior other) => _count == other._count;

        public override bool Equals(object? obj) => obj is LoopBehavior other && Equals(other);

        public override int GetHashCode() => _count;

        public static bool operator ==(LoopBehavior left, LoopBehavior right) => left.Equals(right);

        public static bool operator !=(LoopBehavior left, LoopBehavior right) => !left.Equals(right);
    }

    public readonly struct TransitionMode : IEquatable<TransitionMode>
    {
        private const byte ImmediateMode = 0xFF;

        private readonly byte _modeFromImmediate;

        internal byte Mode => (byte)(_modeFromImmediate ^ ImmediateMode);
        internal ulong Value { get; }

        private TransitionMode(byte mode, ulong value)
        {
            _modeFromImmediate = (byte)(mode ^ ImmediateMode);
            Value = value;
        }

        public static TransitionMode SyncIdx => new TransitionMode(0x00, 0);

        public static TransitionMode SysTime(SysTime sysTime) => new TransitionMode(0x01, sysTime.Nanos);

        public static TransitionMode Gpio(GpioIn gpio) => new TransitionMode(0x02, (byte)gpio);

        public static TransitionMode Ext => new TransitionMode(0xF0, 0);

        public static TransitionMode Later => new TransitionMode(0xFE, 0);

        public static TransitionMode Immediate => new TransitionMode(ImmediateMode, 0);

        public bool IsLater => Mode == 0xFE;

        public bool Equals(TransitionMode other) => _modeFromImmediate == other._modeFromImmediate && Value == other.Value;

        public override bool Equals(object? obj) => obj is TransitionMode other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(_modeFromImmediate, Value);

        public static bool operator ==(TransitionMode left, TransitionMode right) => left.Equals(right);

        public static bool operator !=(TransitionMode left, TransitionMode right) => !left.Equals(right);
    }

    public enum FpgaBusWait : byte
    {
        Cycles3 = 0,
        Cycles2 = 1,
    }

    internal static class FpgaBusWaitNative
    {
        internal static byte ToCycles(FpgaBusWait wait) => wait switch
        {
            FpgaBusWait.Cycles2 => 2,
            FpgaBusWait.Cycles3 => 3,
            _ => 0,
        };

        internal static FpgaBusWait FromCycles(byte cycles) => cycles switch
        {
            2 => FpgaBusWait.Cycles2,
            3 => FpgaBusWait.Cycles3,
            _ => throw new Autd3Exception($"unknown fpga bus wait {cycles}"),
        };
    }
}
