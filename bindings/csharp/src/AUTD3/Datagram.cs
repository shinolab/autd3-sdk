using System;
using System.Threading;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    public enum RtSchedulePolicy : byte
    {
        Normal = 0,
        Fifo = 1,
        RoundRobin = 2,
    }

    public readonly struct ClientConfig
    {
        public bool LowLatency { get; }
        public uint TimeoutCycles { get; }
        public uint MaxInflight { get; }
        public uint MaxResyncRounds { get; }
        public uint ResetResendCycles { get; }
        public byte? RtPriority { get; }
        public bool DisableRtPriority { get; }
        public RtSchedulePolicy RtPolicy { get; }
        public ulong? RtAffinity { get; }
        public bool ValidateState { get; }
        public bool RequireSupportedFirmware { get; }

        public ClientConfig() : this(lowLatency: false)
        {
        }

        public ClientConfig(
            bool lowLatency = false,
            uint timeoutCycles = 10,
            uint maxInflight = 127,
            uint maxResyncRounds = 8,
            uint resetResendCycles = 2,
            byte? rtPriority = null,
            bool disableRtPriority = false,
            RtSchedulePolicy rtPolicy = RtSchedulePolicy.Fifo,
            ulong? rtAffinity = null,
            bool validateState = true,
            bool requireSupportedFirmware = false)
        {
            if (rtPriority.HasValue && disableRtPriority)
            {
                throw new ArgumentException("rtPriority and disableRtPriority are mutually exclusive");
            }
            LowLatency = lowLatency;
            TimeoutCycles = timeoutCycles;
            MaxInflight = maxInflight;
            MaxResyncRounds = maxResyncRounds;
            ResetResendCycles = resetResendCycles;
            RtPriority = rtPriority;
            DisableRtPriority = disableRtPriority;
            RtPolicy = rtPolicy;
            RtAffinity = rtAffinity;
            ValidateState = validateState;
            RequireSupportedFirmware = requireSupportedFirmware;
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
                NativeConfig.Apply("timeoutCycles", NativeClient.autd3_client_config_set_timeout_cycles(handle, TimeoutCycles));
                NativeConfig.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
                NativeConfig.Apply("maxResyncRounds", NativeClient.autd3_client_config_set_max_resync_rounds(handle, MaxResyncRounds));
                NativeConfig.Apply("resetResendCycles", NativeClient.autd3_client_config_set_reset_resend_cycles(handle, ResetResendCycles));
                NativeConfig.Apply("rtPriority", NativeClient.autd3_client_config_set_rt_priority(handle, NativeConfig.RtPriorityMode(RtPriority, DisableRtPriority), RtPriority ?? 0));
                NativeConfig.Apply("rtPolicy", NativeClient.autd3_client_config_set_rt_policy(handle, (byte)RtPolicy));
                NativeConfig.Apply("rtAffinity", NativeClient.autd3_client_config_set_rt_affinity(handle, RtAffinity.HasValue, (UIntPtr)(RtAffinity ?? 0)));
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
        internal const byte RtPriorityModeDefault = 0;
        internal const byte RtPriorityModeDisabled = 1;
        internal const byte RtPriorityModeExplicit = 2;

        internal static byte RtPriorityMode(byte? rtPriority, bool disabled) =>
            rtPriority.HasValue
                ? RtPriorityModeExplicit
                : disabled
                    ? RtPriorityModeDisabled
                    : RtPriorityModeDefault;

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
        private readonly IntPtr _client;

        private IntPtr _handle;

        internal IntPtr Handle => _handle;

        public DatagramBuilder(Geometry geometry) : this(geometry, IntPtr.Zero)
        {
        }

        internal DatagramBuilder(Geometry geometry, IntPtr client)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            _client = client;
            _handle = NativeClient.autd3_datagram_builder_new(geometry.Handle);
            if (Handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create datagram builder");
            }
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
            var handle = NativeClient.autd3_datagram_builder_build(Handle, _client, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new Frames(handle);
        }

        public void Dispose()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagram_builder_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~DatagramBuilder()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagram_builder_free(handle);
            }
        }
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
        private IntPtr _handle;

        internal IntPtr Handle => _handle;

        internal Frames(IntPtr handle)
        {
            _handle = handle;
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

        public void Dispose()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagrams_free(handle);
            }
            GC.SuppressFinalize(this);
        }

        ~Frames()
        {
            var handle = Interlocked.Exchange(ref _handle, IntPtr.Zero);
            if (handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagrams_free(handle);
            }
        }
    }
}
