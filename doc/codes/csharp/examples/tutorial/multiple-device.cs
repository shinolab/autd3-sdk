using System;
using System.Numerics;
using AUTD3;

namespace AUTD3.DocSamples.TutorialMultipleDevice;

internal static class Sample
{
    internal static void Run()
    {
        // ANCHOR: translation
        new Geometry(new[]
        {
            new Device(Vector3.Zero, Quaternion.Identity),
            new Device(
                new Vector3(Device.Width, 0.0f, 0.0f),
                Quaternion.Identity
            ),
        });
        // ANCHOR_END: translation

        // ANCHOR: global
        new Geometry(new[]
        {
            new Device(
                new Vector3(-Device.Width, 0.0f, 0.0f),
                Quaternion.Identity
            ),
            new Device(Vector3.Zero, Quaternion.Identity),
        });
        // ANCHOR_END: global

        // ANCHOR: rotation
        new Geometry(new[]
        {
            new Device(Vector3.Zero, Quaternion.Identity),
            new Device(
                new Vector3(0.0f, 0.0f, Device.Width),
                Quaternion.CreateFromAxisAngle(Vector3.UnitY, MathF.PI / 2.0f)
            ),
        });
        // ANCHOR_END: rotation
    }
}
