using System;
using System.CommandLine;
using System.Linq;

namespace TwincatCli
{
    internal class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            Console.OutputEncoding = System.Text.Encoding.UTF8;

            var clientIpAddr = new Option<string>("--client", "-c")
            {
                Description = "Client IP address. If empty, use localhost.",
                DefaultValueFactory = _ => "",
            };
            clientIpAddr.HelpName = "IP_ADDR";
            var deviceName = new Option<string>("--device_name")
            {
                Description = "Ethernet device name. If empty, use the first device found.",
                DefaultValueFactory = _ => "",
            };
            deviceName.HelpName = "DEV_NAME";
            var sync0CycleTime = new Option<int>("--sync0", "-s")
            {
                Description = "Sync0 cycle time in units of 500μs.",
                DefaultValueFactory = _ => 2,
            };
            sync0CycleTime.HelpName = "CYCLE_TIME";
            var taskCycleTime = new Option<int>("--task", "-t")
            {
                Description = "Task cycle time in units of CPU base time.",
                DefaultValueFactory = _ => 1,
            };
            taskCycleTime.HelpName = "CYCLE_TIME";
            var cpuBaseTime = new Option<string>("--base", "-b")
            {
                Description = "CPU base time.",
                DefaultValueFactory = _ => "1ms",
            };
            cpuBaseTime.AcceptOnlyFromAmong(CpuBaseTimeParser.AvailableTimes().ToArray());
            cpuBaseTime.HelpName = "TIME";
            var tcVersion = new Option<string>("--twincat")
            {
                Description = "TwinCAT version",
                DefaultValueFactory = _ => "4026",
            };
            tcVersion.AcceptOnlyFromAmong(TwinCATVersionParser.AvailableVersions().ToArray());
            var keep = new Option<bool>("--keep", "-k")
            {
                Description = "Keep TwinCAT XAE Shell window open.",
                DefaultValueFactory = _ => false
            };
            var delayTime = new Option<int>("--delay")
            {
                Description = "Delay time to wait for the operation to complete (ms).",
                DefaultValueFactory = _ => 1000,
            };
            delayTime.HelpName = "DELAY_MS";
            var debug = new Option<bool>("--debug", "-d")
            {
                Description = "Enable debug mode.",
                DefaultValueFactory = _ => false
            };
            var twincatRoot = new Option<string>("--twincat-root")
            {
                Description = "TwinCAT 3.1 install directory (the folder %TwinCAT3Dir% points to). Auto-detected if empty.",
                DefaultValueFactory = _ => "",
            };
            twincatRoot.HelpName = "DIR";
            var progId = new Option<string>("--progid")
            {
                Description = "Override the DTE COM ProgID (e.g. VisualStudio.DTE.17.0). Defaults by --twincat version.",
                DefaultValueFactory = _ => "",
            };
            progId.HelpName = "PROGID";

            var runCommand = new Command("run", "Scan AUTD devices and set up a TwinCAT project.");
            runCommand.Options.Add(clientIpAddr);
            runCommand.Options.Add(deviceName);
            runCommand.Options.Add(sync0CycleTime);
            runCommand.Options.Add(taskCycleTime);
            runCommand.Options.Add(cpuBaseTime);
            runCommand.Options.Add(tcVersion);
            runCommand.Options.Add(keep);
            runCommand.Options.Add(delayTime);
            runCommand.Options.Add(debug);
            runCommand.Options.Add(twincatRoot);
            runCommand.Options.Add(progId);

            runCommand.SetAction(parseResult =>
            {
                var clientIp = parseResult.GetValue(clientIpAddr);
                var devName = parseResult.GetValue(deviceName);
                var sync0Cycle = parseResult.GetValue(sync0CycleTime);
                var taskCycle = parseResult.GetValue(taskCycleTime);
                var baseTime = CpuBaseTimeParser.Parse(parseResult.GetResult(cpuBaseTime));
                var version = TwinCATVersionParser.Parse(parseResult.GetResult(tcVersion));
                var keepOpen = parseResult.GetValue(keep);
                var delay= parseResult.GetValue(delayTime);
                var debugMode = parseResult.GetValue(debug);
                var tcRoot = parseResult.GetValue(twincatRoot);
                var dteProgId = parseResult.GetValue(progId);
                Setup(version, clientIp, devName, sync0Cycle, taskCycle, baseTime, keepOpen, delay, debugMode, tcRoot, dteProgId);
            });

            var openVersion = new Option<string>("--twincat")
            {
                Description = "TwinCAT version",
                DefaultValueFactory = _ => "4026",
            };
            openVersion.AcceptOnlyFromAmong(TwinCATVersionParser.AvailableVersions().ToArray());
            var openProgId = new Option<string>("--progid")
            {
                Description = "Override the DTE COM ProgID (e.g. VisualStudio.DTE.17.0). Defaults by --twincat version.",
                DefaultValueFactory = _ => "",
            };
            openProgId.HelpName = "PROGID";
            var openTwincatRoot = new Option<string>("--twincat-root")
            {
                Description = "TwinCAT 3.1 install directory (the folder %TwinCAT3Dir% points to). Auto-detected if empty.",
                DefaultValueFactory = _ => "",
            };
            openTwincatRoot.HelpName = "DIR";
            var openDebug = new Option<bool>("--debug", "-d")
            {
                Description = "Enable debug mode.",
                DefaultValueFactory = _ => false
            };

            var openCommand = new Command("open", "Open the already-saved TwinCAT project (for when --keep was forgotten).");
            openCommand.Options.Add(openVersion);
            openCommand.Options.Add(openProgId);
            openCommand.Options.Add(openTwincatRoot);
            openCommand.Options.Add(openDebug);

            openCommand.SetAction(parseResult =>
            {
                var version = TwinCATVersionParser.Parse(parseResult.GetResult(openVersion));
                var dteProgId = parseResult.GetValue(openProgId);
                var tcRoot = parseResult.GetValue(openTwincatRoot);
                var debugMode = parseResult.GetValue(openDebug);
                (new SetupTwinCAT(version, dteProgId, tcRoot, debugMode)).Open();
            });

            var doctorCommand = new Command("doctor", "Diagnose virtualization-based security (must be OFF for TwinCAT real-time).");
            doctorCommand.SetAction(_ => Doctor.Run());

            var installEsiCommand = new Command("install-esi", "Install the bundled AUTD ESI (AUTD.xml) into the TwinCAT config directory.");
            installEsiCommand.SetAction(_ => { EsiInstaller.Install(true); });

            var rootCommand = new RootCommand("TwinCAT AUTD3 server");
            rootCommand.Subcommands.Add(runCommand);
            rootCommand.Subcommands.Add(openCommand);
            rootCommand.Subcommands.Add(doctorCommand);
            rootCommand.Subcommands.Add(installEsiCommand);

            return rootCommand.Parse(args).Invoke();
        }

        [STAThread]
        private static void Setup(TwinCATVersion version, string clientIpAddr, string deviceName, int sync0CycleTime, int taskCycleTime, CpuBaseTime cpuBaseTime, bool keep, int delayTime, bool debugMode, string twinCatRoot, string progId)
        {
            try
            {
                EsiInstaller.Install(false);
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"warning: {e.Message}");
            }

            var baseTime = CpuBaseTimeParser.ToValueUnitsOf100ns(cpuBaseTime);
            var sync0CycleTimeInNs = 500000 * sync0CycleTime;
            (new SetupTwinCAT(version, clientIpAddr, deviceName, baseTime * taskCycleTime, baseTime, sync0CycleTimeInNs, keep, delayTime, debugMode, twinCatRoot, progId)).Run();
        }
    }
}