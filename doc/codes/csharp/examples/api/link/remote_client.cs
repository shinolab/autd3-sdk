using System;
using AUTD3.Link;

namespace DocSamples.ApiLinkRemoteClient;

internal static class Sample
{
    internal static void Run()
    {
        var addr = "127.0.0.1:8080";

        // ANCHOR: api
        new RemoteLinkOption(addr);
        // ANCHOR_END: api
    }

    internal static void Discover()
    {
        // ANCHOR: discover
        var option = RemoteLinkOption.Discover();
        // ANCHOR_END: discover
    }

    internal static void DiscoverWithOption()
    {
        // ANCHOR: discover_option
        var option = RemoteLinkOption.Discover(TimeSpan.FromSeconds(5), "autd3-0a1b2c3d");
        // ANCHOR_END: discover_option
    }
}
