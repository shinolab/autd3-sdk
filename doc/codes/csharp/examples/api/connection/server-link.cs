using System;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiConnectionServerLink;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3() });

        var port = ServerLink.ServerPort;
        var timeout = TimeSpan.FromSeconds(1);
        // ANCHOR: api
        var link = new ServerLink { Port = port, Timeout = timeout };
        var client = await Client.OpenAsync(geometry, link, new ClientConfig());
        // ANCHOR_END: api
    }
}
