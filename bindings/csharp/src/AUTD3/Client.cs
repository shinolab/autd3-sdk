using System;
using System.Collections.Generic;
using System.Runtime.CompilerServices;
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
        public bool IsPatternStopped => (Raw & (1 << 4)) != 0;
        public bool IsModStopped => (Raw & (1 << 5)) != 0;
        public bool IsTransitionPending => (Raw & (1 << 6)) != 0;
        public bool ReadsEnabled => (Raw & (1 << 7)) != 0;
    }

    public sealed class LinkStatus
    {
        public IReadOnlyList<DeviceState> Devices { get; }
        public ulong Recoveries { get; }

        internal LinkStatus(IReadOnlyList<DeviceState> devices, ulong recoveries)
        {
            Devices = devices;
            Recoveries = recoveries;
        }

        public bool AllOp
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state != DeviceState.Op)
                    {
                        return false;
                    }
                }
                return true;
            }
        }

        public bool AnyLost
        {
            get
            {
                foreach (var state in Devices)
                {
                    if (state == DeviceState.Lost)
                    {
                        return true;
                    }
                }
                return false;
            }
        }

        public override bool Equals(object? obj)
        {
            if (obj is not LinkStatus other)
            {
                return false;
            }
            if (Recoveries != other.Recoveries || Devices.Count != other.Devices.Count)
            {
                return false;
            }
            for (var i = 0; i < Devices.Count; i++)
            {
                if (Devices[i] != other.Devices[i])
                {
                    return false;
                }
            }
            return true;
        }

        public override int GetHashCode()
        {
            var hash = new HashCode();
            hash.Add(Recoveries);
            foreach (var state in Devices)
            {
                hash.Add(state);
            }
            return hash.ToHashCode();
        }

        public static bool operator ==(LinkStatus? left, LinkStatus? right) =>
            left is null ? right is null : left.Equals(right);

        public static bool operator !=(LinkStatus? left, LinkStatus? right) => !(left == right);
    }

    public sealed class Checker : IDisposable
    {
        private IntPtr _handle;

        internal Checker(IntPtr handle)
        {
            _handle = handle;
        }

        public async Task<LinkStatus> CheckAsync()
        {
            var handle = _handle;
            var status = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_checker_check(handle, cb, ud)).ConfigureAwait(false);
            try
            {
                var count = (int)NativeClient.autd3_link_status_num_devices(status);
                var devices = new List<DeviceState>(count);
                for (var i = 0; i < count; i++)
                {
                    if (!NativeClient.autd3_link_status_device_state(status, (UIntPtr)i, out var kind, out var bits))
                    {
                        throw new Autd3Exception("failed to read device state");
                    }
                    devices.Add(DeviceState.FromNative(kind, bits));
                }
                return new LinkStatus(devices, NativeClient.autd3_link_status_recoveries(status));
            }
            finally
            {
                NativeClient.autd3_link_status_free(status);
            }
        }

        public void Dispose()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeClient.autd3_checker_free(_handle);
                _handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~Checker()
        {
            if (_handle != IntPtr.Zero)
            {
                NativeClient.autd3_checker_free(_handle);
            }
        }
    }

    public sealed class Response
    {
        private readonly byte[] _data;

        internal Response(byte[] data)
        {
            _data = data;
        }

        public IReadOnlyList<byte> Data => _data;

        public void Check()
        {
            var err = new byte[256];
            if (!NativeClient.autd3_response_check(_data, (UIntPtr)_data.Length, err, (UIntPtr)err.Length))
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
        }
    }

    public sealed class ResponseToken : IDisposable
    {
        private IntPtr _handle;

        internal ResponseToken(IntPtr handle)
        {
            _handle = handle;
        }

        public async Task<Response> AwaitAsync()
        {
            var handle = _handle;
            _handle = IntPtr.Zero;
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("ResponseToken has already been awaited");
            }
            GC.SuppressFinalize(this);
            var data = await Client.ReadByteArrayAsync((cb, ud) =>
                NativeClient.autd3_response_token_await(handle, cb, ud)).ConfigureAwait(false);
            return new Response(data);
        }

        public TaskAwaiter<Response> GetAwaiter() => AwaitAsync().GetAwaiter();

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

        private readonly Geometry _geometry;

        private Client(IntPtr handle, Geometry geometry)
        {
            Handle = handle;
            _geometry = geometry;
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
            return new Client(value, geometry);
        }

        public static async Task<(Client Client, Checker Checker)> OpenWithCheckerAsync(Geometry geometry, ILink link, ClientConfig config)
        {
            var client = await OpenAsync(geometry, link, config).ConfigureAwait(false);
            var checker = NativeClient.autd3_client_checker(client.Handle);
            if (checker == IntPtr.Zero)
            {
                client.Dispose();
                throw new Autd3Exception("failed to create checker");
            }
            return (client, new Checker(checker));
        }

        public int NumDevices => (int)NativeClient.autd3_client_num_devices(Handle);


        public DatagramBuilder DatagramBuilder() => new DatagramBuilder(_geometry, Handle);



        public Task SendCheckedAsync(Frame frame) =>
            AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send_checked(Handle, frame.Frames.Handle, frame.Index, cb, ud));

        public async Task<ResponseToken> SendAsync(Frame frame)
        {
            var token = await AsyncOps.InvokeAsync((cb, ud) =>
                NativeClient.autd3_client_send(Handle, frame.Frames.Handle, frame.Index, cb, ud)).ConfigureAwait(false);
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

        public Task<byte[]> ReadTelemetryAsync(Telemetry counter) =>
            ReadByteArrayAsync((cb, ud) => NativeClient.autd3_client_read_telemetry(Handle, (byte)counter, cb, ud));

        internal static async Task<byte[]> ReadByteArrayAsync(Action<CompletionCallback, IntPtr> invoke)
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
