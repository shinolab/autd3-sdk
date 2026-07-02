using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiCommandOutputMask;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Device(Vector3.Zero) });

        var masks = new bool[geometry.NumDevices][];
        for (var i = 0; i < geometry.NumDevices; i++)
        {
            var count = geometry[i].NumTransducers;
            masks[i] = new bool[count];
            for (var t = 0; t < count; t++)
            {
                masks[i][t] = true;
            }
        }

        // ANCHOR: api
        new SetOutputMask(masks);
        // ANCHOR_END: api
    }
}
