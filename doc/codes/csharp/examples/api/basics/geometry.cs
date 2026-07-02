using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.ApiBasicsGeometry;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        new Geometry(new[] { new Device(Vector3.Zero, Quaternion.Identity) });
        // ANCHOR_END: api

        var geometry = new Geometry(new[] { new Device(Vector3.Zero, Quaternion.Identity) });

        // ANCHOR: access
        var numDevices = geometry.NumDevices;
        var totalTransducers = geometry.NumTransducers;
        var arrayCenter = geometry.Center;

        for (var i = 0; i < geometry.NumDevices; i++)
        {
            var device = geometry[i];
        }

        var first = geometry[0];
        // ANCHOR_END: access

        var dev = geometry[0];
        // ANCHOR: device
        var idx = dev.Idx;
        var numTransducers = dev.NumTransducers;
        var rotation = dev.Rotation;
        var x = dev.XDirection;
        var y = dev.YDirection;
        var axial = dev.AxialDirection;
        // ANCHOR_END: device

        _ = (numDevices, totalTransducers, arrayCenter, first);
        _ = (idx, numTransducers, rotation, x, y, axial);
    }
}
