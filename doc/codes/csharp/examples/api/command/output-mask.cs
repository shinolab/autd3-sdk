using AUTD3;

namespace DocSamples.ApiCommandOutputMask;

internal static class Sample
{
    internal static void Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var masks = geometry.OutputMaskBuffer();

        // ANCHOR: api
        new SetOutputMask(masks);
        // ANCHOR_END: api
    }
}
