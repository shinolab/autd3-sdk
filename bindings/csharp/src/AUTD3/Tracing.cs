using System;
using System.Text;

namespace AUTD3
{
    public enum LogWriter : byte
    {
        Stdout = 0,
        Stderr = 1,
    }

    internal static class TracingDefaults
    {
        internal static readonly string DefaultFilter;
        internal static readonly LogWriter Writer;

        static TracingDefaults()
        {
            var filter = new byte[NativeAbi.ErrorBufferLength];
            OptionNative.Apply("tracing", NativeClient.autd3_tracing_option_default(filter, (UIntPtr)filter.Length, out var writer));
            DefaultFilter = NativeUtil.Utf8(filter);
            Writer = (LogWriter)writer;
        }
    }

    public readonly struct TracingOption
    {
        private readonly string? _defaultFilter;
        public string DefaultFilter { get => _defaultFilter ?? TracingDefaults.DefaultFilter; init => _defaultFilter = value; }
        private readonly LogWriter? _writer;
        public LogWriter Writer { get => _writer ?? TracingDefaults.Writer; init => _writer = value; }
    }

    public sealed class TracingGuard : IDisposable
    {
        private readonly TracingGuardHandle _handle;

        internal TracingGuard(IntPtr handle)
        {
            _handle = new TracingGuardHandle(handle);
        }

        public void Dispose() => _handle.Dispose();
    }

    public static class Tracing
    {
        private static readonly object Lock = new object();
        private static TracingGuard? _current;
        private static bool _exitHooked;

        public static TracingGuard Init(TracingOption option)
        {
            var filter = option.DefaultFilter;
            if (filter == null)
            {
                throw new ArgumentNullException(nameof(option));
            }
            if (filter.IndexOf('\0') >= 0)
            {
                throw new Autd3Exception("the default filter must not contain a NUL character", Autd3ErrorCode.InvalidArgument);
            }
            var utf8 = new byte[Encoding.UTF8.GetByteCount(filter) + 1];
            Encoding.UTF8.GetBytes(filter, 0, filter.Length, utf8, 0);
            var err = new byte[NativeAbi.ErrorBufferLength];
            var guard = NativeClient.autd3_init_tracing(utf8, (byte)option.Writer, err, (UIntPtr)err.Length);
            if (guard == IntPtr.Zero)
            {
                throw new Autd3Exception(NativeUtil.Utf8(err));
            }
            var current = new TracingGuard(guard);
            lock (Lock)
            {
                _current = current;
                if (!_exitHooked)
                {
                    AppDomain.CurrentDomain.ProcessExit += (_, _) => _current?.Dispose();
                    _exitHooked = true;
                }
            }
            return current;
        }
    }
}
