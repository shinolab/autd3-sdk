using AUTD3;
using Xunit;

namespace AUTD3.Tests
{
    public class AbiVersionTests
    {
        private static uint Pack(ushort major, ushort minor, ushort patch) =>
            ((uint)major << 20) | ((uint)minor << 10) | patch;

        [Fact]
        public void TheNativeLibraryReportsTheVersionTheBindingExpects()
        {
            using var geometry = new Geometry(new[] { new Autd3(System.Numerics.Vector3.Zero) });
            Assert.Equal(1, geometry.NumDevices);
        }

        [Fact]
        public void AnExactVersionMatchIsAccepted()
        {
            NativeAbi.Verify("test", Pack(NativeAbi.Major, NativeAbi.Minor, NativeAbi.Patch));
        }

        [Theory]
        [InlineData(1, 0, 0)]
        [InlineData(0, 1, 0)]
        [InlineData(0, 0, 1)]
        [InlineData(0, 0, -1)]
        public void AnyVersionComponentMismatchIsRejected(int dMajor, int dMinor, int dPatch)
        {
            var actual = Pack(
                (ushort)(NativeAbi.Major + dMajor),
                (ushort)(NativeAbi.Minor + dMinor),
                (ushort)(NativeAbi.Patch + dPatch));
            Assert.Throws<Autd3Exception>(() => NativeAbi.Verify("test", actual));
        }
    }
}
