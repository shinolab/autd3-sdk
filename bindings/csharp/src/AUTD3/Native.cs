using System;
using System.Runtime.InteropServices;

namespace AUTD3
{


    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void CompletionCallback(int code, IntPtr value, IntPtr msg, IntPtr userData);

    internal static class NativeClient
    {
        private const string Lib = "autd3capi";

        [DllImport(Lib)]
        internal static extern IntPtr autd3_client_config_new([MarshalAs(UnmanagedType.I1)] bool lowLatency);

        [DllImport(Lib)]
        internal static extern void autd3_client_config_free(IntPtr config);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_datagram_builder_new(UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern void autd3_datagram_builder_push(IntPtr builder, IntPtr op);

        [DllImport(Lib)]
        internal static extern void autd3_datagram_builder_free(IntPtr builder);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_datagram_builder_build(IntPtr builder, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_datagrams_num_frames(IntPtr datagrams);

        [DllImport(Lib)]
        internal static extern void autd3_datagrams_free(IntPtr datagrams);

        [DllImport(Lib)]
        internal static extern void autd3_client_open(IntPtr geometry, IntPtr link, IntPtr config, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_client_num_devices(IntPtr client);

        [DllImport(Lib)]
        internal static extern void autd3_client_send_checked(IntPtr client, IntPtr datagrams, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_client_stop(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_client_close(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_client_free(IntPtr client);

        [DllImport(Lib)]
        internal static extern void autd3_client_read_firmware_version(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_client_read_fpga_state(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_client_read_error_detail(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_byte_array_len(IntPtr array);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_byte_array_data(IntPtr array);

        [DllImport(Lib)]
        internal static extern void autd3_byte_array_free(IntPtr array);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_string_array_len(IntPtr array);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_string_array_get(IntPtr array, UIntPtr index);

        [DllImport(Lib)]
        internal static extern void autd3_string_array_free(IntPtr array);

        [DllImport(Lib)]
        internal static extern void autd3_client_check_status(IntPtr client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_link_status_all_op(IntPtr status);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_link_status_any_lost(IntPtr status);

        [DllImport(Lib)]
        internal static extern ulong autd3_link_status_recoveries(IntPtr status);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_link_status_num_devices(IntPtr status);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_link_status_device_state(IntPtr status, UIntPtr index);

        [DllImport(Lib)]
        internal static extern void autd3_link_status_free(IntPtr status);
    }
}
