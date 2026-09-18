using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

namespace DocSamples.ApiComputePatternGroup;

// ANCHOR: api
// ANCHOR: compute
internal enum Side
{
    Left,
    Right,
}
// ANCHOR_END: compute
// ANCHOR_END: api

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });

        var left = geometry.PatternBuffer();
        var right = geometry.PatternBuffer();
        Pattern.SetPhase(new Phase(0x80), right);
        var dst = geometry.PatternBuffer();
        var center = geometry.Center;
        // ANCHOR: api
        var groups = new TransducerGroups<Side>(geometry, (device, tr) => device.Position(tr).X < center.X ? Side.Left : Side.Right);
        Pattern.Group(geometry, groups, side => side == Side.Left ? left : right, dst);
        // ANCHOR_END: api

        var wavelength = Pattern.Wavelength(340.0f * m / s);
        var foci = new[] { new AmplitudeTarget(center + new Vector3(-30.0f, 0.0f, 150.0f), 5e3f * Pa) };
        var target = center + new Vector3(40.0f, 0.0f, 150.0f);
        // ANCHOR: compute
        Pattern.GroupCompute(geometry, groups, (side, mask, buffer) =>
        {
            if (side == Side.Left)
            {
                Holo.Gspat(geometry, foci, wavelength, new GspatOption(mask: mask), buffer);
            }
            else
            {
                Pattern.Focus(geometry, target, wavelength, buffer);
            }
        }, dst);
        // ANCHOR_END: compute
    }
}
