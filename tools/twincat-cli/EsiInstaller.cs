using System;
using System.Collections.Generic;
using System.IO;
using System.Reflection;

namespace TwincatCli
{
    internal static class EsiInstaller
    {
        public static void Install(bool announceNoop)
        {
            var roots = new List<string>();
            var env = Environment.GetEnvironmentVariable("TWINCAT3DIR");
            if (!string.IsNullOrWhiteSpace(env))
            {
                roots.Add(env);
            }
            roots.Add(@"C:\TwinCAT\3.1");
            roots.Add(@"C:\Program Files (x86)\Beckhoff\TwinCAT\3.1");

            var dsts = new List<string>();
            foreach (var root in roots)
            {
                var dst = Path.Combine(root, @"Config\Io\EtherCAT\AUTD.xml");
                var parent = Path.GetDirectoryName(dst);
                if (!File.Exists(dst) && parent != null && Directory.Exists(parent) && !dsts.Contains(dst))
                {
                    dsts.Add(dst);
                }
            }

            if (dsts.Count == 0)
            {
                if (announceNoop)
                {
                    Console.WriteLine("AUTD.xml already installed (or no TwinCAT EtherCAT config dir found)");
                }
                return;
            }

            var content = LoadEmbedded();
            var failed = false;
            foreach (var dst in dsts)
            {
                try
                {
                    File.WriteAllBytes(dst, content);
                    Console.WriteLine($"installed AUTD.xml -> {dst}");
                }
                catch (Exception e)
                {
                    failed = true;
                    Console.Error.WriteLine($"failed to copy AUTD.xml to {dst}: {e.Message}");
                }
            }

            if (failed)
            {
                Console.Error.WriteLine("\nre-run as Administrator, or manually copy AUTD.xml into:");
                foreach (var dst in dsts)
                {
                    var parent = Path.GetDirectoryName(dst);
                    if (parent != null)
                    {
                        Console.Error.WriteLine($"    {parent}");
                    }
                }
                throw new Exception("could not install AUTD.xml automatically (see manual steps above)");
            }
        }

        private static byte[] LoadEmbedded()
        {
            var asm = Assembly.GetExecutingAssembly();
            using (var stream = asm.GetManifestResourceStream("AUTD.xml"))
            {
                if (stream == null)
                {
                    throw new Exception("embedded AUTD.xml resource not found");
                }
                using (var ms = new MemoryStream())
                {
                    stream.CopyTo(ms);
                    return ms.ToArray();
                }
            }
        }
    }
}
