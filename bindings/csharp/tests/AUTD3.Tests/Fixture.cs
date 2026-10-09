using System.Linq;
using System.Numerics;
using System.Threading.Tasks;

namespace AUTD3.Tests
{
    internal static class Fixture
    {
        internal static Geometry SingleDevice() => Devices(1);

        internal static Geometry Devices(int n) =>
            new Geometry(Enumerable.Range(0, n).Select(_ => new Autd3(Vector3.Zero)).ToArray());

        internal static Task<Client> OpenAsync(UdpEmulator emulator, Geometry geometry) =>
            Client.OpenAsync(geometry, emulator.Option(), new ClientConfig());
    }
}
