using System;
using System.Runtime.InteropServices;

namespace AUTD3
{


    [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
    internal delegate void CompletionCallback(int code, IntPtr value, IntPtr msg, IntPtr userData);

    internal static class NativeClient
    {
        private const string Lib = "autd3capi";

        static NativeClient() => NativeAbi.Verify(Lib, autd3_abi_version());

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        private static extern uint autd3_abi_version();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_client_config_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_low_latency(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_validate_state(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_require_supported_firmware(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_timeout_cycles(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_max_inflight(IntPtr config, UIntPtr value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_max_resync_rounds(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_reset_resend_cycles(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_rt_priority(IntPtr config, byte mode, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_rt_policy(IntPtr config, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_client_config_set_rt_affinity(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool hasAffinity, UIntPtr coreId);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_config_free(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_opener_free(IntPtr opener);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_datagram_builder_new(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_datagram_builder_push(DatagramBuilderHandle builder, IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_datagram_builder_push_each(DatagramBuilderHandle builder, IntPtr[] ops, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_op_free(IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_datagram_builder_free(IntPtr builder);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_datagram_builder_build(DatagramBuilderHandle builder, IntPtr client, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_datagrams_num_frames(FramesHandle datagrams);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_datagrams_free(IntPtr datagrams);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_open(GeometryHandle geometry, IntPtr link, IntPtr config, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_client_num_devices(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send_checked(ClientHandle client, FramesHandle datagrams, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_send(ClientHandle client, FramesHandle datagrams, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_await(IntPtr token, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_response_token_free(IntPtr token);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_response_check(byte[] data, UIntPtr len, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_stop(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_close(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_free(IntPtr client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_firmware_version(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_fpga_state(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_error_detail(ClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_client_read_telemetry(ClientHandle client, byte counter, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_byte_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_byte_array_data(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_byte_array_free(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_string_array_len(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_string_array_get(IntPtr array, UIntPtr index);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_string_array_free(IntPtr array);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_client_checker(ClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_checker_check(CheckerHandle checker, byte[] err, UIntPtr errLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_checker_free(IntPtr checker);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern ulong autd3_link_status_recoveries(IntPtr status);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_link_status_num_devices(IntPtr status);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        [return: MarshalAs(UnmanagedType.I1)]
        internal static extern bool autd3_link_status_device_state(IntPtr status, UIntPtr index, out byte outKind, out byte outBits);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_link_status_free(IntPtr status);
    }

    internal static class NativeLegacyClient
    {
        private const string Lib = "autd3capi";

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_legacy_client_config_new();

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_client_config_set_timeout_cycles(IntPtr config, uint value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_client_config_set_rt_priority(IntPtr config, byte mode, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_client_config_set_rt_policy(IntPtr config, byte value);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_client_config_set_rt_affinity(IntPtr config, [MarshalAs(UnmanagedType.I1)] bool hasAffinity, UIntPtr coreId);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_config_free(IntPtr config);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_opener_free(IntPtr opener);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_legacy_datagram_builder_new(GeometryHandle geometry);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_datagram_builder_push(LegacyDatagramBuilderHandle builder, IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_legacy_op_change_segment(byte kind, byte bank, byte transitionMode, ulong transitionValue);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_op_free(IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_datagram_builder_push_legacy(LegacyDatagramBuilderHandle builder, IntPtr op);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern int autd3_legacy_datagram_builder_push_each(LegacyDatagramBuilderHandle builder, IntPtr[] ops, UIntPtr numDevices);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_datagram_builder_free(IntPtr builder);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_legacy_datagram_builder_build(LegacyDatagramBuilderHandle builder, IntPtr client, byte[] outErr, UIntPtr outErrLen);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_legacy_frames_num_frames(LegacyFramesHandle frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_frames_free(IntPtr frames);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_open(GeometryHandle geometry, IntPtr link, IntPtr config, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern UIntPtr autd3_legacy_client_num_devices(LegacyClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_send(LegacyClientHandle client, LegacyFramesHandle frames, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_send_checked(LegacyClientHandle client, LegacyFramesHandle frames, long frame, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_read_firmware_version(LegacyClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_read_fpga_state(LegacyClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern IntPtr autd3_legacy_client_checker(LegacyClientHandle client);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_stop(LegacyClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_close(LegacyClientHandle client, CompletionCallback cb, IntPtr userData);

        [DllImport(Lib, CallingConvention = CallingConvention.Cdecl)]
        internal static extern void autd3_legacy_client_free(IntPtr client);
    }
}
