using System;
using System.Numerics;

namespace AUTD3
{
    public sealed class Autd3Exception : Exception
    {
        public Autd3Exception(string message) : base(message)
        {
        }
    }


    public readonly struct Device
    {
        public const float Width = 192.0f;
        public const float Height = 151.4f;

        public Vector3 Origin { get; }
        public Quaternion Rotation { get; }

        public Device(Vector3 origin) : this(origin, Quaternion.Identity)
        {
        }

        public Device(Vector3 origin, Quaternion rotation)
        {
            Origin = origin;
            Rotation = rotation;
        }

        internal NativeCore.Autd3Device ToNative()
        {
            return new NativeCore.Autd3Device
            {
                Ox = Origin.X,
                Oy = Origin.Y,
                Oz = Origin.Z,
                Rw = Rotation.W,
                Rx = Rotation.X,
                Ry = Rotation.Y,
                Rz = Rotation.Z,
            };
        }
    }

    public readonly struct Intensity
    {
        public byte Value { get; }

        public Intensity(byte value)
        {
            Value = value;
        }

        public static Intensity Max => new Intensity(0xFF);
        public static Intensity Min => new Intensity(0x00);
    }

    public readonly struct Phase
    {
        public byte Value { get; }

        public Phase(byte value)
        {
            Value = value;
        }

        public static Phase Zero => new Phase(0x00);
        public static Phase Pi => new Phase(0x80);

        public float Radian() => NativeCore.autd3_core_phase_radian(Value);
    }

    public readonly struct Emission
    {
        public Phase Phase { get; }
        public Intensity Intensity { get; }

        public Emission(Phase phase, Intensity intensity)
        {
            Phase = phase;
            Intensity = intensity;
        }
    }
}
