using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    public readonly struct ClientConfig
    {
        public bool LowLatency { get; }
        public uint TimeoutCycles { get; }
        public uint MaxInflight { get; }
        public uint MaxResyncRounds { get; }
        public uint ResetResendCycles { get; }
        public byte? RtPriority { get; }
        public ulong? RtAffinity { get; }
        public bool ValidateState { get; }

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
            ulong? rtAffinity = null,
            bool validateState = true)
        {
            LowLatency = lowLatency;
            TimeoutCycles = timeoutCycles;
            MaxInflight = maxInflight;
            MaxResyncRounds = maxResyncRounds;
            ResetResendCycles = resetResendCycles;
            RtPriority = rtPriority;
            RtAffinity = rtAffinity;
            ValidateState = validateState;
        }

        internal IntPtr CreateHandle()
        {
            var handle = NativeClient.autd3_client_config_new(
                LowLatency,
                TimeoutCycles,
                (UIntPtr)MaxInflight,
                MaxResyncRounds,
                ResetResendCycles,
                RtPriority.HasValue,
                RtPriority ?? 0,
                RtAffinity.HasValue,
                (UIntPtr)(RtAffinity ?? 0),
                ValidateState);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            return handle;
        }
    }

    public sealed class DatagramBuilder : IDisposable
    {
        private readonly Geometry _geometry;
        private readonly int _numDevices;
        private readonly IntPtr _client;

        internal IntPtr Handle { get; private set; }

        public DatagramBuilder(Geometry geometry) : this(geometry, IntPtr.Zero)
        {
        }

        internal DatagramBuilder(Geometry geometry, IntPtr client)
        {
            _geometry = geometry;
            _numDevices = geometry.NumDevices;
            _client = client;
            Handle = NativeClient.autd3_datagram_builder_new(geometry.Handle);
            if (Handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create datagram builder");
            }
        }

        public DatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            NativeClient.autd3_datagram_builder_push(Handle, op);
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
            NativeClient.autd3_datagram_builder_push_each(Handle, ops, (UIntPtr)_numDevices);
            return this;
        }

        public Frames Build()
        {
            var err = new byte[256];
            var handle = NativeClient.autd3_datagram_builder_build(Handle, _client, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new Frames(handle);
        }

        public void Dispose()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagram_builder_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~DatagramBuilder()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagram_builder_free(Handle);
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
        internal IntPtr Handle { get; private set; }

        internal Frames(IntPtr handle)
        {
            Handle = handle;
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
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagrams_free(Handle);
                Handle = IntPtr.Zero;
            }
            GC.SuppressFinalize(this);
        }

        ~Frames()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagrams_free(Handle);
            }
        }
    }
}
