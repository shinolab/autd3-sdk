using EnvDTE;
using EnvDTE80;
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using System.Text.RegularExpressions;
using System.Xml;
using System.Xml.Linq;
using TCatSysManagerLib;
using TwinCAT.Ads;
using TwinCAT.SystemManager;

namespace TwincatCli
{

    internal class SetupTwinCAT
    {
        private const string SolutionName = "twincat-cli";
        private const int HeadSize = 64;
        private const int BodySize = 249;

        private readonly TwinCATVersion _version;
        private readonly string _clientIpAddr;
        private readonly string _deviceName;
        private readonly int _taskCycleTime;
        private readonly int _cpuBaseTime;
        private readonly int _sync0CycleTime;
        private readonly bool _keep;
        private readonly int _delayTime;
        private readonly bool _debug;
        private readonly string _twinCatRoot;
        private readonly string _progId;

        internal SetupTwinCAT(TwinCATVersion version, string clientIpAddr, string deviceName, int taskCycleTime, int cpuBaseTime, int sync0CycleTime, bool keep, int delayTime, bool debug, string twinCatRoot, string progId)
        {
            if (debug)
            {
                Console.WriteLine($"TwinCAT version: {version}");
                Console.WriteLine($"Client IP: {clientIpAddr}");
                Console.WriteLine($"Device Name: {deviceName}");
                Console.WriteLine($"Sync0 Cycle Time: {sync0CycleTime}");
                Console.WriteLine($"Task Cycle Time: {taskCycleTime}");
                Console.WriteLine($"CPU Base Time: {cpuBaseTime}");
                Console.WriteLine($"Delay Time: {delayTime} ms");
                Console.WriteLine($"Keep Open: {keep}");
            }

            _version = version;
            _clientIpAddr = clientIpAddr;
            _deviceName = deviceName;
            _taskCycleTime = taskCycleTime;
            _cpuBaseTime = cpuBaseTime;
            _sync0CycleTime = sync0CycleTime;
            _keep = keep;
            _delayTime = delayTime;
            _debug = debug;
            _twinCatRoot = twinCatRoot;
            _progId = progId;
        }

        internal SetupTwinCAT(TwinCATVersion version, string progId, string twinCatRoot, bool debug)
        {
            _version = version;
            _progId = progId;
            _twinCatRoot = twinCatRoot;
            _debug = debug;
        }

        private string ProgID()
        {
            if (!string.IsNullOrWhiteSpace(_progId))
                return _progId;
            switch (_version)
            {
                case TwinCATVersion.Build4024:
                    return "TcXaeShell.DTE.15.0";
                case TwinCATVersion.Build4026:
                    return "TcXaeShell.DTE.17.0";
            }
            throw new Exception($"TwinCAT version {_version} is not supported.");
        }

        private string TemplatePath()
        {
            const string relTemplate = @"Components\Base\PrjTemplate\TwinCAT Project.tsproj";
            var roots = new List<string>();
            if (!string.IsNullOrWhiteSpace(_twinCatRoot)) roots.Add(_twinCatRoot);
            var envRoot = Environment.GetEnvironmentVariable("TWINCAT3DIR");
            if (!string.IsNullOrWhiteSpace(envRoot)) roots.Add(envRoot);
            roots.Add(@"C:\TwinCAT\3.1");
            roots.Add(@"C:\Program Files (x86)\Beckhoff\TwinCAT\3.1");

            var tried = new List<string>();
            foreach (var root in roots)
            {
                var path = Path.Combine(root, relTemplate);
                if (File.Exists(path)) return path;
                tried.Add(path);
            }
            throw new FileNotFoundException(
                "TwinCAT project template not found. Tried:\n  " + string.Join("\n  ", tried) +
                "\nSet %TwinCAT3Dir% or pass --twincat-root <TwinCAT 3.1 directory>.");
        }

