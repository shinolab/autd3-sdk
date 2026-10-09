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

        public void Validate(Geometry geometry)
        {
            if (Mask is null)
            {
                return;
            }
            if (Mask.Length != geometry.NumDevices)
            {
                throw new Autd3Exception(
                    $"the mask has {Mask.Length} device slots but the geometry has {geometry.NumDevices} devices",
                    Autd3ErrorCode.InvalidArgument);
            }
            var dev = 0;
            foreach (var device in geometry)
            {
                var got = Mask[dev]?.Length ?? 0;
                if (got != device.NumTransducers)
                {
                    throw new Autd3Exception(
                        $"the mask slot for device {dev} has {got} transducers but the device has {device.NumTransducers}",
                        Autd3ErrorCode.InvalidArgument);
                }
                dev++;
            }
        }

        public bool IsEnabled(int device, int transducer) => Mask is null || Mask[device][transducer];

        public int NumEnabled(Geometry geometry)
        {
            if (Mask is null)
            {
                return geometry.NumTransducers;
            }
            var count = 0;
            foreach (var device in Mask)
            {
                foreach (var enabled in device)
                {
                    if (enabled)
                    {
                        count++;
                    }
                }
            }
            return count;
        }
    }

    public sealed class TransducerGroups<TKey> where TKey : struct
    {
        private readonly int[] _numTransducers;
        private readonly List<TKey> _keys = new List<TKey>();
        private readonly Dictionary<TKey, int> _lookup = new Dictionary<TKey, int>();

        internal int[] FlatIndices { get; }

        public IReadOnlyList<TKey> Keys => _keys;

        public int NumDevices => _numTransducers.Length;

        public TransducerGroups(Geometry geometry, Func<Device, int, TKey?> key)
        {
            _numTransducers = new int[geometry.NumDevices];
            FlatIndices = new int[geometry.NumTransducers];
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
                        FlatIndices[k++] = -1;
                        continue;
                    }
                    if (!_lookup.TryGetValue(value.Value, out var index))
                    {
                        index = _keys.Count;
                        _lookup.Add(value.Value, index);
                        _keys.Add(value.Value);
                    }
                    FlatIndices[k++] = index;
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
            var index = FlatIndices[Offset(device) + transducer];
            return index < 0 ? (TKey?)null : _keys[index];
        }

        public int? Index(int device, int transducer)
        {
            if (transducer < 0 || transducer >= NumTransducers(device))
            {
                throw new ArgumentOutOfRangeException(nameof(transducer));
            }
            var index = FlatIndices[Offset(device) + transducer];
            return index < 0 ? (int?)null : index;
        }

        public IReadOnlyList<int?> Indices(int device)
        {
            var indices = new int?[NumTransducers(device)];
            var offset = Offset(device);
            for (var tr = 0; tr < indices.Length; tr++)
            {
                var index = FlatIndices[offset + tr];
                indices[tr] = index < 0 ? (int?)null : index;
            }
            return indices;
        }

        public int NumTransducers(int device)
        {
            if (device < 0 || device >= _numTransducers.Length)
            {
                throw new ArgumentOutOfRangeException(nameof(device));
            }
            return _numTransducers[device];
        }

        public int NumTransducersIn(TKey key)
        {
            if (!_lookup.TryGetValue(key, out var index))
            {
                return 0;
            }
            var count = 0;
            foreach (var i in FlatIndices)
            {
                if (i == index)
                {
                    count++;
                }
            }
            return count;
        }

        public IEnumerable<(TKey Key, TransducerMask Mask)> Masks()
        {
            foreach (var key in _keys)
            {
                yield return (key, MaskOf(_lookup[key]));
            }
        }

        private int Offset(int device)
        {
            var offset = 0;
            for (var dev = 0; dev < device; dev++)
            {
                offset += _numTransducers[dev];
            }
            return offset;
        }

        public TransducerMask? Mask(TKey key) =>
            _lookup.TryGetValue(key, out var index) ? MaskOf(index) : null;

        internal TransducerMask MaskOf(int index)
        {
            var mask = new bool[_numTransducers.Length][];
            var k = 0;
            for (var dev = 0; dev < mask.Length; dev++)
            {
                mask[dev] = new bool[_numTransducers[dev]];
                for (var tr = 0; tr < mask[dev].Length; tr++)
                {
                    mask[dev][tr] = FlatIndices[k++] == index;
                }
            }
            return TransducerMask.Masked(mask);
        }
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
        internal static extern IntPtr autd3_core_geometry_phase_buffer(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_phase_buffer_from_array(byte[] src, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_phase_buffer_num_devices(PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_phase_buffer_num_transducers(PhaseBufferHandle buffer, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_phase_buffer_get(PhaseBufferHandle buffer, UIntPtr dev, UIntPtr tr, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_phase_buffer_set(PhaseBufferHandle buffer, UIntPtr dev, UIntPtr tr, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_phase_buffer_copy_to(PhaseBufferHandle buffer, [Out] byte[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_phase_buffer_free(IntPtr buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_intensity_buffer(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_intensity_buffer_from_array(byte[] src, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_intensity_buffer_num_devices(IntensityBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_intensity_buffer_num_transducers(IntensityBufferHandle buffer, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_intensity_buffer_get(IntensityBufferHandle buffer, UIntPtr dev, UIntPtr tr, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_intensity_buffer_set(IntensityBufferHandle buffer, UIntPtr dev, UIntPtr tr, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_intensity_buffer_copy_to(IntensityBufferHandle buffer, [Out] byte[] dst, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_intensity_buffer_free(IntPtr buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus(GeometryHandle geometry, float[] target, float wavelengthMm, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_device(GeometryHandle geometry, UIntPtr dev, float[] target, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_focus_transducer(float[] position, float[] target, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane(GeometryHandle geometry, float[] dir, float wavelengthMm, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_device(GeometryHandle geometry, UIntPtr dev, float[] dir, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_plane_transducer(float[] position, float[] dir, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel(GeometryHandle geometry, float[] apex, float[] dir, float thetaRad, float wavelengthMm, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_device(GeometryHandle geometry, UIntPtr dev, float[] apex, float[] dir, float thetaRad, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_bessel_transducer(float[] position, float[] apex, float[] dir, float thetaRad, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase(GeometryHandle geometry, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_phase_transducer(float[] position, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_intensity(GeometryHandle geometry, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, IntensityBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_laguerre_gaussian_intensity_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, uint p, int l, float waistMm, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase(GeometryHandle geometry, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_phase_transducer(float[] position, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, out byte @out);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_intensity(GeometryHandle geometry, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, IntensityBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_hermite_gaussian_intensity_device(GeometryHandle geometry, UIntPtr dev, float[] target, float[] axis, float[] xDir, uint m, uint n, float waistMm, float wavelengthMm, [In, Out] byte[] dst);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_intensity(byte intensity, IntensityBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_set_phase(byte phase, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_add_phase(byte phase, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_phase(GeometryHandle geometry, int[] keys, IntPtr[] sources, UIntPtr numSources, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_copy_phase(GeometryHandle geometry, int[] indices, int index, PhaseBufferHandle source, PhaseBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_intensity(GeometryHandle geometry, int[] keys, IntPtr[] sources, UIntPtr numSources, IntensityBufferHandle buffer);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_pattern_group_copy_intensity(GeometryHandle geometry, int[] indices, int index, IntensityBufferHandle source, IntensityBufferHandle buffer);


        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_pattern(byte bank, PhaseBufferHandle phases, IntPtr intensities, byte uniformIntensity, byte transitionMode, ulong transitionValue);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_pattern_buffer(byte bank, ushort index, PhaseBufferHandle phases, IntPtr intensities, byte uniformIntensity);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_write_pattern_phase(byte bank, ushort index, byte depth, byte intensity, IntPtr[] patterns, UIntPtr numPatterns);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_pattern(byte bank, IntPtr samplingConfig, uint size, ushort rep);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_config_foci_stm(byte bank, IntPtr samplingConfig, uint size, byte numFoci, float soundSpeedMPerS, ushort rep);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_op_activate_pattern_bank(byte bank, byte transitionMode, ulong transitionValue);

        [DllImport(ClientLib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_phase_depth_max_count(byte depth, out UIntPtr @out);
    }

    internal sealed class PhaseBufferHandle : Autd3SafeHandle
    {
        internal PhaseBufferHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativePattern.autd3_phase_buffer_free(handle);
            return true;
        }
    }

    internal sealed class IntensityBufferHandle : Autd3SafeHandle
    {
        internal IntensityBufferHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativePattern.autd3_intensity_buffer_free(handle);
            return true;
        }
    }

    public readonly struct DevicePhases : IEnumerable<Phase>
    {
        private readonly PhaseBufferHandle _buffer;
        private readonly UIntPtr _dev;

        internal DevicePhases(PhaseBufferHandle buffer, UIntPtr dev)
        {
            _buffer = buffer;
            _dev = dev;
        }

        public int NumTransducers => (int)NativePattern.autd3_phase_buffer_num_transducers(_buffer, _dev);

        public Phase this[int tr]
        {
            get
            {
                if (NativePattern.autd3_phase_buffer_get(_buffer, _dev, (UIntPtr)tr, out var v) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
                return new Phase(v);
            }
            set
            {
                if (NativePattern.autd3_phase_buffer_set(_buffer, _dev, (UIntPtr)tr, value.Value) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
            }
        }

        public IEnumerator<Phase> GetEnumerator()
        {
            var count = NumTransducers;
            for (var i = 0; i < count; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }

    public readonly struct DeviceIntensities : IEnumerable<Intensity>
    {
        private readonly IntensityBufferHandle _buffer;
        private readonly UIntPtr _dev;

        internal DeviceIntensities(IntensityBufferHandle buffer, UIntPtr dev)
        {
            _buffer = buffer;
            _dev = dev;
        }

        public int NumTransducers => (int)NativePattern.autd3_intensity_buffer_num_transducers(_buffer, _dev);

        public Intensity this[int tr]
        {
            get
            {
                if (NativePattern.autd3_intensity_buffer_get(_buffer, _dev, (UIntPtr)tr, out var v) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
                return new Intensity(v);
            }
            set
            {
                if (NativePattern.autd3_intensity_buffer_set(_buffer, _dev, (UIntPtr)tr, value.Value) != 0)
                    throw new ArgumentOutOfRangeException(nameof(tr));
            }
        }

        public IEnumerator<Intensity> GetEnumerator()
        {
            var count = NumTransducers;
            for (var i = 0; i < count; i++)
            {
                yield return this[i];
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();
    }

    public sealed class PhaseBuffer : IDisposable, IEnumerable<DevicePhases>
    {
        private readonly PhaseBufferHandle _handle;

        internal PhaseBufferHandle Handle => _handle;

        internal PhaseBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create phase buffer");
            }
            _handle = new PhaseBufferHandle(handle);
        }

        public static PhaseBuffer FromArray(Phase[][] phases)
        {
            var flat = BufferArray.Flatten(phases, p => p.Value, "phases");
            return new PhaseBuffer(NativePattern.autd3_phase_buffer_from_array(flat, (UIntPtr)phases.Length));
        }

        public int NumDevices => (int)NativePattern.autd3_phase_buffer_num_devices(Handle);

        public Phase[][] ToArray()
        {
            var result = new Phase[NumDevices][];
            var total = 0;
            for (var dev = 0; dev < result.Length; dev++)
            {
                result[dev] = new Phase[(int)NativePattern.autd3_phase_buffer_num_transducers(Handle, (UIntPtr)dev)];
                total += result[dev].Length;
            }
            var flat = new byte[total];
            if (NativePattern.autd3_phase_buffer_copy_to(Handle, flat, (UIntPtr)flat.Length) != 0)
            {
                throw new Autd3Exception("failed to copy the buffer");
            }
            var k = 0;
            foreach (var device in result)
            {
                for (var tr = 0; tr < device.Length; tr++)
                {
                    device[tr] = new Phase(flat[k++]);
                }
            }
            return result;
        }

        public DevicePhases this[int dev]
        {
            get
            {
                if (dev < 0 || dev >= NumDevices)
                    throw new ArgumentOutOfRangeException(nameof(dev));
                return new DevicePhases(Handle, (UIntPtr)dev);
            }
        }

        public IEnumerator<DevicePhases> GetEnumerator()
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

    public sealed class IntensityBuffer : IDisposable, IEnumerable<DeviceIntensities>
    {
        private readonly IntensityBufferHandle _handle;

        internal IntensityBufferHandle Handle => _handle;

        internal IntensityBuffer(IntPtr handle)
        {
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create intensity buffer");
            }
            _handle = new IntensityBufferHandle(handle);
        }

        public static IntensityBuffer FromArray(Intensity[][] intensities)
        {
            var flat = BufferArray.Flatten(intensities, i => i.Value, "intensities");
            return new IntensityBuffer(NativePattern.autd3_intensity_buffer_from_array(flat, (UIntPtr)intensities.Length));
        }

        public int NumDevices => (int)NativePattern.autd3_intensity_buffer_num_devices(Handle);

        public Intensity[][] ToArray()
        {
            var result = new Intensity[NumDevices][];
            var total = 0;
            for (var dev = 0; dev < result.Length; dev++)
            {
                result[dev] = new Intensity[(int)NativePattern.autd3_intensity_buffer_num_transducers(Handle, (UIntPtr)dev)];
                total += result[dev].Length;
            }
            var flat = new byte[total];
            if (NativePattern.autd3_intensity_buffer_copy_to(Handle, flat, (UIntPtr)flat.Length) != 0)
            {
                throw new Autd3Exception("failed to copy the buffer");
            }
            var k = 0;
            foreach (var device in result)
            {
                for (var tr = 0; tr < device.Length; tr++)
                {
                    device[tr] = new Intensity(flat[k++]);
                }
            }
            return result;
        }

        public DeviceIntensities this[int dev]
        {
            get
            {
                if (dev < 0 || dev >= NumDevices)
                    throw new ArgumentOutOfRangeException(nameof(dev));
                return new DeviceIntensities(Handle, (UIntPtr)dev);
            }
        }

        public IEnumerator<DeviceIntensities> GetEnumerator()
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

    internal static class BufferArray
    {
        internal static byte[] Flatten<T>(T[][] src, Func<T, byte> value, string name)
        {
            if (src == null)
            {
                throw new ArgumentNullException(name);
            }
            var flat = new byte[src.Length * Autd3.NumTransducers];
            for (var d = 0; d < src.Length; d++)
            {
                if (src[d] == null || src[d].Length != Autd3.NumTransducers)
                {
                    throw new Autd3Exception($"each device requires {Autd3.NumTransducers} {name}");
                }
                for (var t = 0; t < Autd3.NumTransducers; t++)
                {
                    flat[d * Autd3.NumTransducers + t] = value(src[d][t]);
                }
            }
            return flat;
        }
    }

    public static class GeometryPatternBufferExtensions
    {
        public static PhaseBuffer PhaseBuffer(this Geometry geometry) =>
            new PhaseBuffer(NativePattern.autd3_core_geometry_phase_buffer(geometry.Handle));

        public static IntensityBuffer IntensityBuffer(this Geometry geometry) =>
            new IntensityBuffer(NativePattern.autd3_core_geometry_intensity_buffer(geometry.Handle));
    }

    public readonly struct PatternIntensity
    {
        private readonly IntensityBuffer? _buffer;
        private readonly Intensity? _uniform;

        public PatternIntensity(Intensity uniform)
        {
            _buffer = null;
            _uniform = uniform;
        }

        public PatternIntensity(IntensityBuffer buffer)
        {
            _buffer = buffer ?? throw new ArgumentNullException(nameof(buffer));
            _uniform = null;
        }

        internal IntensityBuffer? Buffer => _buffer;

        internal byte Uniform => (_uniform ?? Intensity.Max).Value;

        public static implicit operator PatternIntensity(Intensity uniform) => new PatternIntensity(uniform);

        public static implicit operator PatternIntensity(IntensityBuffer buffer) => new PatternIntensity(buffer);
    }

    public sealed class Pattern : ICommand
    {
        private readonly PatternBank _bank;
        private readonly PhaseBuffer _phases;
        private readonly PatternIntensity _intensities;
        private readonly TransitionMode _transitionMode;

        public Pattern(PhaseBuffer phases, PatternIntensity intensities, TransitionMode? transitionMode = null)
            : this(PatternBank.B0, phases, intensities, transitionMode)
        {
        }

        public Pattern(PatternBank bank, PhaseBuffer phases, PatternIntensity intensities, TransitionMode? transitionMode = null)
        {
            _bank = bank;
            _phases = phases;
            _intensities = intensities;
            _transitionMode = transitionMode ?? TransitionMode.Immediate;
        }

        IntPtr ICommand.CreateOp(Geometry geometry)
        {
            using var intensityLease = new HandleLease(_intensities.Buffer?.Handle);
            return NativePattern.autd3_op_pattern((byte)_bank, _phases.Handle, intensityLease.Pointer, _intensities.Uniform, _transitionMode.Mode, _transitionMode.Value);
        }


        public static Length Wavelength(Velocity soundSpeed) =>
            new Length(NativePattern.autd3_pattern_wavelength(soundSpeed.MmS));


        public static void Focus(Geometry geometry, Vector3 target, Length wavelength, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_focus(geometry.Handle, Coords.PointArray(target), wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("focus failed (buffer device count must match geometry)");
            }
        }

        public static void FocusDevice(Device device, Vector3 target, Length wavelength, Phase[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_focus_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("focus_device failed");
            }
            FromNative(native, dst);
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

        public static void Plane(Geometry geometry, Vector3 dir, Length wavelength, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_plane(geometry.Handle, Coords.DirArray(dir), wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("plane failed (buffer device count must match geometry)");
            }
        }

        public static void PlaneDevice(Device device, Vector3 dir, Length wavelength, Phase[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_plane_device(device.GeometryHandle, device.DeviceIndex,
                Coords.DirArray(dir), wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("plane_device failed");
            }
            FromNative(native, dst);
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

        public static void Bessel(Geometry geometry, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_bessel(geometry.Handle, Coords.PointArray(apex), Coords.DirArray(dir), theta.Rad, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("bessel failed (buffer device count must match geometry)");
            }
        }

        public static void BesselDevice(Device device, Vector3 apex, Vector3 dir, Angle theta, Length wavelength, Phase[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_bessel_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(apex),
                Coords.DirArray(dir), theta.Rad, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("bessel_device failed");
            }
            FromNative(native, dst);
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

        public static void LaguerreGaussianPhase(Geometry geometry, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_laguerre_gaussian_phase(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis),
                option.P, option.L, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_phase failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void LaguerreGaussianPhaseDevice(Device device, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, Phase[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_laguerre_gaussian_phase_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), option.P, option.L, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_phase_device failed (waist must be positive)");
            }
            FromNative(native, dst);
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

        public static void LaguerreGaussianIntensity(Geometry geometry, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, IntensityBuffer dst)
        {
            if (NativePattern.autd3_pattern_laguerre_gaussian_intensity(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis),
                option.P, option.L, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_intensity failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void LaguerreGaussianIntensityDevice(Device device, Vector3 target, Vector3 axis, LaguerreGaussianOption option, Length wavelength, Intensity[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_laguerre_gaussian_intensity_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), option.P, option.L, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("laguerre_gaussian_intensity_device failed (waist must be positive)");
            }
            FromNative(native, dst);
        }

        public static void HermiteGaussianPhase(Geometry geometry, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_hermite_gaussian_phase(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir),
                option.M, option.N, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_phase failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void HermiteGaussianPhaseDevice(Device device, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, Phase[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_hermite_gaussian_phase_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir), option.M, option.N, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_phase_device failed (waist must be positive)");
            }
            FromNative(native, dst);
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

        public static void HermiteGaussianIntensity(Geometry geometry, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, IntensityBuffer dst)
        {
            if (NativePattern.autd3_pattern_hermite_gaussian_intensity(geometry.Handle, Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir),
                option.M, option.N, option.Waist.Mm, wavelength.Mm, dst.Handle) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_intensity failed (waist must be positive and buffer device count must match geometry)");
            }
        }

        public static void HermiteGaussianIntensityDevice(Device device, Vector3 target, Vector3 axis, Vector3 xDir, HermiteGaussianOption option, Length wavelength, Intensity[] dst)
        {
            var native = ToNative(dst);
            if (NativePattern.autd3_pattern_hermite_gaussian_intensity_device(device.GeometryHandle, device.DeviceIndex,
                Coords.PointArray(target), Coords.DirArray(axis), Coords.DirArray(xDir), option.M, option.N, option.Waist.Mm, wavelength.Mm, native) != 0)
            {
                throw new Autd3Exception("hermite_gaussian_intensity_device failed (waist must be positive)");
            }
            FromNative(native, dst);
        }

        public static void SetIntensity(Intensity intensity, IntensityBuffer dst)
        {
            if (NativePattern.autd3_pattern_set_intensity(intensity.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("set_intensity failed");
            }
        }

        public static void SetPhase(Phase phase, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_set_phase(phase.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("set_phase failed");
            }
        }

        public static void AddPhase(Phase phase, PhaseBuffer dst)
        {
            if (NativePattern.autd3_pattern_add_phase(phase.Value, dst.Handle) != 0)
            {
                throw new Autd3Exception("add_phase failed");
            }
        }

        public static void Group<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Func<TKey, PhaseBuffer> source, PhaseBuffer dst) where TKey : struct
        {
            using var lease = GroupSources(geometry, groups, key => source(key)?.Handle, dst.Handle);
            if (NativePattern.autd3_pattern_group_phase(geometry.Handle, groups.FlatIndices, lease.Pointers, (UIntPtr)groups.Keys.Count, dst.Handle) != 0)
            {
                throw new Autd3Exception("group failed (every buffer must match the geometry)");
            }
        }

        public static void Group<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Func<TKey, IntensityBuffer> source, IntensityBuffer dst) where TKey : struct
        {
            using var lease = GroupSources(geometry, groups, key => source(key)?.Handle, dst.Handle);
            if (NativePattern.autd3_pattern_group_intensity(geometry.Handle, groups.FlatIndices, lease.Pointers, (UIntPtr)groups.Keys.Count, dst.Handle) != 0)
            {
                throw new Autd3Exception("group failed (every buffer must match the geometry)");
            }
        }

        private static HandleArray GroupSources<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Func<TKey, SafeHandle?> source, SafeHandle dst) where TKey : struct
        {
            if (groups.FlatIndices.Length != geometry.NumTransducers)
            {
                throw new Autd3Exception("groups must be built from the same geometry");
            }
            var keys = groups.Keys;
            var handles = new SafeHandle[keys.Count];
            for (var i = 0; i < keys.Count; i++)
            {
                var handle = source(keys[i]);
                if (handle == null)
                {
                    throw new Autd3Exception($"no source was given for the key {keys[i]}");
                }
                if (ReferenceEquals(handle, dst))
                {
                    throw new Autd3Exception("dst must not be one of the sources");
                }
                handles[i] = handle;
            }
            return new HandleArray(handles);
        }

        public static void GroupCompute<TKey>(Geometry geometry, TransducerGroups<TKey> groups, Action<TKey, TransducerMask, PhaseBuffer, IntensityBuffer> compute, PhaseBuffer phases, IntensityBuffer intensities) where TKey : struct
        {
            if (compute == null)
            {
                throw new ArgumentNullException(nameof(compute));
            }
            if (groups.FlatIndices.Length != geometry.NumTransducers)
            {
                throw new Autd3Exception("groups must be built from the same geometry");
            }
            if (phases.NumDevices != geometry.NumDevices || intensities.NumDevices != geometry.NumDevices)
            {
                throw new Autd3Exception("group_compute failed (dst must match the geometry)");
            }
            using var scratchPhases = geometry.PhaseBuffer();
            using var scratchIntensities = geometry.IntensityBuffer();
            var keys = groups.Keys;
            for (var i = 0; i < keys.Count; i++)
            {
                SetPhase(Phase.Zero, scratchPhases);
                SetIntensity(Intensity.Max, scratchIntensities);
                compute(keys[i], groups.MaskOf(i), scratchPhases, scratchIntensities);
                if (NativePattern.autd3_pattern_group_copy_phase(geometry.Handle, groups.FlatIndices, i, scratchPhases.Handle, phases.Handle) != 0
                    || NativePattern.autd3_pattern_group_copy_intensity(geometry.Handle, groups.FlatIndices, i, scratchIntensities.Handle, intensities.Handle) != 0)
                {
                    throw new Autd3Exception("group_compute failed (dst must match the geometry)");
                }
            }
        }

        private static byte[] ToNative(Phase[] dst) => ToNative(dst, p => p.Value);

        private static byte[] ToNative(Intensity[] dst) => ToNative(dst, i => i.Value);

        private static byte[] ToNative<T>(T[] dst, Func<T, byte> value)
        {
            if (dst == null)
            {
                throw new ArgumentNullException(nameof(dst));
            }
            if (dst.Length != Autd3.NumTransducers)
            {
                throw new Autd3Exception($"dst requires {Autd3.NumTransducers} elements");
            }
            var native = new byte[dst.Length];
            for (var i = 0; i < dst.Length; i++)
            {
                native[i] = value(dst[i]);
            }
            return native;
        }

        private static void FromNative(byte[] native, Phase[] dst)
        {
            for (var i = 0; i < dst.Length; i++)
            {
                dst[i] = new Phase(native[i]);
            }
        }

        private static void FromNative(byte[] native, Intensity[] dst)
        {
            for (var i = 0; i < dst.Length; i++)
            {
                dst[i] = new Intensity(native[i]);
            }
        }
    }
}
