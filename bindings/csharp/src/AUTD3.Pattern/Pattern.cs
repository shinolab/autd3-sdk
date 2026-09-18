using System;
using System.Threading;
using System.Collections;
using System.Collections.Generic;
using System.Numerics;
using System.Runtime.InteropServices;

namespace AUTD3
{
    public readonly struct LaguerreGaussianOption
    {
        public uint P { get; }
        public int L { get; }
        public Length Waist { get; }

        public LaguerreGaussianOption(uint p, int l, Length waist)
        {
            P = p;
            L = l;
            Waist = waist;
        }
    }

    public readonly struct HermiteGaussianOption
    {
        public uint M { get; }
        public uint N { get; }
        public Length Waist { get; }

        public HermiteGaussianOption(uint m, uint n, Length waist)
        {
            M = m;
            N = n;
            Waist = waist;
        }
    }

    public readonly struct TransducerMask
    {
        internal bool[][]? Mask { get; }

        private TransducerMask(bool[][]? mask)
        {
            Mask = mask;
        }

        public static TransducerMask AllEnabled => new TransducerMask(null);

        public static TransducerMask Masked(bool[][] mask) => new TransducerMask(mask);
    }

    public sealed class TransducerGroups<TKey> where TKey : struct
    {
        private readonly int[] _numTransducers;
        private readonly List<TKey> _keys = new List<TKey>();
        private readonly Dictionary<TKey, int> _lookup = new Dictionary<TKey, int>();

        internal int[] Indices { get; }

        public IReadOnlyList<TKey> Keys => _keys;

        public int NumDevices => _numTransducers.Length;

        public TransducerGroups(Geometry geometry, Func<Device, int, TKey?> key)
        {
            _numTransducers = new int[geometry.NumDevices];
            Indices = new int[geometry.NumTransducers];
            var k = 0;
            var dev = 0;
            foreach (var device in geometry)
            {
                var numTransducers = device.NumTransducers;
                _numTransducers[dev++] = numTransducers;
                for (var tr = 0; tr < numTransducers; tr++)
                {
                    var value = key(device, tr);
                    if (value == null)
                    {
                        Indices[k++] = -1;
                        continue;
                    }
                    if (!_lookup.TryGetValue(value.Value, out var index))
                    {
                        index = _keys.Count;
                        _lookup.Add(value.Value, index);
                        _keys.Add(value.Value);
                    }
                    Indices[k++] = index;
                }
            }
        }

        public TKey? Key(int device, int transducer)
        {
            if (device < 0 || device >= _numTransducers.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(device));
            }
            if (transducer < 0 || transducer >= _numTransducers[device])
            {
                throw new ArgumentOutOfRangeException(nameof(transducer));
            }
            var offset = 0;
            for (var dev = 0; dev < device; dev++)
            {
                offset += _numTransducers[dev];
            }
            var index = Indices[offset + transducer];
            return index < 0 ? (TKey?)null : _keys[index];
        }

