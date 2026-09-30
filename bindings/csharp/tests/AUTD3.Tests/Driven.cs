using System.Threading;

namespace AUTD3.Tests
{
    internal static class Driven
    {
        internal static Connector Start(UdpEmulator emulator, Geometry geometry)
        {
            var (driver, connector) = Driver.Open(emulator.Option(), geometry.NumDevices);
            new Thread(() =>
            {
                using (driver)
                {
                    try
                    {
                        driver.Run();
                    }
                    catch (Autd3Exception)
                    {
                    }
                }
            })
            { IsBackground = true }.Start();
            return connector;
        }
    }
}
