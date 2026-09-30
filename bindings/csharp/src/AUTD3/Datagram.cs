using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    public readonly struct ClientConfig
    {
        public bool LowLatency { get; init; } = false;
        public TimeSpan AckTimeout { get; init; } = TimeSpan.FromMilliseconds(10);
        public uint MaxInflight { get; init; } = 7;
        public uint MaxResyncRounds { get; init; } = 8;
        public bool ValidateState { get; init; } = true;
        public bool RequireSupportedFirmware { get; init; } = false;

        public ClientConfig()
        {
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                NativeConfig.Apply("lowLatency", NativeClient.autd3_client_config_set_low_latency(handle, LowLatency));
                NativeConfig.Apply("ackTimeout", AckTimeout < TimeSpan.Zero ? -1 : NativeClient.autd3_client_config_set_ack_timeout_ns(handle, OptionNative.ToNanos(AckTimeout)));
                NativeConfig.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
                NativeConfig.Apply("maxResyncRounds", NativeClient.autd3_client_config_set_max_resync_rounds(handle, MaxResyncRounds));
                NativeConfig.Apply("validateState", NativeClient.autd3_client_config_set_validate_state(handle, ValidateState));
                NativeConfig.Apply("requireSupportedFirmware", NativeClient.autd3_client_config_set_require_supported_firmware(handle, RequireSupportedFirmware));
            }
            catch
            {
                NativeClient.autd3_client_config_free(handle);
                throw;
            }
            return handle;
        }
    }

    internal static class NativeConfig
    {
        internal static void Apply(string field, int code)
        {
            if (code != 0)
            {
                throw new Autd3Exception($"`{field}` is out of the range the native library accepts");
            }
        }
    }

    public sealed class DatagramBuilder : IDisposable
    {
        private readonly Geometry _geometry;
        private readonly int _numDevices;
        private readonly Client? _client;

        private readonly DatagramBuilderHandle _handle;

        internal DatagramBuilderHandle Handle => _handle;

        public DatagramBuilder(Geometry geometry) : this(geometry, null)
        {
        }

        internal DatagramBuilder(Geometry geometry, Client? client)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            _client = client;
            var handle = NativeClient.autd3_datagram_builder_new(geometry.Handle);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create datagram builder");
            }
            _handle = new DatagramBuilderHandle(handle);
        }

        public DatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            if (NativeClient.autd3_datagram_builder_push(Handle, op) != 0)
            {
                throw new Autd3Exception("failed to push the command onto the datagram builder");
            }
            return this;
        }

        public DatagramBuilder PushEach(Func<Device, ICommand?> factory)
        {
            var ops = new IntPtr[_numDevices];
            try
            {
                for (var i = 0; i < _numDevices; i++)
                {
                    var command = factory(_geometry[i]);
                    ops[i] = command == null ? IntPtr.Zero : command.CreateOp();
                }
            }
            catch
            {
                foreach (var op in ops)
                {
                    if (op != IntPtr.Zero)
                    {
                        NativeClient.autd3_op_free(op);
                    }
                }
                throw;
            }
            if (NativeClient.autd3_datagram_builder_push_each(Handle, ops, (UIntPtr)_numDevices) != 0)
            {
                throw new Autd3Exception("failed to push the per-device commands onto the datagram builder");
            }
            return this;
        }

        public Frames Build()
        {
            var err = new byte[NativeAbi.ErrorBufferLength];
            using var client = new HandleLease(_client?.Handle);
            var handle = NativeClient.autd3_datagram_builder_build(Handle, client.Pointer, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new Frames(handle);
        }

        public void Dispose() => _handle.Dispose();
    }


    public readonly struct Frame
    {
        internal Frames Frames { get; }
        internal long Index { get; }

        internal Frame(Frames frames, long index)
        {
            Frames = frames;
            Index = index;
        }
    }

    public sealed class Frames : IDisposable, IEnumerable<Frame>
    {
        private readonly FramesHandle _handle;

        internal FramesHandle Handle => _handle;

        internal Frames(IntPtr handle)
        {
            _handle = new FramesHandle(handle);
        }

        public int Length => (int)NativeClient.autd3_datagrams_num_frames(Handle);

        public Frame this[int index]
        {
            get
            {
                if (index < 0 || index >= Length)
                {
                    throw new ArgumentOutOfRangeException(nameof(index));
                }
                return new Frame(this, index);
            }
        }

        public IEnumerator<Frame> GetEnumerator()
        {
            var count = Length;
            for (long i = 0; i < count; i++)
            {
                yield return new Frame(this, i);
            }
        }

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public void Dispose() => _handle.Dispose();
    }
}
