using System.Numerics;
using AUTD3;

namespace DocSamples.ApiCommandPhaseCorrection;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var phases = new Phase[geometry.NumDevices][];
        for (var i = 0; i < geometry.NumDevices; i++)
        {
            var count = geometry[i].NumTransducers;
            phases[i] = new Phase[count];
            for (var t = 0; t < count; t++)
            {
                phases[i][t] = Phase.Zero;
            }
        }

        // ANCHOR: api
        new SetPhaseCorrection(phases);
        // ANCHOR_END: api
    }
}
