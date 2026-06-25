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

    public readonly struct LoopBehavior
    {
        internal ushort Rep { get; }

        private LoopBehavior(ushort rep)
        {
            Rep = rep;
        }

        public static LoopBehavior Infinite => new LoopBehavior(0xFFFF);

        public static LoopBehavior Once => new LoopBehavior(0);

        public static LoopBehavior Finite(ushort count)
        {
            if (count == 0)
            {
                throw new Autd3Exception("loop count must be >= 1");
            }
            return new LoopBehavior((ushort)(count - 1));
        }
    }

    public readonly struct TransitionMode
    {
        internal byte Mode { get; }
        internal ulong Value { get; }

        private TransitionMode(byte mode, ulong value)
        {
            Mode = mode;
            Value = value;
        }

        public static TransitionMode SyncIdx => new TransitionMode(0x00, 0);

        public static TransitionMode SysTime(ulong sysTimeNs) => new TransitionMode(0x01, sysTimeNs);

        public static TransitionMode Gpio(GpioIn gpio) => new TransitionMode(0x02, (byte)gpio);

        public static TransitionMode Ext => new TransitionMode(0xF0, 0);

        public static TransitionMode Immediate => new TransitionMode(0xFF, 0);
    }
}