        [STAThread]
        public void Run()
        {
            var solutionPath = Path.Combine(Environment.GetEnvironmentVariable("temp") ?? string.Empty, SolutionName);
            MessageFilter.Register();
            DTE2 dte = null;
            try
            {
                var processes = System.Diagnostics.Process.GetProcesses().Where(x => x.MainWindowTitle.StartsWith(SolutionName) && x.ProcessName.Contains("TcXaeShell"));
                foreach (var process in processes) GetDte(process.Id)?.Quit();

                IPAddress.TryParse(_clientIpAddr ?? string.Empty, out var ipAddr);

                Console.WriteLine("Connecting to TcXaeShell DTE...");
                var t = Type.GetTypeFromProgID(ProgID());
                dte = (DTE2)Activator.CreateInstance(t);

                dte.SuppressUI = false;
                dte.MainWindow.Visible = true;
                dte.UserControl = true;

                Console.WriteLine("Switching TwinCAT3 to Config Mode...");
                SetConfigMode();
                System.Threading.Thread.Sleep(_delayTime);
                Console.WriteLine("Creating a Project...");
                Project project;
                try
                {
                    project = CreateProject(dte, solutionPath);
                }
                catch (Exception ex)
                {
                    throw new Exception(
                        ex.Message + Environment.NewLine +
                        "Note: installing the TcXaeShell alone is not enough. The TwinCAT XAE " +
                        "integration must also be installed, otherwise the .tsproj project type / " +
                        "template cannot be loaded. See tools/twincat-cli/README.md.",
                        ex);
                }
                ITcSysManager sysManager = (ITcSysManager)project.Object;
                if (ipAddr != null)
                {
                    Console.WriteLine("Setting up the Routing Table to " + ipAddr);
                    AddRoute(sysManager, ipAddr);
                }
                Console.WriteLine("Scanning Devices...");
                System.Threading.Thread.Sleep(_delayTime);
                var autds = ScanAUTDs(sysManager);
                AssignCpuCores(sysManager);
                SetupTask(sysManager, autds);
                Console.WriteLine("Activating and Restarting TwinCAT3...");
                sysManager.ActivateConfiguration();
                sysManager.StartRestartTwinCAT();

                if (ipAddr != null)
                {
                    var system = sysManager.LookupTreeItem("SYSTEM");
                    var systemXml = system.ProduceXml(true);
                    var amsNetIdReg = Regex.Match(systemXml, @"<AmsNetId>(?<AmsNetId>.*)</AmsNetId>").Groups["AmsNetId"].Value;
                    Console.WriteLine($"Server AmsNetId: {amsNetIdReg}");
                    Console.WriteLine($"Client AmsNetId: {ipAddr}.1.1");
                }

                Console.WriteLine($"Saving the Project...");
                SaveProject(dte, project, solutionPath);
            }
            catch (Exception e)
            {
                Console.Write("Error: ");
                Console.WriteLine(e.Message);
            }

            if (!_keep) dte?.Quit();

            MessageFilter.Revoke();
        }

        [STAThread]
        public void Open()
        {
            var solutionPath = Path.Combine(Environment.GetEnvironmentVariable("temp") ?? string.Empty, SolutionName);
            var slnFile = Path.Combine(solutionPath, SolutionName + ".sln");
            if (!File.Exists(slnFile))
            {
                Console.WriteLine($"No saved TwinCAT project found at {slnFile}.");
                Console.WriteLine("Run `cargo xtask tool twincat run -- --keep` first to create and keep one.");
                return;
            }

            MessageFilter.Register();
            DTE2 dte = null;
            try
            {
                Console.WriteLine("Connecting to TcXaeShell DTE...");
                var t = Type.GetTypeFromProgID(ProgID());
                dte = (DTE2)Activator.CreateInstance(t);

                dte.SuppressUI = false;
                dte.MainWindow.Visible = true;
                dte.UserControl = true;

                Console.WriteLine($"Opening {slnFile}...");
                dte.Solution.Open(slnFile);
            }
            catch (Exception e)
            {
                Console.Write("Error: ");
                Console.WriteLine(e.Message);
            }

            MessageFilter.Revoke();
        }

