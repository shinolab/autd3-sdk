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
        public ushort Rep { get; }

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

        public static TransitionMode SysTime(SysTime sysTime) => new TransitionMode(0x01, sysTime.Nanos);

        public static TransitionMode Gpio(GpioIn gpio) => new TransitionMode(0x02, (byte)gpio);

        public static TransitionMode Ext => new TransitionMode(0xF0, 0);

        public static TransitionMode Later => new TransitionMode(0xFE, 0);

        public static TransitionMode Immediate => new TransitionMode(0xFF, 0);

        public bool IsLater => Mode == 0xFE;
    }

    public enum Telemetry : byte
    {
        FifoDrop = 0x00,
        Dedup = 0x01,
        SeqMismatch = 0x02,
        DispatchError = 0x03,
        Processed = 0x04,
        Failsafe = 0x05,
        SyncResync = 0x06,
        PtpUnlockFailsafe = 0x07,
        SendFailure = 0x08,
        BootFailure = 0x09,
    }

    public enum FpgaBusWait : byte
    {
        Cycles2 = 2,
        Cycles3 = 3,
    }
}
