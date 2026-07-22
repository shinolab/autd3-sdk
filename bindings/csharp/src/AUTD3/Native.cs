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
        internal static extern IntPtr autd3_client_config_new(
            [MarshalAs(UnmanagedType.I1)] bool lowLatency,
            uint timeoutCycles,
            UIntPtr maxInflight,
            uint sendIntervalCycles,
            uint maxResyncRounds,
            uint resetResendCycles,
            [MarshalAs(UnmanagedType.I1)] bool hasRtPriority,
            byte rtPriority,
            [MarshalAs(UnmanagedType.I1)] bool hasRtAffinity,
            UIntPtr rtAffinity,
            [MarshalAs(UnmanagedType.I1)] bool validateState);

        [DllImport(Lib)]
        internal static extern void autd3_client_config_free(IntPtr config);

        [DllImport(Lib)]
        internal static extern IntPtr autd3_datagram_builder_new(IntPtr geometry);

        [DllImport(Lib)]
        internal static extern void autd3_datagram_builder_push(IntPtr builder, IntPtr op);

        [DllImport(Lib)]
        internal static extern void autd3_datagram_builder_push_each(IntPtr builder, IntPtr[] ops, UIntPtr numDevices);

        [DllImport(Lib)]
        internal static extern void autd3_op_free(IntPtr op);

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
        internal static extern void autd3_client_send(IntPtr client, IntPtr datagrams, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_response_token_await(IntPtr token, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_response_token_free(IntPtr token);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_response_check(byte[] data, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

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
        internal static extern void autd3_client_read_telemetry(IntPtr client, byte counter, CompletionCallback cb, IntPtr userData);

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
        internal static extern IntPtr autd3_client_checker(IntPtr client);

        [DllImport(Lib)]
        internal static extern void autd3_checker_check(IntPtr checker, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib)]
        internal static extern void autd3_checker_free(IntPtr checker);

        [DllImport(Lib)]
        internal static extern ulong autd3_link_status_recoveries(IntPtr status);

        [DllImport(Lib)]
        internal static extern UIntPtr autd3_link_status_num_devices(IntPtr status);

        [DllImport(Lib)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_link_status_device_state(IntPtr status, UIntPtr index, out byte outKind, out byte outBits);

        [DllImport(Lib)]
        internal static extern void autd3_link_status_free(IntPtr status);
    }
}