        [DllImport("ole32.dll")]
        private static extern int CreateBindCtx(uint reserved, out IBindCtx ppbc);

        public DTE GetDte(int processId)
        {
            var progId = $"!{ProgID()}:{processId}";
            object runningObject = null;

            IBindCtx bindCtx = null;
            IRunningObjectTable rot = null;
            IEnumMoniker enumMonikers = null;

            try
            {
                Marshal.ThrowExceptionForHR(CreateBindCtx(0, out bindCtx));
                bindCtx.GetRunningObjectTable(out rot);
                rot.EnumRunning(out enumMonikers);

                var moniker = new IMoniker[1];
                var numberFetched = IntPtr.Zero;
                while (enumMonikers.Next(1, moniker, numberFetched) == 0)
                {
                    var runningObjectMoniker = moniker[0];
                    string name = null;
                    try
                    {
                        runningObjectMoniker?.GetDisplayName(bindCtx, null, out name);
                    }
                    catch (UnauthorizedAccessException)
                    {
                        // Do nothing, there is something in the ROT that we do not have access to.
                    }

                    if (string.IsNullOrEmpty(name) || !string.Equals(name, progId, StringComparison.Ordinal)) continue;
                    Marshal.ThrowExceptionForHR(rot.GetObject(runningObjectMoniker, out runningObject));
                    break;
                }
            }
            finally
            {
                if (enumMonikers != null) Marshal.ReleaseComObject(enumMonikers);
                if (rot != null) Marshal.ReleaseComObject(rot);
                if (bindCtx != null) Marshal.ReleaseComObject(bindCtx);
            }
            return (DTE)runningObject;
        }

        private static void SetConfigMode()
        {
            var client = new AdsClient();
            var mode = new StateInfo();

            client.Connect((int)AmsPort.SystemService);
            mode.AdsState = client.ReadState().AdsState;
            mode.AdsState = AdsState.Reconfig;
            client.WriteControl(mode);
            client.Dispose();
        }

        private static void DeleteDirectory(string path)
        {
            foreach (var directory in Directory.GetDirectories(path))
                DeleteDirectory(directory);

            try
            {
                Directory.Delete(path, true);
            }
            catch (IOException)
            {
                Directory.Delete(path, true);
            }
            catch (UnauthorizedAccessException)
            {
                Directory.Delete(path, true);
            }
        }

        private Project CreateProject(DTE2 dte, string path)
        {
            if (Directory.Exists(path))
                DeleteDirectory(path);
            Directory.CreateDirectory(path);

            var templatePath = TemplatePath();

            var solution = dte.Solution as Solution2;
            solution.Create(path, SolutionName);
            solution.SaveAs(Path.Combine(path, SolutionName + ".sln"));

            return solution.AddFromTemplate(templatePath, path, SolutionName);
        }

        private static void SaveProject(DTE2 dte, Project project, string path)
        {
            project.Save();
            dte.Solution.SaveAs(Path.Combine(path, SolutionName + ".sln"));
            Console.WriteLine("The Solution was saved at " + path + ".");
        }

        private static void AddRoute(ITcSysManager sysManager, IPAddress ipAddr)
        {
            var routeConfiguration = sysManager.LookupTreeItem("TIRR");
            var addProjectRouteIp = @"<TreeItem>
                                           <RoutePrj>
                                             <AddProjectRoute>
                                               <Name>" + ipAddr + @"</Name>
                                               <NetId>" + ipAddr + @".1.1" + @"</NetId>
                                               <IpAddr>" + ipAddr + @"</IpAddr>
                                             </AddProjectRoute>
                                           </RoutePrj>
                                         </TreeItem>";

            routeConfiguration.ConsumeXml(addProjectRouteIp);
        }

