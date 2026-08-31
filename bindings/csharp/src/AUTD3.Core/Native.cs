using System;
using System.Runtime.InteropServices;
using System.Text;

namespace AUTD3
{
    internal static class NativeCore
    {
        private const string Lib = "autd3_core";

        static NativeCore() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [StructLayout(LayoutKind.Sequential)]
        internal struct Autd3Device
        {
            public float Ox, Oy, Oz;
            public float Rw, Rx, Ry, Rz;
        }

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_geometry_new(Autd3Device[] devices, UIntPtr len);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_geometry_num_devices(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_geometry_center(GeometryHandle geometry, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_geometry_num_transducers(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_device_num_transducers(GeometryHandle geometry, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_core_device_idx(GeometryHandle geometry, UIntPtr dev);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_rotation(GeometryHandle geometry, UIntPtr dev, [Out] float[] outWijk);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_center(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_x(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_y(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_device_direction_axial(GeometryHandle geometry, UIntPtr dev, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_transducer_position(GeometryHandle geometry, UIntPtr dev, UIntPtr tr, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_transducer_direction(GeometryHandle geometry, UIntPtr dev, UIntPtr tr, [Out] float[] outXyz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_geometry_free(IntPtr geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern float autd3_core_phase_radian(byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_divide(ushort divide);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_freq(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_freq_nearest(float hz);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_period(ulong nanos);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_core_sampling_config_period_nearest(ulong nanos);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_sampling_config_divide_value(IntPtr config, out ushort outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_sampling_config_freq_value(IntPtr config, out float outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_core_sampling_config_period_value(IntPtr config, out ulong outValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_core_sampling_config_free(IntPtr config);
    }

    internal static class NativeAbi
    {
        internal const ushort Major = 1;
        internal const ushort Minor = 0;

        internal const int ErrorBufferLength = 1024;

        internal static void Verify(string library, uint actual)
        {
            var expected = ((uint)Major << 16) | Minor;
            if (actual == expected)
            {
                return;
            }
            throw new Autd3Exception(
                $"native library '{library}' reports C ABI version {actual >> 16}.{actual & 0xFFFF}, " +
                $"but this binding requires {Major}.{Minor}. " +
                "The managed package and the native library are from different releases.");
        }
    }

    internal static class LinkOptionNative
    {
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int SetDurationFn(IntPtr option, ulong ns);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int GetDurationFn(IntPtr option, out ulong ns);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int SetOptionalDurationFn(IntPtr option, [MarshalAs(UnmanagedType.I1)] bool hasValue, ulong ns);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate int GetOptionalDurationFn(IntPtr option, [MarshalAs(UnmanagedType.I1)] out bool hasValue, out ulong ns);

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        internal delegate IntPtr OpenFn(IntPtr option, byte[] outErr, UIntPtr outErrLen);

        internal static void Apply(string field, int code)
        {
            if (code != 0)
            {
                throw new Autd3Exception($"`{field}` is out of the range the native library accepts");
            }
        }

        internal static void SetDuration(IntPtr option, string field, TimeSpan? value, SetDurationFn set)
        {
            if (value is { } v)
            {
                Apply(field, set(option, ToNanos(v)));
            }
        }

        internal static void SetOptionalDuration(IntPtr option, string field, TimeSpan? value, SetOptionalDurationFn set)
        {
            Apply(field, set(option, value.HasValue, value.HasValue ? ToNanos(value.Value) : 0UL));
        }

        internal static TimeSpan GetDuration(IntPtr option, GetDurationFn get)
        {
            Apply("preset", get(option, out var ns));
            return FromNanos(ns);
        }

        internal static TimeSpan? GetOptionalDuration(IntPtr option, GetOptionalDurationFn get)
        {
            Apply("preset", get(option, out var hasValue, out var ns));
            return hasValue ? FromNanos(ns) : (TimeSpan?)null;
        }

        internal static IntPtr TakeOpener(string link, IntPtr option, OpenFn open)
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            var opener = open(option, err, (UIntPtr)err.Length);
            if (opener == IntPtr.Zero)
            {
                var reason = NativeUtil.Utf8(err);
                throw new Autd3Exception(reason.Length == 0
                    ? $"failed to create {link} link"
                    : $"failed to create {link} link: {reason}");
            }
            return opener;
        }

        internal static ulong ToNanos(TimeSpan value) => (ulong)value.Ticks * 100UL;

        internal static TimeSpan FromNanos(ulong ns) => TimeSpan.FromTicks((long)(ns / 100));
    }

    internal static class NativeUtil
    {
        internal static string Utf8(byte[] buffer)
        {
            var n = Array.IndexOf<byte>(buffer, 0);
            if (n < 0)
            {
                n = buffer.Length;
            }
            return Encoding.UTF8.GetString(buffer, 0, n);
        }

        internal static string PtrToString(IntPtr ptr)
        {
            return ptr == IntPtr.Zero ? string.Empty : Marshal.PtrToStringUTF8(ptr) ?? string.Empty;
        }
    }
}
