using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading.Tasks;

namespace AUTD3
{
    public readonly struct FpgaState
    {
        public byte Raw { get; }

        public FpgaState(byte raw)
        {
            Raw = raw;
        }

        public bool IsThermalAsserted => (Raw & (1 << 0)) != 0;
        public bool ReadsEnabled => (Raw & (1 << 7)) != 0;
    }

    public sealed class LinkStatus
    {
        public IReadOnlyList<string> DeviceStates { get; }
        public bool AllOp { get; }
        public bool AnyLost { get; }
        public ulong Recoveries { get; }

        internal LinkStatus(IReadOnlyList<string> deviceStates, bool allOp, bool anyLost, ulong recoveries)
        {
            DeviceStates = deviceStates;
            AllOp = allOp;
            AnyLost = anyLost;
            Recoveries = recoveries;
        }

        public override bool Equals(object? obj)
        {
            if (obj is not LinkStatus other)
            {
                return false;
            }
            if (AllOp != other.AllOp || AnyLost != other.AnyLost || Recoveries != other.Recoveries)
            {
                return false;
            }
            if (DeviceStates.Count != other.DeviceStates.Count)
            {
                return false;
            }
            for (var i = 0; i < DeviceStates.Count; i++)
            {
                if (!string.Equals(DeviceStates[i], other.DeviceStates[i], StringComparison.Ordinal))
                {
                    return false;
                }
            }
            return true;
        }

        public override int GetHashCode()
        {
            var hash = new HashCode();
            hash.Add(AllOp);
            hash.Add(AnyLost);
            hash.Add(Recoveries);
            foreach (var state in DeviceStates)
            {
                hash.Add(state, StringComparer.Ordinal);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(LinkStatus? left, LinkStatus? right) =>
            left is null ? right is null : left.Equals(right);

        public static bool operator !=(LinkStatus? left, LinkStatus? right) => !(left == right);
    }

    public sealed class ResponseToken : IDisposable
    {
        private IntPtr _handle;

        internal ResponseToken(IntPtr handle)
        {
            _handle = handle;
        }

        public Task AwaitAsync()
        {
            var handle = _handle;
            _handle = IntPtr.Zero;
            if (handle == IntPtr.Zero)
            {
                return Task.CompletedTask;
            }
            GC.SuppressFinalize(this);
            return AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_response_token_await(handle, cb, ud));
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeClient.autd3_response_token_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~ResponseToken()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeClient.autd3_response_token_free(_handle);
            }
        }
    }

    public sealed class Client : IDisposable
    {
        public const int MaxInflight = 127;

        internal IntPtr Handle { get; private set; }

        private Client(IntPtr handle)
        {
            Handle = handle;
        }

        public static async Task<Client> OpenAsync(Geometry geometry, ILink link, ClientConfig config)
        {
            var opener = link.TakeOpener();


            var configHandle = config.CreateHandle();
            Task<IntPtr> task;
            try
            {
                task = AsyncOps.InvokeAsync((cb, ud) =>
                    NativeClient.autd3_client_open(geometry.Handle, opener, configHandle, cb, ud));
            }
            finally
            {
                NativeClient.autd3_client_config_free(configHandle);
            }
            var value = await task.ConfigureAwait(false);
            return new Client(value);
        }

        public int NumDevices => (int)NativeClient.autd3_client_num_devices(Handle);


        public DatagramBuilder DatagramBuilder() => new DatagramBuilder(NumDevices);



        public Task SendCheckedAsync(Frame frame) =>
            AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_checked(Handle, frame.Datagrams.Handle, frame.Index, cb, ud));

        public async Task<ResponseToken> SendAsync(Frame frame)
        {
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send(Handle, frame.Datagrams.Handle, frame.Index, cb, ud)).ConfigureAwait(false);
            return new ResponseToken(token);
        }

        public async Task<IReadOnlyList<string>> ReadFirmwareVersionAsync()
        {
            var array = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_read_firmware_version(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_string_array_len(array);
                var versions = new List<string>(count);
                for (var i = 0; i < count; i++)
                {
                    versions.Add(NativeUtil.PtrToString(NativeClient.autd3_string_array_get(array, (UIntPtr)i)));
                }
                return versions;
            }
            finally
            {
                NativeClient.autd3_string_array_free(array);
            }
        }

        public async Task<IReadOnlyList<FpgaState>> ReadFpgaStateAsync()
        {
            var bytes = await ReadByteArrayAsync((cb, ud) =>
                NativeClient.autd3_client_read_fpga_state(Handle, cb, ud)).ConfigureAwait(false);
            var states = new FpgaState[bytes.Length];
            for (var i = 0; i < bytes.Length; i++)
            {
                states[i] = new FpgaState(bytes[i]);
            }
            return states;
        }

        public Task<byte[]> ReadErrorDetailAsync() =>
            ReadByteArrayAsync((cb, ud) => NativeClient.autd3_client_read_error_detail(Handle, cb, ud));

        private static async Task<byte[]> ReadByteArrayAsync(Action<CompletionCallback, IntPtr> invoke)
        {
            var array = await AsyncOps.InvokeAsync(invoke).ConfigureAwait(false);
            try
            {
                var len = (int)NativeClient.autd3_byte_array_len(array);
                var bytes = new byte[len];
                if (len > 0)
                {
                    Marshal.Copy(NativeClient.autd3_byte_array_data(array), bytes, 0, len);
                }
                return bytes;
            }
            finally
            {
                NativeClient.autd3_byte_array_free(array);
            }
        }

        public async Task<LinkStatus> CheckStatusAsync()
        {
            var status = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_check_status(Handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_link_status_num_devices(status);
                var states = new List<string>(count);
                for (var i = 0; i < count; i++)
                {
                    states.Add(NativeUtil.PtrToString(NativeClient.autd3_link_status_device_state(status, (UIntPtr)i)));
                }
                return new LinkStatus(
                    states,
                    NativeClient.autd3_link_status_all_op(status),
                    NativeClient.autd3_link_status_any_lost(status),
                    NativeClient.autd3_link_status_recoveries(status));
            }
            finally
            {
                NativeClient.autd3_link_status_free(status);
            }
        }

        public Task StopAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_stop(Handle, cb, ud));

        public Task CloseAsync() =>
            AsyncOps.InvokeAsync((cb, ud) => NativeClient.autd3_client_close(Handle, cb, ud));

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_client_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~Client()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_client_free(Handle);
            }
        }
    }
}
