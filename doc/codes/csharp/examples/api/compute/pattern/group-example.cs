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
var geometry = new Geometry(new[] { new Autd3() });
var wavelength = Pattern.Wavelength(340.0f * m / s);
var center = geometry.Center;

var groups = new TransducerGroups<Side>(geometry, (device, tr) => device.Position(tr).X < center.X ? Side.Left : Side.Right);

var foci = new[]
{
    new AmplitudeTarget(center + Offset(-50.0f * mm, 0.0f * mm, 150.0f * mm), 5e3f * Pa),
    new AmplitudeTarget(center + Offset(-20.0f * mm, 0.0f * mm, 150.0f * mm), 5e3f * Pa),
};

var phases = geometry.PhaseBuffer();
var intensities = geometry.IntensityBuffer();
Pattern.GroupCompute(geometry, groups, (side, mask, p, i) =>
{
    if (side == Side.Left)
    {
        Holo.Gspat(geometry, foci, wavelength, new GspatOption { Mask = mask }, p, i);
    }
    else
    {
        Pattern.Focus(geometry, center + Offset(40.0f * mm, 0.0f * mm, 150.0f * mm), wavelength, p);
    }
}, phases, intensities);
        // HIDE
    }
}
// HIDE_END