        private List<ITcSmTreeItem> ScanAUTDs(ITcSysManager sysManager)
        {
            var devices = (ITcSmTreeItem3)sysManager.LookupTreeItem("TIID");
            var doc = new XmlDocument();
            var xml = devices.ProduceXml(false);
            doc.LoadXml(xml);
            var nodes = doc.SelectNodes("TreeItem/DeviceGrpDef/FoundDevices/Device");
            var ethernetNodes = (from XmlNode node in nodes let typeNode = node.SelectSingleNode("ItemSubType") let subType = int.Parse(typeNode.InnerText) where subType == (int)DeviceType.EtherCAT_AutomationProtocol || subType == (int)DeviceType.EtherCAT_DirectMode || subType == (int)DeviceType.EtherCAT_DirectModeV210 select node).ToList();

            if (ethernetNodes.Count == 0)
                throw new Exception("No TwinCAT RT-Ethernet adapters were found.");

            if (ethernetNodes.Count == 1)
                Console.WriteLine("Scan found a RT-Ethernet adapter.");
            else
                Console.WriteLine($"Scan found {ethernetNodes.Count} RT-Ethernet adapters.");

            if (_debug)
            {
                Console.WriteLine("Found Ethernet adapters:");
                foreach (var node in ethernetNodes)
                {
                    var addrinfo = node.SelectSingleNode("AddressInfo");
                    Console.WriteLine($"\t{addrinfo.SelectSingleNode("Pnp/DeviceName").InnerText} ({addrinfo.SelectSingleNode("Pnp/DeviceDesc").InnerText})");
                }
            }

            XmlNode ethernetNode = null;

            if (_deviceName == "")
            {
                ethernetNode = ethernetNodes[0];
                if (ethernetNodes.Count != 1)
                {
                    var addrinfo = ethernetNode.SelectSingleNode("AddressInfo");
                    Console.WriteLine($"Multiple RT-Ethernet adapters found, but no device name specified. Using {addrinfo.SelectSingleNode("Pnp/DeviceName").InnerText}.");
                }
            }
            else
            {
                foreach (var node in ethernetNodes)
                {
                    var addrinfo = node.SelectSingleNode("AddressInfo");
                    if (addrinfo.SelectSingleNode("Pnp/DeviceName").InnerText == _deviceName)
                    {
                        ethernetNode = node;
                        break;
                    }
                }
                if (ethernetNode == null)
                    throw new Exception($"No RT-Ethernet adapter with name {_deviceName} found.");
            }

            var device = (ITcSmTreeItem3)devices.CreateChild("EtherCAT Master", (int)DeviceType.EtherCAT_DirectMode, null);

            var addressInfoNode = ethernetNode.SelectSingleNode("AddressInfo");
            addressInfoNode.SelectSingleNode("Pnp/DeviceDesc").InnerText = "TwincatEthernetDevice";
            var xml2 = $"<TreeItem><DeviceDef>{addressInfoNode.OuterXml}</DeviceDef></TreeItem>";
            device.ConsumeXml(xml2);

            const string scanXml = "<TreeItem><DeviceDef><ScanBoxes>1</ScanBoxes></DeviceDef></TreeItem>";
            device.ConsumeXml(scanXml);
            var autds = new List<ITcSmTreeItem>();
            foreach (ITcSmTreeItem box in device)
            {
                if (box.ItemSubTypeName != "AUTD") continue;
                var bdoc = new XmlDocument();
                var bxml = box.ProduceXml(false);
                bdoc.LoadXml(bxml);

                if (_debug && autds.Count == 0)
                {
                    Console.WriteLine("Box XML (InfoData/State schema inspection):");
                    Console.WriteLine(bxml);
                }

                // set DC
                {
                    var dcOpmodes = bdoc.SelectNodes("TreeItem/EtherCAT/Slave/DC/OpMode");
                    foreach (XmlNode item in dcOpmodes)
                    {
                        if (item.SelectSingleNode("Name")?.InnerText == "DC")
                        {
                            var attr = bdoc.CreateAttribute("Selected");
                            attr.Value = "true";
                            item.Attributes?.SetNamedItem(attr);

                            item.SelectSingleNode("CycleTimeSync0").InnerText = _sync0CycleTime.ToString();
                            attr = bdoc.CreateAttribute("Factor");
                            attr.Value = "0";
                            item.Attributes?.SetNamedItem(attr);
                            item.SelectSingleNode("CycleTimeSync0").Attributes?.SetNamedItem(attr);
                        }
                        else
                        {
                            item.Attributes?.RemoveAll();
                        }
                    }
                }

                box.ConsumeXml(bdoc.OuterXml);

                autds.Add(box);
            }

            if (autds.Count == 0)
                throw new Exception("No AUTD devices were found.");

            Console.WriteLine($"{autds.Count} AUTD device{(autds.Count == 1 ? " is" : "s are")} found and added.");

            return autds;
        }

