using AUTD3.Link;

namespace DocSamples.ApiLinkTwincat;

internal static class Sample
{
    internal static void Run()
    {
        var addr = "169.254.0.1";
        var amsNetId = "169.254.0.1.1.1";
        // ANCHOR: api
        TwinCATLinkOption.Local();

        TwinCATLinkOption.Remote(addr, amsNetId);
        // ANCHOR_END: api
    }
}