        public TransducerMask Mask(TKey key)
        {
            if (!_lookup.TryGetValue(key, out var index))
            {
                throw new ArgumentException($"no transducer is assigned the key {key}", nameof(key));
            }
            var mask = new bool[_numTransducers.Length][];
            var k = 0;
            for (var dev = 0; dev < mask.Length; dev++)
            {
                mask[dev] = new bool[_numTransducers[dev]];
                for (var tr = 0; tr < mask[dev].Length; tr++)
                {
                    mask[dev][tr] = Indices[k++] == index;
                }
            }
            return TransducerMask.Masked(mask);
        }
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
        internal static extern int autd3_pattern_focus(GeometryHandle geometry, float[] target, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_device(GeometryHandle geometry, UIntPtr dev, float[] target, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_transducer(float[] position, float[] target, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane(GeometryHandle geometry, float[] dir, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_device(GeometryHandle geometry, UIntPtr dev, float[] dir, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_transducer(float[] position, float[] dir, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel(GeometryHandle geometry, float[] apex, float[] dir, float thetaRad, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_device(GeometryHandle geometry, UIntPtr dev, float[] apex, float[] dir, float thetaRad, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_transducer(float[] position, float[] apex, float[] dir, float thetaRad, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase(GeometryHandle geometry, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase_transducer(float[] position, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_intensity(GeometryHandle geometry, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_intensity_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase(GeometryHandle geometry, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase_transducer(float[] position, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_intensity(GeometryHandle geometry, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_intensity_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, [In, Out] EmissionNative[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_intensity(byte intensity, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_intensity_device(byte intensity, [In, Out] EmissionNative[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_phase(byte phase, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_phase_device(byte phase, [In, Out] EmissionNative[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_phase_and_intensity(byte phase, byte intensity, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_phase_and_intensity_device(byte phase, byte intensity, [In, Out] EmissionNative[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_add_phase(byte phase, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_add_phase_device(byte phase, [In, Out] EmissionNative[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group(GeometryHandle geometry, int[] keys, IntPtr[] sources, UIntPtr numSources, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_null(GeometryHandle geometry, int[] indices, PatternBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_copy(GeometryHandle geometry, int[] indices, int index, PatternBufferHandle source, PatternBufferHandle buffer);


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


        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_focus(geometry.Handle, Coords.PointArray(target), wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }

        public static void FocusDevice(Device device, Vector3 target, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_focus_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("focus_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Phase FocusTransducer(Vector3 position, Vector3 target, Length wavelength)
        {
            if (NativePattern.autd3_pattern_focus_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target), wavelength.Mm, out var p) != 0)
            {
                throw new Autd3Exception("focus_transducer failed");
            }
            return new Phase(p);
        }

        public static void Plane(Geometry geometry, Vector3 dir, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_plane(geometry.Handle, Coords.DirArray(dir), wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("plane failed (buffer device count must match geometry)");
            }
        }

        public static void PlaneDevice(Device device, Vector3 dir, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_plane_device(device.GeometryHandle, device.DeviceIndex,
                Coords.DirArray(dir), wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("plane_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Phase PlaneTransducer(Vector3 position, Vector3 dir, Length wavelength)
        {
            if (NativePattern.autd3_pattern_plane_transducer(
                Coords.PointArray(position),
                Coords.DirArray(dir), wavelength.Mm, out var p) != 0)
            {
                throw new Autd3Exception("plane_transducer failed");
            }
            return new Phase(p);
        }

        public static void Bessel(Geometry geometry, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_bessel(geometry.Handle, Coords.PointArray(apex), Coords.DirArray(dir), theta.Rad, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("bessel failed (buffer device count must match geometry)");
            }
        }

        public static void BesselDevice(Device device, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_bessel_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(apex),
                Coords.DirArray(dir), theta.Rad, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("bessel_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static Phase BesselTransducer(Vector3 position, Vector3 apex, Vector3 dir, Angle theta, Length wavelength)
        {
            if (NativePattern.autd3_pattern_bessel_transducer(
                Coords.PointArray(position),
                Coords.PointArray(apex),
                Coords.DirArray(dir), theta.Rad, wavelength.Mm, out var p) != 0)
            {
                throw new Autd3Exception("bessel_transducer failed");
            }
            return new Phase(p);
        }

        public static void LaguerreGaussianPhase(Geometry geometry, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_laguerre_gaussian_phase(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis),
                option.P, option.L, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_phase failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void LaguerreGaussianPhaseDevice(Device device, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_laguerre_gaussian_phase_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), option.P, option.L, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_phase_device failed (waist must be positive)");
            }
            FromNativeDst(native, dst);
        }

        public static Phase LaguerreGaussianPhaseTransducer(Vector3 position, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength)
        {
            if (NativePattern.autd3_pattern_laguerre_gaussian_phase_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target),
                Coords.DirArray(axis), option.P, option.L, option.Waist.Mm, wavelength.Mm, out var p) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_phase_transducer failed (waist must be positive)");
            }
            return new Phase(p);
        }

        public static void LaguerreGaussianIntensity(Geometry geometry, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_laguerre_gaussian_intensity(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis),
                option.P, option.L, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_intensity failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void LaguerreGaussianIntensityDevice(Device device, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_laguerre_gaussian_intensity_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), option.P, option.L, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_intensity_device failed (waist must be positive)");
            }
            FromNativeDst(native, dst);
        }

        public static void HermiteGaussianPhase(Geometry geometry, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_hermite_gaussian_phase(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir),
                option.M, option.N, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_phase failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void HermiteGaussianPhaseDevice(Device device, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_hermite_gaussian_phase_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir), option.M, option.N, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_phase_device failed (waist must be positive)");
            }
            FromNativeDst(native, dst);
        }

        public static Phase HermiteGaussianPhaseTransducer(Vector3 position, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength)
        {
            if (NativePattern.autd3_pattern_hermite_gaussian_phase_transducer(
                Coords.PointArray(position),
                Coords.PointArray(target),
                Coords.DirArray(axis),
                Coords.DirArray(xDir), option.M, option.N, option.Waist.Mm, wavelength.Mm, out var p) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_phase_transducer failed (waist must be positive)");
            }
            return new Phase(p);
        }

        public static void HermiteGaussianIntensity(Geometry geometry, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_hermite_gaussian_intensity(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir),
                option.M, option.N, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_intensity failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void HermiteGaussianIntensityDevice(Device device, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, Emission[] dst)
        {
            var native = ToNativeDst(dst);
            if (NativePattern.autd3_pattern_hermite_gaussian_intensity_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir), option.M, option.N, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_intensity_device failed (waist must be positive)");
            }
            FromNativeDst(native, dst);
        }

        public static void SetIntensity(Intensity intensity, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_set_intensity(intensity.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("set_intensity failed");
            }
        }

        public static void SetIntensityDevice(Intensity intensity, Emission[] dst)
        {
            var native = ToNativeDstAnyLength(dst);
            if (NativePattern.autd3_pattern_set_intensity_device(intensity.Value, native, (UIntPtr)native.Length) != 0)
            {
                throw new Autd3Exception("set_intensity_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static void SetPhase(Phase phase, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_set_phase(phase.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("set_phase failed");
            }
        }

        public static void SetPhaseDevice(Phase phase, Emission[] dst)
        {
            var native = ToNativeDstAnyLength(dst);
            if (NativePattern.autd3_pattern_set_phase_device(phase.Value, native, (UIntPtr)native.Length) != 0)
            {
                throw new Autd3Exception("set_phase_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static void SetPhaseAndIntensity(Phase phase, Intensity intensity, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_set_phase_and_intensity(phase.Value, intensity.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("set_phase_and_intensity failed");
            }
        }

        public static void SetPhaseAndIntensityDevice(Phase phase, Intensity intensity, Emission[] dst)
        {
            var native = ToNativeDstAnyLength(dst);
            if (NativePattern.autd3_pattern_set_phase_and_intensity_device(phase.Value, intensity.Value, native, (UIntPtr)native.Length) != 0)
            {
                throw new Autd3Exception("set_phase_and_intensity_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static void AddPhase(Phase phase, PatternBuffer dst)
        {
            if (NativePattern.autd3_pattern_add_phase(phase.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("add_phase failed");
            }
        }

        public static void AddPhaseDevice(Phase phase, Emission[] dst)
        {
            var native = ToNativeDstAnyLength(dst);
            if (NativePattern.autd3_pattern_add_phase_device(phase.Value, native, (UIntPtr)native.Length) != 0)
            {
                throw new Autd3Exception("add_phase_device failed");
            }
            FromNativeDst(native, dst);
        }

        public static void Group<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Func<TKey, PatternBuffer> source, PatternBuffer dst) where TKey : struct
        {
            if (groups.Indices.Length != geometry.NumTransducers)
            {
                throw new Autd3Exception("groups must be built from the same geometry");
            }
            var keys = groups.Keys;
            var handles = new SafeHandle[keys.Count];
            for (var i = 0; i < keys.Count; i++)
            {
                var buffer = source(keys[i]);
                if (buffer == null)
                {
                    throw new Autd3Exception($"no source was given for the key {keys[i]}");
                }
                if (ReferenceEquals(buffer, dst))
                {
                    throw new Autd3Exception("dst must not be one of the sources");
                }
                handles[i] = buffer.Handle;
            }
            using var lease = new HandleArray(handles);
            if (NativePattern.autd3_pattern_group(geometry.Handle, groups.Indices, lease.Pointers, (UIntPtr)keys.Count, dst.Handle) != 0)
            {
                throw new Autd3Exception("group failed (every buffer must match the geometry)");
            }
        }

        public static void GroupCompute<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Action<TKey, TransducerMask, PatternBuffer> compute, PatternBuffer dst) where TKey : struct
        {
            if (compute == null)
            {
                throw new ArgumentNullException(nameof(compute));
            }
            if (groups.Indices.Length != geometry.NumTransducers)
            {
                throw new Autd3Exception("groups must be built from the same geometry");
            }
            if (NativePattern.autd3_pattern_group_null(geometry.Handle, groups.Indices, dst.Handle) != 0)
            {
                throw new Autd3Exception("group_compute failed (dst must match the geometry)");
            }
            using var scratch = geometry.PatternBuffer();
            var keys = groups.Keys;
            for (var i = 0; i < keys.Count; i++)
            {
                SetPhaseAndIntensity(Phase.Zero, Intensity.Max, scratch);
                compute(keys[i], groups.Mask(keys[i]), scratch);
                if (NativePattern.autd3_pattern_group_copy(geometry.Handle, groups.Indices, i, scratch.Handle, dst.Handle) != 0)
                {
                    throw new Autd3Exception("group_compute failed (dst must match the geometry)");
                }
            }
        }

        private static EmissionNative[] ToNativeDst(Emission[] dst)
        {
            if (dst == null)
            {
                throw new ArgumentNullException(nameof(dst));
            }
            if (dst.Length != Autd3.NumTransducers)
            {
                throw new Autd3Exception($"dst requires {Autd3.NumTransducers} emissions");
            }
            return ToNativeDstAnyLength(dst);
        }

        private static EmissionNative[] ToNativeDstAnyLength(Emission[] dst)
        {
            if (dst == null)
            {
                throw new ArgumentNullException(nameof(dst));
            }
            var native = new EmissionNative[dst.Length];
            for (var i = 0; i < dst.Length; i++)
            {
                native[i] = new EmissionNative { Phase = dst[i].Phase.Value, Intensity = dst[i].Intensity.Value };
            }
            return native;
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
