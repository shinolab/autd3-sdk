using System;
using System.Collections;
using System.Collections.Generic;
using System.Text;

namespace AUTD3
{
    public readonly struct DeviceMask : IEquatable<DeviceMask>, IEnumerable<int>
    {
        private const int Capacity = 128;

        private readonly ulong _lo;
        private readonly ulong _hi;

        private DeviceMask(ulong lo, ulong hi)
        {
            _lo = lo;
            _hi = hi;
        }

        public static DeviceMask FromDevices(IEnumerable<int> devices)
        {
            if (devices is null)
            {
                throw new ArgumentNullException(nameof(devices));
            }
            ulong lo = 0;
            ulong hi = 0;
            foreach (var device in devices)
            {
                if (device < 0 || device >= Capacity)
                {
                    continue;
                }
                if (device < 64)
                {
                    lo |= 1UL << device;
                }
                else
                {
                    hi |= 1UL << (device - 64);
                }
            }
            return new DeviceMask(lo, hi);
        }

        public bool this[int device] => Contains(device);

        public bool Contains(int device)
        {
            if (device < 0 || device >= Capacity)
            {
                return false;
            }
            return device < 64 ? (_lo >> device & 1UL) == 1UL : (_hi >> (device - 64) & 1UL) == 1UL;
        }

        public bool IsEmpty => _lo == 0 && _hi == 0;

        public int Count => PopCount(_lo) + PopCount(_hi);

        public IEnumerator<int> GetEnumerator() => Enumerate(_lo, _hi);

        IEnumerator IEnumerable.GetEnumerator() => GetEnumerator();

        public static DeviceMask operator |(DeviceMask left, DeviceMask right) =>
            new DeviceMask(left._lo | right._lo, left._hi | right._hi);

        public bool Equals(DeviceMask other) => _lo == other._lo && _hi == other._hi;

        public override bool Equals(object? obj) => obj is DeviceMask other && Equals(other);

        public override int GetHashCode() => HashCode.Combine(_lo, _hi);

        public static bool operator ==(DeviceMask left, DeviceMask right) => left.Equals(right);

        public static bool operator !=(DeviceMask left, DeviceMask right) => !left.Equals(right);

        public override string ToString()
        {
            var builder = new StringBuilder("DeviceMask {");
            var first = true;
            foreach (var device in this)
            {
                builder.Append(first ? " " : ", ");
                builder.Append(device);
                first = false;
            }
            builder.Append(" }");
            return builder.ToString();
        }

        private static IEnumerator<int> Enumerate(ulong lo, ulong hi)
        {
            for (var device = 0; device < 64; device++)
            {
                if ((lo >> device & 1UL) == 1UL)
                {
                    yield return device;
                }
            }
            for (var device = 0; device < 64; device++)
            {
                if ((hi >> device & 1UL) == 1UL)
                {
                    yield return device + 64;
                }
            }
        }

        private static int PopCount(ulong bits)
        {
            var count = 0;
            while (bits != 0)
            {
                bits &= bits - 1;
                count++;
            }
            return count;
        }
    }
}
