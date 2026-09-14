using System.Numerics;
using AUTD3;
using AUTD3.Holo;
using static AUTD3.Units;
using static AUTD3.Holo.HoloUnits;

// HIDE
namespace DocSamples.ApiComputePatternGroupExample;

// HIDE_END
internal enum Side
{
    Left,
    Right,
}

// HIDE
internal static class Sample
{
    internal static void Run()
    {
        // HIDE_END
var geometry = new Geometry(new[] { new Autd3(Vector3.Zero) });
var wavelength = Pattern.Wavelength(340.0f * m / s);
var center = geometry.Center;

var groups = new TransducerGroups<Side>(geometry, (device, tr) => device.Position(tr).X < center.X ? Side.Left : Side.Right);

var foci = new[]
{
    new AmplitudeTarget(center + new Vector3(-50.0f, 0.0f, 150.0f), 5e3f * Pa),
    new AmplitudeTarget(center + new Vector3(-20.0f, 0.0f, 150.0f), 5e3f * Pa),
};

var dst = geometry.PatternBuffer();
Pattern.GroupCompute(geometry, groups, (side, mask, buffer) =>
{
    if (side == Side.Left)
    {
        Holo.Gspat(geometry, foci, wavelength, new GspatOption(mask: mask), buffer);
    }
    else
    {
        Pattern.Focus(geometry, center + new Vector3(40.0f, 0.0f, 150.0f), wavelength, new FocusOption(), buffer);
    }
}, dst);
        // HIDE
    }
}
// HIDE_END
