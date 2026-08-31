using System;
using System.Runtime.InteropServices;

namespace AUTD3
{
    internal abstract class Autd3SafeHandle : SafeHandle
    {
        protected Autd3SafeHandle(IntPtr handle) : base(IntPtr.Zero, true)
        {
            SetHandle(handle);
        }

        public override bool IsInvalid => handle == IntPtr.Zero;
    }

    internal sealed class GeometryHandle : Autd3SafeHandle
    {
        internal GeometryHandle(IntPtr handle) : base(handle)
        {
        }

        protected override bool ReleaseHandle()
        {
            NativeCore.autd3_core_geometry_free(handle);
            return true;
        }
    }

    internal sealed class HandleArray : IDisposable
    {
        private readonly SafeHandle[] _handles;
        private readonly bool[] _added;

        internal readonly IntPtr[] Pointers;

        internal HandleArray(SafeHandle[] handles)
        {
            _handles = handles;
            _added = new bool[handles.Length];
            Pointers = new IntPtr[handles.Length];
            try
            {
                for (var i = 0; i < handles.Length; i++)
                {
                    handles[i].DangerousAddRef(ref _added[i]);
                    Pointers[i] = handles[i].DangerousGetHandle();
                }
            }
            catch
            {
                Dispose();
                throw;
            }
        }

        public void Dispose()
        {
            for (var i = 0; i < _handles.Length; i++)
            {
                if (_added[i])
                {
                    _added[i] = false;
                    _handles[i].DangerousRelease();
                }
            }
        }
    }

    internal struct HandleLease : IDisposable
    {
        private readonly SafeHandle? _handle;
        private bool _added;

        internal IntPtr Pointer { get; }

        internal HandleLease(SafeHandle? handle)
        {
            _handle = handle;
            _added = false;
            Pointer = IntPtr.Zero;
            if (handle == null)
            {
                return;
            }
            var added = false;
            handle.DangerousAddRef(ref added);
            _added = added;
            Pointer = handle.DangerousGetHandle();
        }

        public void Dispose()
        {
            if (_added)
            {
                _added = false;
                _handle!.DangerousRelease();
            }
        }
    }
}
