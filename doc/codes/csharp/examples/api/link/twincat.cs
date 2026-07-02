using AUTD3.Link;

namespace AUTD3.DocSamples.ApiLinkTwincat;

internal static class Sample
{
    internal static void Run()
    {
        var addr = "169.254.0.1";
        var amsNetId = "169.254.0.1.1.1";
        // ANCHOR: api
        TwinCATLink.Local();

        TwinCATLink.Remote(addr, amsNetId);
        // ANCHOR_END: api
    }
}
