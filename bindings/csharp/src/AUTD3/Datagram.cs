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
        public uint SendIntervalCycles { get; }
        public uint MaxResyncRounds { get; }
        public uint ResetResendCycles { get; }
        public byte? RtPriority { get; }
        public ulong? RtAffinity { get; }
        public bool ValidateState { get; }

        public ClientConfig(
            bool lowLatency = false,
            uint timeoutCycles = 10,
            uint maxInflight = 127,
            uint sendIntervalCycles = 1,
            uint maxResyncRounds = 8,
            uint resetResendCycles = 2,
            byte? rtPriority = null,
            ulong? rtAffinity = null,
            bool validateState = true)
        {
            LowLatency = lowLatency;
            TimeoutCycles = timeoutCycles;
            MaxInflight = maxInflight;
            SendIntervalCycles = sendIntervalCycles;
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
                SendIntervalCycles,
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
        private readonly int _numDevices;

        internal IntPtr Handle { get; private set; }

        internal DatagramBuilder(int numDevices)
        {
            _numDevices = numDevices;
            Handle = NativeClient.autd3_datagram_builder_new((UIntPtr)numDevices);
        }


        public DatagramBuilder(Geometry geometry) : this(geometry.NumDevices)
        {
        }

        public DatagramBuilder Push(ICommand command)
        {
            var op = command.CreateOp();
            NativeClient.autd3_datagram_builder_push(Handle, op);
            return this;
        }

        public DatagramBuilder PushEach(Func<int, ICommand?> factory)
        {
            var ops = new IntPtr[_numDevices];
            try
            {
                for (var i = 0; i < _numDevices; i++)
                {
                    var command = factory(i);
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

        public Datagrams Build()
        {
            var err = new byte[256];
            var handle = NativeClient.autd3_datagram_builder_build(Handle, err, (UIntPtr)err.Length);
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            return new Datagrams(handle);
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
        internal Datagrams Datagrams { get; }
        internal long Index { get; }

        internal Frame(Datagrams datagrams, long index)
        {
            Datagrams = datagrams;
            Index = index;
        }
    }

    public sealed class Datagrams : IDisposable, IEnumerable<Frame>
    {
        internal IntPtr Handle { get; private set; }

        internal Datagrams(IntPtr handle)
        {
            Handle = handle;
        }

        public int NumFrames => (int)NativeClient.autd3_datagrams_num_frames(Handle);

        public Frame this[int index]
        {
            get
            {
                if (index < 0 || index >= NumFrames)
                {
                    throw new ArgumentOutOfRangeException(nameof(index));
                }
                return new Frame(this, index);
            }
        }

        public IEnumerator<Frame> GetEnumerator()
        {
            var count = NumFrames;
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

        ~Datagrams()
        {
            if (Handle != IntPtr.Zero)
            {
                NativeClient.autd3_datagrams_free(Handle);
            }
        }
    }
}
