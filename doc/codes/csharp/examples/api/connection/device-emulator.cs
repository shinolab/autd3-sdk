using System.Numerics;
using System.Threading;
using System.Threading.Tasks;
using AUTD3;

namespace DocSamples.ApiConnectionDeviceEmulator;

internal static class Sample
{
    internal static async Task Run()
    {
        var geometry = new Geometry(new[] { new Autd3(Vector3.Zero), new Autd3(new Vector3(192, 0, 0)) });
        using var emulator = new UdpEmulator(geometry.NumDevices);
        var (driver, connector) = Driver.Open(emulator.Option(), geometry.NumDevices);
        new Thread(driver.Run) { IsBackground = true }.Start();
        await using (var client = await Client.OpenAsync(geometry, connector, new ClientConfig()))
        {
            var builder = client.DatagramBuilder();
            builder.Push(new SetSilencer());
            foreach (var frame in builder.Build())
            {
                await client.SendCheckedAsync(frame);
            }
        }

        emulator.Reboot(1);
    }
}
