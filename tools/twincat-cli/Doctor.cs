using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;

namespace TwincatCli
{
    internal static class Doctor
    {
        private const string Script =
            "try { $vbs = (Get-CimInstance -Namespace root\\Microsoft\\Windows\\DeviceGuard -ClassName Win32_DeviceGuard -ErrorAction Stop).VirtualizationBasedSecurityStatus } catch { $vbs = '' }; " +
            "'VBS=' + $vbs; " +
            "try { $mi = (Get-ItemProperty 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\DeviceGuard\\Scenarios\\HypervisorEnforcedCodeIntegrity' -Name Enabled -ErrorAction Stop).Enabled } catch { $mi = '' }; " +
            "'MemIntegrity=' + $mi; " +
            "try { $hv = (Get-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-Hypervisor -ErrorAction Stop).State } catch { $hv = '' }; " +
            "'HyperV=' + $hv; " +
            "try { $vmp = (Get-WindowsOptionalFeature -Online -FeatureName VirtualMachinePlatform -ErrorAction Stop).State } catch { $vmp = '' }; " +
            "'VMP=' + $vmp";

        private enum Status
        {
            Off,
            On,
            Unknown,
        }

        public static int Run()
        {
            var values = new Dictionary<string, string>();
            try
            {
                var psi = new ProcessStartInfo
                {
                    FileName = FindPowerShell(),
                    Arguments = "-NoProfile -NonInteractive -Command \"" + Script + "\"",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    CreateNoWindow = true,
                };
                using (var proc = Process.Start(psi))
                {
                    var stdout = proc.StandardOutput.ReadToEnd();
                    proc.WaitForExit();
                    foreach (var line in stdout.Split('\n'))
                    {
                        var idx = line.IndexOf('=');
                        if (idx >= 0)
                        {
                            values[line.Substring(0, idx).Trim()] = line.Substring(idx + 1).Trim();
                        }
                    }
                }
            }
            catch (Exception e)
            {
                Console.Error.WriteLine($"failed to run PowerShell diagnosis: {e.Message}");
                return 1;
            }

            string Get(string key) => values.TryGetValue(key, out var v) ? v : "";

            Console.WriteLine("Virtualization-based security diagnosis (all should be OFF for TwinCAT real-time):\n");

            var anyFlagged = false;

            anyFlagged |= Report(
                "Virtualization-based security (VBS)",
                Get("VBS") == "0" ? Status.Off : (Get("VBS") == "1" || Get("VBS") == "2" ? Status.On : Status.Unknown),
                new[]
                {
                    @"Set HKLM\SYSTEM\CurrentControlSet\Control\DeviceGuard\EnableVirtualizationBasedSecurity = 0",
                    @"Set HKLM\SYSTEM\CurrentControlSet\Control\DeviceGuard\HyperVVirtualizationBasedSecurityOptout = 1",
                    "Then reboot. VBS is sticky; see the readiness tool below if it survives a reboot.",
                });

            anyFlagged |= Report(
                "Core isolation / memory integrity (HVCI)",
                Get("MemIntegrity") == "" || Get("MemIntegrity") == "0" ? Status.Off : Status.On,
                new[]
                {
                    "Settings > Privacy & security > Windows Security > Device security > Core isolation > turn Memory integrity off",
                    @"Or set HKLM\SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity\Enabled = 0, then reboot",
                });

            anyFlagged |= Report(
                "Hyper-V hypervisor",
                FeatureStatus(Get("HyperV")),
                new[]
                {
                    "Run as admin: Disable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-Hypervisor",
                    "Or: bcdedit /set hypervisorlaunchtype off",
                    "Then reboot.",
                });

            anyFlagged |= Report(
                "Virtual Machine Platform",
                FeatureStatus(Get("VMP")),
                new[]
                {
                    "Run as admin: Disable-WindowsOptionalFeature -Online -FeatureName VirtualMachinePlatform",
                    "Then reboot.",
                });

            if (anyFlagged)
            {
                Console.WriteLine();
                Console.WriteLine("If VBS/Credential Guard stays on after the above (it can be enforced by UEFI lock or policy), use Microsoft's \"Device Guard and Credential Guard hardware readiness tool\":");
                Console.WriteLine("    https://www.microsoft.com/en-us/download/details.aspx?id=53337");
                Console.WriteLine("    Run as admin: .\\DG_Readiness_Tool_v3.6.ps1 -Disable    (then reboot; accept the UEFI prompt on next boot)");
            }

            return 0;
        }

        private static Status FeatureStatus(string state)
        {
            switch (state)
            {
                case "Disabled":
                case "DisabledWithPayloadRemoved":
                    return Status.Off;
                case "":
                    return Status.Unknown;
                default:
                    return Status.On;
            }
        }

        private static bool Report(string label, Status status, string[] remedy)
        {
            switch (status)
            {
                case Status.Off:
                    Console.WriteLine($"  OK       {label}: off");
                    return false;
                case Status.On:
                    Console.WriteLine($"  WARNING  {label}: enabled (disable it)");
                    foreach (var step in remedy)
                    {
                        Console.WriteLine($"             - {step}");
                    }
                    return true;
                default:
                    Console.WriteLine($"  ?        {label}: unknown (please run as admin)");
                    return true;
            }
        }

        private static string FindPowerShell()
        {
            var root = Environment.GetEnvironmentVariable("SystemRoot");
            if (!string.IsNullOrEmpty(root))
            {
                var abs = Path.Combine(root, @"System32\WindowsPowerShell\v1.0\powershell.exe");
                if (File.Exists(abs))
                {
                    return abs;
                }
            }
            return "powershell";
        }
    }
}
