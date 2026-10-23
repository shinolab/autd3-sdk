using System.Linq;
using System.Numerics;

namespace AUTD3.Tests
{
    internal static class Fixture
    {
        internal static Geometry SingleDevice() => Devices(1);

        internal static Geometry Devices(int n) =>
            new Geometry(Enumerable.Range(0, n).Select(_ => new Autd3(Vector3.Zero)).ToArray());

        internal static void Fill(IntensityBuffer buffer, Intensity value)
        {
            foreach (var device in buffer)
            {
                for (var tr = 0; tr < device.NumTransducers; tr++)
                {
                    var slot = device;
                    slot[tr] = value;
                }
            }
        }
    }
}
