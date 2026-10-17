using System;
using System.Collections;
using System.Collections.Generic;

namespace AUTD3
{


    internal static class ClientConfigDefaults
    {
        internal static readonly ClientConfig Preset;

        static ClientConfigDefaults()
        {
            var handle = NativeClient.autd3_client_config_new();
            if (handle == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create client config");
            }
            try
            {
                Preset = ClientConfig.FromHandle(handle);
            }
            finally
            {
                NativeClient.autd3_client_config_free(handle);
            }
        }
    }

    public readonly struct ClientConfig
    {
        private readonly TimeSpan? _ackTimeout;
        public TimeSpan AckTimeout { get => _ackTimeout ?? ClientConfigDefaults.Preset.AckTimeout; init => _ackTimeout = value; }
        private readonly uint? _maxInflight;
        public uint MaxInflight { get => _maxInflight ?? ClientConfigDefaults.Preset.MaxInflight; init => _maxInflight = value; }
        private readonly uint? _maxResyncRounds;
        public uint MaxResyncRounds { get => _maxResyncRounds ?? ClientConfigDefaults.Preset.MaxResyncRounds; init => _maxResyncRounds = value; }
        private readonly bool? _requireSupportedFirmware;
        public bool RequireSupportedFirmware { get => _requireSupportedFirmware ?? ClientConfigDefaults.Preset.RequireSupportedFirmware; init => _requireSupportedFirmware = value; }
        private readonly bool _resynchronizeOnRecoverySet;
        private readonly uint? _resynchronizeOnRecovery;
        public uint? ResynchronizeOnRecovery
        {
            get => _resynchronizeOnRecoverySet ? _resynchronizeOnRecovery : ClientConfigDefaults.Preset.ResynchronizeOnRecovery;
            init
            {
                _resynchronizeOnRecovery = value;
                _resynchronizeOnRecoverySet = true;
            }
        }
        private readonly bool _releaseFailsafeOnRecoverySet;
        private readonly uint? _releaseFailsafeOnRecovery;
        public uint? ReleaseFailsafeOnRecovery
        {
            get => _releaseFailsafeOnRecoverySet ? _releaseFailsafeOnRecovery : ClientConfigDefaults.Preset.ReleaseFailsafeOnRecovery;
            init
            {
                _releaseFailsafeOnRecovery = value;
                _releaseFailsafeOnRecoverySet = true;
            }
        }
        private readonly bool _execLeadSet;
        private readonly TimeSpan? _execLead;
        public TimeSpan? ExecLead
        {
            get => _execLeadSet ? _execLead : ClientConfigDefaults.Preset.ExecLead;
            init
            {
                _execLead = value;
                _execLeadSet = true;
            }
        }

        internal static ClientConfig FromHandle(IntPtr handle)
        {
            var ackTimeout = OptionNative.GetDuration(handle, NativeClient.autd3_client_config_get_ack_timeout_ns);
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_max_inflight(handle, out var maxInflight));
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_max_resync_rounds(handle, out var maxResyncRounds));
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_require_supported_firmware(handle, out var requireSupportedFirmware));
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_resynchronize_on_recovery(handle, out var resynchronizeOnRecovery));
            OptionNative.Apply("preset", NativeClient.autd3_client_config_get_release_failsafe_on_recovery(handle, out var releaseFailsafeOnRecovery));
            return new ClientConfig
            {
                AckTimeout = ackTimeout,
                MaxInflight = (uint)maxInflight,
                MaxResyncRounds = maxResyncRounds,
                RequireSupportedFirmware = requireSupportedFirmware,
                ResynchronizeOnRecovery = resynchronizeOnRecovery == 0 ? null : resynchronizeOnRecovery,
                ReleaseFailsafeOnRecovery = releaseFailsafeOnRecovery == 0 ? null : releaseFailsafeOnRecovery,
                ExecLead = OptionNative.GetOptionalDuration(handle, NativeClient.autd3_client_config_get_exec_lead),
            };
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
                OptionNative.SetRequiredDuration(handle, "ackTimeout", AckTimeout, NativeClient.autd3_client_config_set_ack_timeout_ns);
                OptionNative.Apply("maxInflight", NativeClient.autd3_client_config_set_max_inflight(handle, (UIntPtr)MaxInflight));
                OptionNative.Apply("maxResyncRounds", NativeClient.autd3_client_config_set_max_resync_rounds(handle, MaxResyncRounds));
                OptionNative.Apply("requireSupportedFirmware", NativeClient.autd3_client_config_set_require_supported_firmware(handle, RequireSupportedFirmware));
                OptionNative.Apply("resynchronizeOnRecovery", ResynchronizeOnRecovery switch
                {
                    null => NativeClient.autd3_client_config_set_resynchronize_on_recovery(handle, 0),
                    0 => -1,
                    { } attempts => NativeClient.autd3_client_config_set_resynchronize_on_recovery(handle, attempts),
                });
                OptionNative.Apply("releaseFailsafeOnRecovery", ReleaseFailsafeOnRecovery switch
                {
                    null => NativeClient.autd3_client_config_set_release_failsafe_on_recovery(handle, 0),
                    0 => -1,
                    { } attempts => NativeClient.autd3_client_config_set_release_failsafe_on_recovery(handle, attempts),
                });
                OptionNative.SetOptionalDuration(handle, "execLead", ExecLead, NativeClient.autd3_client_config_set_exec_lead);
            }
            catch
            {
                NativeClient.autd3_client_config_free(handle);
                throw;
            }
            return handle;
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
        private readonly FramesHandle _handle;

        internal FramesHandle Handle => _handle;

        internal Frames(IntPtr handle)
        {
            _handle = new FramesHandle(handle);
        }

        public Frames() : this(NativeClient.autd3_frames_new())
        {
        }

        public static Frames Encode(Geometry geometry, ICommand command)
        {
            if (geometry == null)
            {
                throw new ArgumentNullException(nameof(geometry));
            }
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            var op = CreateOp(geometry, command);
            var err = new byte[NativeAbi.ErrorBufferLength];
            IntPtr handle;
            int code;
            try
            {
                handle = NativeClient.autd3_frames_encode(geometry.Handle, op, out code, err, (UIntPtr)err.Length);
            }
            catch
            {
                NativeClient.autd3_op_free(op);
                throw;
            }
            if (handle == IntPtr.Zero)
            {
                NativeClient.autd3_op_free(op);
                throw Autd3Exception.FromNative(code, err);
            }
            return new Frames(handle);
        }

        public void EncodeInto(Geometry geometry, ICommand command)
        {
            if (geometry == null)
            {
                throw new ArgumentNullException(nameof(geometry));
            }
            if (command == null)
            {
                throw new ArgumentNullException(nameof(command));
            }
            var op = CreateOp(geometry, command);
            var err = new byte[NativeAbi.ErrorBufferLength];
            int code;
            try
            {
                code = NativeClient.autd3_frames_encode_into(Handle, geometry.Handle, op, err, (UIntPtr)err.Length);
            }
            catch
            {
                NativeClient.autd3_op_free(op);
                throw;
            }
            if (code != 0)
            {
                NativeClient.autd3_op_free(op);
                throw Autd3Exception.FromNative(code, err);
            }
        }

        private static IntPtr CreateOp(Geometry geometry, ICommand command)
        {
            var op = command.CreateOp(geometry);
            if (op == IntPtr.Zero)
            {
                throw new Autd3Exception("failed to create the command", Autd3ErrorCode.InvalidArgument);
            }
            return op;
        }

        public int Length => (int)NativeClient.autd3_frames_num_frames(Handle);

        public bool IsEmpty => Length == 0;

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
