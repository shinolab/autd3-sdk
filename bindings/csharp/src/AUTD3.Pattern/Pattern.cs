using System;
using System.Threading;
using System.Collections;
using System.Collections.Generic;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    [StructLayout(LayoutKind.Sequential)]
    internal struct PatternOptionNative
    {
        public byte Intensity;
        public byte PhaseOffset;
    }

    public readonly struct FocusOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public FocusOption() : this(intensity: null)
        {
        }

        public FocusOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct PlaneOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public PlaneOption() : this(intensity: null)
        {
        }

        public PlaneOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct BesselOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public BesselOption() : this(intensity: null)
        {
        }

        public BesselOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct TwinTrapOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public TwinTrapOption() : this(intensity: null)
        {
        }

        public TwinTrapOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    public readonly struct VortexOption
    {
        public Intensity Intensity { get; }
        public Phase PhaseOffset { get; }

        public VortexOption() : this(intensity: null)
        {
        }

        public VortexOption(Intensity? intensity = null, Phase? phaseOffset = null)
        {
            Intensity = intensity ?? Intensity.Max;
            PhaseOffset = phaseOffset ?? Phase.Zero;
        }

        internal PatternOptionNative ToNative() =>
            new PatternOptionNative { Intensity = Intensity.Value, PhaseOffset = PhaseOffset.Value };
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct EmissionNative
    {
        public byte Phase;
        public byte Intensity;
    }

    internal static class NativePattern
    {
        private const string Lib = "autd3_pattern";

        private const string ClientLib = "autd3capi";

        static NativePattern()
        {
            NativeAbi.Verify(Lib, autd3_abi_version());
            NativeAbi.Verify(ClientLib, autd3capi_abi_version());
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(ClientLib, EntryPoint = "autd3_abi_version", CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3capi_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_pattern_wavelength(float soundSpeedMmPerS);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_pattern_buffer(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_pattern_buffer_from_array(EmissionNative[] emissions, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_pattern_buffer_num_devices(PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_pattern_buffer_num_transducers(PatternBufferHandle buffer, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_buffer_get(PatternBufferHandle buffer, UIntPtr dev, UIntPtr tr, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_buffer_set(PatternBufferHandle buffer, UIntPtr dev, UIntPtr tr, EmissionNative emission);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_pattern_buffer_free(IntPtr buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus(GeometryHandle geometry, float[] target, float wavelengthMm, in PatternOptionNative option, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_device(GeometryHandle geometry, UIntPtr dev, float[] target, float wavelengthMm, in PatternOptionNative option, [Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_transducer(float[] position, float[] target, float wavelengthMm, in PatternOptionNative option, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane(GeometryHandle geometry, float[] dir, float wavelengthMm, in PatternOptionNative option, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_device(GeometryHandle geometry, UIntPtr dev, float[] dir, float wavelengthMm, in PatternOptionNative option, [Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_transducer(float[] position, float[] dir, float wavelengthMm, in PatternOptionNative option, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel(GeometryHandle geometry, float[] apex, float[] dir, float thetaRad, float wavelengthMm, in PatternOptionNative option, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_device(GeometryHandle geometry, UIntPtr dev, float[] apex, float[] dir, float thetaRad, float wavelengthMm, in PatternOptionNative option, [Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_transducer(float[] position, float[] apex, float[] dir, float thetaRad, float wavelengthMm, in PatternOptionNative option, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_twin_trap(GeometryHandle geometry, float[] target, float[] normal, float wavelengthMm, in PatternOptionNative option, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_twin_trap_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] normal, float wavelengthMm, in PatternOptionNative option, [Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_twin_trap_transducer(float[] position, float[] target, float[] normal, float wavelengthMm, in PatternOptionNative option, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_vortex(GeometryHandle geometry, float[] target, float[] axis, int order, float wavelengthMm, in PatternOptionNative option, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_vortex_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, int order, float wavelengthMm, in PatternOptionNative option, [Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_vortex_transducer(float[] position, float[] target, float[] axis, int order, float wavelengthMm, in PatternOptionNative option, out EmissionNative @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_uniform(byte phase, byte intensity, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_pattern_null(PatternBufferHandle buffer);


        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_pattern(byte bank, PatternBufferHandle patternBuffer, byte transitionMode, ulong transitionValue, uint transitionMarginNs);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_pattern_buffer(byte bank, ushort index, PatternBufferHandle patternBuffer);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_pattern_compressed(byte bank, uint index, byte format, IntPtr[] patterns, UIntPtr numPatterns);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_pattern(byte bank, IntPtr samplingConfig, uint size, ushort rep);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_foci_stm(byte bank, IntPtr samplingConfig, uint size, byte numFoci, float soundSpeedMPerS, ushort rep);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_change_pattern_bank(byte bank, byte transitionMode, ulong transitionValue, uint transitionMarginNs);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_compression_per_frame(byte format, out UIntPtr @out);
    }

    internal sealed class PatternBufferHandle : Autd3SafeHandle
    {
        internal PatternBufferHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativePattern.autd3_pattern_buffer_free(handle);
            return true;
        }
    }

    public readonly struct DevicePattern : IEnumerable<Emission>
    {
        private readonly PatternBufferHandle _buffer;
        private readonly UIntPtr _dev;

        internal DevicePattern(PatternBufferHandle buffer, UIntPtr dev)
        {
            _buffer = buffer;
            _dev = dev;
        }

        public int NumTransducers => (int)NativePattern.autd3_pattern_buffer_num_transducers(_buffer, _dev);

        public Emission this[int tr]
        {
            get
            {
                if (NativePattern.autd3_pattern_buffer_get(_buffer, _dev, (UIntPtr)tr, out var e) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
                return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
            }
            set
            {
                var native = new EmissionNative { Phase = value.Phase.Value, Intensity = value.Intensity.Value };
                if (NativePattern.autd3_pattern_buffer_set(_buffer, _dev, (UIntPtr)tr, native) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
            }
        }

        public IEnumerator<Emission> GetEnumerator()
        {
            var count = NumTransducers;
            for (var i = 0; i < count; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }

    public sealed class PatternBuffer : IDisposable, IEnumerable<DevicePattern>
    {
        internal const int NumTransducers = 249;

        private readonly PatternBufferHandle _handle;

        internal PatternBufferHandle Handle => _handle;

        internal PatternBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create pattern buffer");
            }
            _handle = new PatternBufferHandle(handle);
        }

        public static PatternBuffer FromArray(Emission[][] emissions)
        {
            var numDevices = emissions.Length;
            var flat = new EmissionNative[numDevices * NumTransducers];
            for (var d = 0; d < numDevices; d++)
            {
                if (emissions[d].Length != NumTransducers)
                {
                    throw new Autd3Exception($"each device requires {NumTransducers} emissions");
                }
                for (var t = 0; t < NumTransducers; t++)
                {
                    flat[d * NumTransducers + t] = new EmissionNative
                    {
                        Phase = emissions[d][t].Phase.Value,
                        Intensity = emissions[d][t].Intensity.Value,
                    };
                }
            }
            return new PatternBuffer(NativePattern.autd3_pattern_buffer_from_array(flat, (UIntPtr)numDevices));
        }

        public int NumDevices => (int)NativePattern.autd3_pattern_buffer_num_devices(Handle);

        public DevicePattern this[int dev]
        {
            get
            {
                if (dev < 0 || dev >= NumDevices)
                    throw new ArgumentOutOfRangeException(nameof(dev));
                return new DevicePattern(Handle, (UIntPtr)dev);
            }
        }

        public IEnumerator<DevicePattern> GetEnumerator()
        {
            var count = NumDevices;
            for (var i = 0; i < count; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose() => _handle.Dispose();
    }

    public static class GeometryPatternBufferExtensions
    {
        public static PatternBuffer PatternBuffer(this Geometry geometry) =>
            new PatternBuffer(NativePattern.autd3_core_geometry_pattern_buffer(geometry.Handle));
    }

    public sealed class Pattern : ICommand
    {
        private readonly PatternBank _bank;
        private readonly PatternBuffer _buffer;
        private readonly TransitionMode _transitionMode;

        public Pattern(PatternBuffer emissions, TransitionMode? transitionMode = null)
            : this(PatternBank.B0, emissions, transitionMode)
        {
        }

        public Pattern(PatternBank bank, PatternBuffer emissions, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _buffer = emissions;
            _transitionMode = transitionMode ?? TransitionMode.Immediate;
        }

        IntPtr ICommand.CreateOp() =>
            NativePattern.autd3_op_pattern((byte)_bank, _buffer.Handle, _transitionMode.Mode, _transitionMode.Value, _transitionMode.MarginNs);


        public static Length Wavelength(Velocity soundSpeed) =>
            new Length(NativePattern.autd3_pattern_wavelength(soundSpeed.MmS));


        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, FocusOption option, PatternBuffer dst)
        {
            var t = Coords.PointArray(target);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_focus(geometry.Handle, t, wavelength.Mm, in o, dst.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }

        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, Intensity intensity, PatternBuffer dst) =>
            Focus(geometry, target, wavelength, new FocusOption(intensity), dst);

        public static void FocusDevice(Device device, Vector3 target, Length wavelength, FocusOption option, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_focus_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), wavelength.Mm, in o, native) != 0)
            {
                throw new Autd3Exception("focus_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Emission FocusTransducer(Vector3 position, Vector3 target, Length wavelength, FocusOption option)
        {
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_focus_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target), wavelength.Mm, in o, out var e) != 0)
            {
                throw new Autd3Exception("focus_transducer failed");
            }
            return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
        }

        public static void Plane(Geometry geometry, Vector3 dir, Length wavelength, PlaneOption option, PatternBuffer dst)
        {
            var d = Coords.DirArray(dir);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_plane(geometry.Handle, d, wavelength.Mm, in o, dst.Handle) != 0)
            {
                throw new Autd3Exception("plane failed (buffer device count must match geometry)");
            }
        }

        public static void PlaneDevice(Device device, Vector3 dir, Length wavelength, PlaneOption option, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_plane_device(device.GeometryHandle, device.DeviceIndex,
                Coords.DirArray(dir), wavelength.Mm, in o, native) != 0)
            {
                throw new Autd3Exception("plane_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Emission PlaneTransducer(Vector3 position, Vector3 dir, Length wavelength, PlaneOption option)
        {
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_plane_transducer(
                Coords.PointArray(position),
                Coords.DirArray(dir), wavelength.Mm, in o, out var e) != 0)
            {
                throw new Autd3Exception("plane_transducer failed");
            }
            return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
        }

        public static void Bessel(Geometry geometry, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, BesselOption option, PatternBuffer dst)
        {
            var a = Coords.PointArray(apex);
            var d = Coords.DirArray(dir);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_bessel(geometry.Handle, a, d, theta.Rad, wavelength.Mm, in o, dst.Handle) != 0)
            {
                throw new Autd3Exception("bessel failed (buffer device count must match geometry)");
            }
        }

        public static void BesselDevice(Device device, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, BesselOption option, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_bessel_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(apex),
                Coords.DirArray(dir), theta.Rad, wavelength.Mm, in o, native) != 0)
            {
                throw new Autd3Exception("bessel_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Emission BesselTransducer(Vector3 position, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, BesselOption option)
        {
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_bessel_transducer(
                Coords.PointArray(position),
                Coords.PointArray(apex),
                Coords.DirArray(dir), theta.Rad, wavelength.Mm, in o, out var e) != 0)
            {
                throw new Autd3Exception("bessel_transducer failed");
            }
            return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
        }

        public static void TwinTrap(Geometry geometry, Vector3 target, Vector3 normal, Length wavelength, TwinTrapOption option, PatternBuffer dst)
        {
            var t = Coords.PointArray(target);
            var n = Coords.DirArray(normal);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_twin_trap(geometry.Handle, t, n, wavelength.Mm, in o, dst.Handle) != 0)
            {
                throw new Autd3Exception("twin_trap failed (buffer device count must match geometry)");
            }
        }

        public static void TwinTrapDevice(Device device, Vector3 target, Vector3 normal, Length wavelength, TwinTrapOption option, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_twin_trap_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(normal), wavelength.Mm, in o, native) != 0)
            {
                throw new Autd3Exception("twin_trap_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Emission TwinTrapTransducer(Vector3 position, Vector3 target, Vector3 normal, Length wavelength, TwinTrapOption option)
        {
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_twin_trap_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target),
                Coords.DirArray(normal), wavelength.Mm, in o, out var e) != 0)
            {
                throw new Autd3Exception("twin_trap_transducer failed");
            }
            return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
        }

        public static void Vortex(Geometry geometry, Vector3 target, Vector3 axis, int order, Length wavelength, VortexOption option, PatternBuffer dst)
        {
            var t = Coords.PointArray(target);
            var a = Coords.DirArray(axis);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_vortex(geometry.Handle, t, a, order, wavelength.Mm, in o, dst.Handle) != 0)
            {
                throw new Autd3Exception("vortex failed (buffer device count must match geometry)");
            }
        }

        public static void VortexDevice(Device device, Vector3 target, Vector3 axis, int order, Length wavelength, VortexOption option, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_vortex_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), order, wavelength.Mm, in o, native) != 0)
            {
                throw new Autd3Exception("vortex_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Emission VortexTransducer(Vector3 position, Vector3 target, Vector3 axis, int order, Length wavelength, VortexOption option)
        {
            var o = option.ToNative();
            if (NativePattern.autd3_pattern_vortex_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target),
                Coords.DirArray(axis), order, wavelength.Mm, in o, out var e) != 0)
            {
                throw new Autd3Exception("vortex_transducer failed");
            }
            return new Emission(new Phase(e.Phase), new Intensity(e.Intensity));
        }

        public static void Uniform(Emission emission, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_uniform(emission.Phase.Value, emission.Intensity.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("uniform failed");
            }
        }

        public static void UniformDevice(Emission emission, Emission[] dst)
        {
            for (var i = 0; i < dst.Length; i++)
            {
                dst[i] = emission;
            }
        }

        public static void Null(PatternBuffer dst) => NativePattern.autd3_pattern_null(dst.Handle);

        public static void NullDevice(Emission[] dst)
        {
            for (var i = 0; i < dst.Length; i++)
            {
                dst[i] = Emission.Null;
            }
        }

        public static void NullTransducer(ref Emission dst) => dst = Emission.Null;

        private static EmissionNative[] ToNativeDst(Emission[] dst)
        {
            if (dst.Length != Autd3.NumTransducers)
            {
                throw new Autd3Exception($"dst requires {Autd3.NumTransducers} emissions");
            }
            return new EmissionNative[dst.Length];
        }

        private static void FromNativeDst(EmissionNative[] native, Emission[] dst)
        {
            for (var i = 0; i < dst.Length; i++)
            {
                dst[i] = new Emission(new Phase(native[i].Phase), new Intensity(native[i].Intensity));
            }
        }
    }
}
