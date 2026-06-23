// Multiple AUTD3 devices side by side. Run with: cargo xtask cs example MultiDevice

using System;
using System.Collections.Generic;
using System.Numerics;
using System.Threading.Tasks;
using AUTD3;
using AUTD3.Link;

internal static class Program
{
    private static async Task Main()
    {
        using var geometry = new Geometry(new List<Device>
        {
            new Device(Vector3.Zero),
            new Device(new Vector3(Device.Width, 0f, 0f)),
        });

        using var client = await Client.OpenAsync(geometry, EtherCrabLink.Create(), new ClientConfig());

        Console.WriteLine($"devices: {client.NumDevices}");
        var versions = await client.ReadFirmwareVersionAsync();
        for (var i = 0; i < versions.Count; i++)
        {
            Console.WriteLine($"device[{i}] firmware version: {versions[i]}");
        }

        var center = geometry.Center;
        Console.WriteLine($"array center: ({center.X:F2}, {center.Y:F2}, {center.Z:F2}) mm");

        await client.CloseAsync();
    }
}
