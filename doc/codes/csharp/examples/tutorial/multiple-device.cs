using System;
using System.Numerics;
using AUTD3;

namespace DocSamples.TutorialMultipleDevice;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: translation
        new Geometry(new[]
        {
            new Autd3(Vector3.Zero, Quaternion.Identity),
            new Autd3(
                new Vector3(Autd3.DeviceWidth, 0.0f, 0.0f),
                Quaternion.Identity
            ),
        });
        // ANCHOR_END: translation

        // ANCHOR: global
        new Geometry(new[]
        {
            new Autd3(
                new Vector3(-Autd3.DeviceWidth, 0.0f, 0.0f),
                Quaternion.Identity
            ),
            new Autd3(Vector3.Zero, Quaternion.Identity),
        });
        // ANCHOR_END: global

        // ANCHOR: rotation
        new Geometry(new[]
        {
            new Autd3(Vector3.Zero, Quaternion.Identity),
            new Autd3(
                new Vector3(0.0f, 0.0f, Autd3.DeviceWidth),
                Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.PI / 2.0f)
            ),
        });
        // ANCHOR_END: rotation
    }
}
