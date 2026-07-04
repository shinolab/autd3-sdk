using System.Numerics;
using AUTD3;

namespace DocSamples.ApiBasicsGeometry;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: api
        new Geometry(new[] { new Autd3(
            Vector3.Zero,
            Quaternion.Identity
        ) });
        // ANCHOR_END: api

        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero, Quaternion.Identity) });

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
        var center = dev.Center;
        var rotation = dev.Rotation;
        var x = dev.XDirection;
        var y = dev.YDirection;
        var axial = dev.AxialDirection;
        var positions = dev.Positions;
        var directions = dev.Directions;
        var pos0 = dev.Position(0);
        var dir0 = dev.Direction(0);
        // ANCHOR_END: device

        _ = (numDevices, totalTransducers, arrayCenter, first);
        _ = (idx, numTransducers, center, rotation, x, y, axial);
        _ = (positions, directions, pos0, dir0);
    }
}