        private void SetupTask(ITcSysManager sysManager, IReadOnlyCollection<ITcSmTreeItem> autds)
        {
            var tasks = sysManager.LookupTreeItem("TIRT");
            var task1 = tasks.CreateChild("Task 1", 0, null);
            var doc = new XmlDocument();
            var xml = task1.ProduceXml(false);
            doc.LoadXml(xml);

            doc.SelectSingleNode("TreeItem/TaskDef/CycleTime").InnerText = _taskCycleTime.ToString();
            task1.ConsumeXml(doc.OuterXml);

            var task1Out = sysManager.LookupTreeItem("TIRT^Task 1^Outputs");
            for (var id = 0; id < autds.Count; id++)
            {
                for (var i = 0; i < HeadSize; i++)
                {
                    var name = $"header[{id}][{i}]";
                    task1Out.CreateChild(name, -1, null, "WORD");
                }
                for (var i = 0; i < BodySize; i++)
                {
                    var name = $"gbody[{id}][{i}]";
                    task1Out.CreateChild(name, -1, null, "WORD");
                }
            }
            var task1In = sysManager.LookupTreeItem("TIRT^Task 1^Inputs");
            for (var id = 0; id < autds.Count; id++)
            {
                var name = $"input[{id}]";
                task1In.CreateChild(name, -1, null, "WORD");
            }
            for (var id = 0; id < autds.Count; id++)
            {
                var name = $"state[{id}]";
                task1In.CreateChild(name, -1, null, "WORD");
            }
            for (var id = 0; id < autds.Count; id++)
            {
                for (var i = 0; i < HeadSize; i++)
                {
                    var source = $"TIRT^Task 1^Outputs^header[{id}][{i}]";
                    var destination = $"TIID^EtherCAT Master^Box {id + 1} (AUTD)^RxPdo0^data[{i}]";
                    sysManager.LinkVariables(source, destination);
                }
                for (var i = 0; i < BodySize - HeadSize; i++)
                {
                    var source = $"TIRT^Task 1^Outputs^gbody[{id}][{i}]";
                    var destination = $"TIID^EtherCAT Master^Box {id + 1} (AUTD)^RxPdo0^data[{HeadSize + i}]";
                    sysManager.LinkVariables(source, destination);
                }
                for (var i = 0; i < HeadSize; i++)
                {
                    var source = $"TIRT^Task 1^Outputs^gbody[{id}][{BodySize - HeadSize + i}]";
                    var destination = $"TIID^EtherCAT Master^Box {id + 1} (AUTD)^RxPdo1^data[{i}]";
                    sysManager.LinkVariables(source, destination);
                }
                {
                    var source = $"TIRT^Task 1^Inputs^input[{id}]";
                    var destination = $"TIID^EtherCAT Master^Box {id + 1} (AUTD)^TxPdo^dummy";
                    sysManager.LinkVariables(source, destination);
                }
                {
                    var stateSource = $"TIRT^Task 1^Inputs^state[{id}]";
                    var stateDestination = $"TIID^EtherCAT Master^Box {id + 1} (AUTD)^InfoData^State";
                    sysManager.LinkVariables(stateSource, stateDestination);
                }
            }
        }

