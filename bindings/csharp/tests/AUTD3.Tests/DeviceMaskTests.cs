using System.Collections.Generic;
using System.Linq;
using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class DeviceMaskTests
    {
        [Fact]
        public void IndexerTellsWhetherADeviceIsInTheMask()
        {
            var mask = DeviceMask.FromDevices(new[] { 1, 3 });
            Assert.False(mask[0]);
            Assert.True(mask[1]);
            Assert.False(mask[2]);
            Assert.True(mask[3]);
            Assert.True(mask.Contains(3));
            Assert.False(mask.Contains(2));
        }

        [Fact]
        public void IndexerHandlesTheBoundaries()
        {
            var mask = DeviceMask.FromDevices(new[] { 0, 63, 64, 127 });
            Assert.True(mask[0]);
            Assert.True(mask[63]);
            Assert.True(mask[64]);
            Assert.True(mask[127]);
            Assert.False(mask[128]);
            Assert.False(mask[-1]);
            Assert.False(mask[int.MaxValue]);
            Assert.False(mask[int.MinValue]);
            Assert.False(mask.Contains(128));
            Assert.False(mask.Contains(-1));
        }

        [Fact]
        public void CountAndIsEmpty()
        {
            var mask = DeviceMask.FromDevices(new[] { 2, 70, 127 });
            Assert.Equal(3, mask.Count);
            Assert.False(mask.IsEmpty);

            var empty = DeviceMask.FromDevices(new int[0]);
            Assert.Equal(0, empty.Count);
            Assert.True(empty.IsEmpty);
        }

        [Fact]
        public void EnumerationIsAscending()
        {
            var mask = DeviceMask.FromDevices(new[] { 127, 4, 64, 1, 63, 2 });
            Assert.Equal(new[] { 1, 2, 4, 63, 64, 127 }, mask.ToArray());
            var listed = new List<int>();
            foreach (var device in mask)
            {
                listed.Add(device);
            }
            Assert.Equal(new[] { 1, 2, 4, 63, 64, 127 }, listed);
        }

        [Fact]
        public void OrJoinsMasks()
        {
            var joined = DeviceMask.FromDevices(new[] { 0, 100 }) | DeviceMask.FromDevices(new[] { 2, 100, 127 });
            Assert.Equal(new[] { 0, 2, 100, 127 }, joined.ToArray());
            Assert.Equal(4, joined.Count);
        }

        [Fact]
        public void FromDevicesIgnoresOutOfRangeAndDuplicates()
        {
            var mask = DeviceMask.FromDevices(new[] { 3, 0, 3, 128, -1, int.MaxValue, 127, 0 });
            Assert.Equal(new[] { 0, 3, 127 }, mask.ToArray());
            Assert.Equal(3, mask.Count);
            Assert.True(DeviceMask.FromDevices(new[] { 128, -1 }).IsEmpty);
        }

        [Fact]
        public void Equality()
        {
            var a = DeviceMask.FromDevices(new[] { 1, 65 });
            var b = DeviceMask.FromDevices(new[] { 65, 1, 1 });
            var c = DeviceMask.FromDevices(new[] { 1 });
            Assert.Equal(a, b);
            Assert.True(a == b);
            Assert.False(a != b);
            Assert.True(a.Equals((object)b));
            Assert.Equal(a.GetHashCode(), b.GetHashCode());
            Assert.NotEqual(a, c);
            Assert.True(a != c);
            Assert.False(a.Equals((object)1));
            Assert.NotEqual(DeviceMask.FromDevices(new[] { 0 }), DeviceMask.FromDevices(new[] { 64 }));
        }

        [Fact]
        public void DefaultIsEmpty()
        {
            var mask = default(DeviceMask);
            Assert.True(mask.IsEmpty);
            Assert.Equal(0, mask.Count);
            Assert.Empty(mask);
            Assert.False(mask[0]);
            Assert.Equal(DeviceMask.FromDevices(new int[0]), mask);
        }

        [Fact]
        public void ToStringListsDevices()
        {
            Assert.Equal("DeviceMask { 1, 2, 100 }", DeviceMask.FromDevices(new[] { 100, 2, 1 }).ToString());
            Assert.Equal("DeviceMask { }", default(DeviceMask).ToString());
        }
    }
}
