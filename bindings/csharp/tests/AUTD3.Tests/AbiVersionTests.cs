using System;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
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
            using var geometry = Fixture.SingleDevice();
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

        [Fact]
        public void EachNativeClassImportsFromASingleLibrary()
        {
            var assemblies = new[]
            {
                typeof(Geometry).Assembly,
                typeof(Pattern).Assembly,
                typeof(Modulation).Assembly,
                typeof(AUTD3.Holo.Holo).Assembly,
                typeof(Client).Assembly,
            };
            var libraries = assemblies
                .SelectMany(assembly => assembly.GetTypes())
                .Select(type => (
                    type,
                    libs: type.GetMethods(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly)
                        .Select(method => method.GetCustomAttribute<DllImportAttribute>()?.Value)
                        .OfType<string>()
                        .Distinct()
                        .ToArray()))
                .Where(entry => entry.libs.Length > 0)
                .ToArray();
            Assert.NotEmpty(libraries);
            foreach (var (type, libs) in libraries)
            {
                Assert.True(libs.Length == 1, $"{type.FullName} imports from {string.Join(", ", libs)}");
            }
        }

        [Theory]
        [InlineData(typeof(Pattern))]
        [InlineData(typeof(Modulation))]
        [InlineData(typeof(AUTD3.Holo.Holo))]
        public void ComputationLayersDoNotImportTheClientLibrary(Type anchor)
        {
            var offenders = anchor.Assembly.GetTypes()
                .Where(type => !type.Name.EndsWith("Op", StringComparison.Ordinal))
                .Where(type => type.GetMethods(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly)
                    .Any(method => method.GetCustomAttribute<DllImportAttribute>()?.Value == "autd3capi"))
                .Select(type => type.FullName)
                .ToArray();
            Assert.Empty(offenders);
        }
    }
}