        [Flags]
        public enum CpuAffinity : ulong
        {
            Cpu1 = 0x0000000000000001,
            Cpu2 = 0x0000000000000002,
            Cpu3 = 0x0000000000000004,
            Cpu4 = 0x0000000000000008,
            Cpu5 = 0x0000000000000010,
            Cpu6 = 0x0000000000000020,
            Cpu7 = 0x0000000000000040,
            Cpu8 = 0x0000000000000080,
            None = 0x0000000000000000,
            MaskSingle = Cpu1,
            MaskDual = Cpu1 | Cpu2,
            MaskQuad = MaskDual | Cpu3 | Cpu4,
            MaskHexa = MaskQuad | Cpu5 | Cpu6,
            MaskOct = MaskHexa | Cpu7 | Cpu8,
            MaskAll = 0xFFFFFFFFFFFFFFFF
        }

        public void AssignCpuCores(ITcSysManager sysManager)
        {
            var realtimeSettings = sysManager.LookupTreeItem("TIRS");
            var stringWriter = new StringWriter();
            using (var writer = XmlWriter.Create(stringWriter))
            {
                writer.WriteStartElement("TreeItem");
                writer.WriteStartElement("RTimeSetDef");
                writer.WriteElementString("MaxCPUs", "1");
                writer.WriteStartElement("CPUs");
                WriteCpuProperties(writer, 0);
                writer.WriteEndElement(); // CPUs     
                writer.WriteEndElement(); // RTimeSetDef     
                writer.WriteEndElement(); // TreeItem
            }
            var xml = stringWriter.ToString();
            realtimeSettings.ConsumeXml(xml);
        }

        private void WriteCpuProperties(XmlWriter writer, int id)
        {
            writer.WriteStartElement("CPU");
            writer.WriteAttributeString("id", id.ToString());
            writer.WriteElementString("BaseTime", _cpuBaseTime.ToString());
            writer.WriteEndElement();
        }
    }

    public class MessageFilter : IOleMessageFilter
    {
        public static void Register()
        {
            IOleMessageFilter newFilter = new MessageFilter();
            CoRegisterMessageFilter(newFilter, out _);
        }

        public static void Revoke()
        {
            CoRegisterMessageFilter(null, out _);
        }

        int IOleMessageFilter.HandleInComingCall(int dwCallType,
          IntPtr hTaskCaller, int dwTickCount, IntPtr
          lpInterfaceInfo)
        {
            return 0;
        }

        int IOleMessageFilter.RetryRejectedCall(IntPtr
          hTaskCallee, int dwTickCount, int dwRejectType)
        {
            return dwRejectType == 2 ? 99 : -1;
        }

        int IOleMessageFilter.MessagePending(IntPtr hTaskCallee,
          int dwTickCount, int dwPendingType)
        {
            return 2;
        }

        [DllImport("Ole32.dll")]
        private static extern int
          CoRegisterMessageFilter(IOleMessageFilter newFilter, out
          IOleMessageFilter oldFilter);
    }

    [ComImport, Guid("00000016-0000-0000-C000-000000000046"),
    InterfaceTypeAttribute(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IOleMessageFilter
    {
        [PreserveSig]
        int HandleInComingCall(
            int dwCallType,
            IntPtr hTaskCaller,
            int dwTickCount,
            IntPtr lpInterfaceInfo);

        [PreserveSig]
        int RetryRejectedCall(
            IntPtr hTaskCallee,
            int dwTickCount,
            int dwRejectType);

        [PreserveSig]
        int MessagePending(
            IntPtr hTaskCallee,
            int dwTickCount,
            int dwPendingType);
    }
}