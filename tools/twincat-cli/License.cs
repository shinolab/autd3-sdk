using System;
using System.IO;
using System.Reflection;

namespace TwincatCli
{
    internal static class License
    {
        public static void Print()
        {
            Console.WriteLine("twincat-cli - MIT License");
            Console.WriteLine();
            Console.WriteLine(ReadResource("LICENSE", "(LICENSE resource not embedded)"));
            Console.WriteLine();
            Console.WriteLine("================ Third-party (NuGet) licenses ================");
            Console.WriteLine();
            Console.WriteLine(ReadResource(
                "THIRD-PARTY-LICENSES.md",
                "Third-party (NuGet) license information is generated at build time by " +
                "`cargo xtask tool twincat ...` (dotnet-project-licenses). It was not " +
                "embedded in this build.\n\n" +
                "Note: TwinCAT / Beckhoff.TwinCAT.Ads packages are licensed by Beckhoff " +
                "Automation under their own terms; see https://www.beckhoff.com/."));
        }

        private static string ReadResource(string name, string fallback)
        {
            var asm = Assembly.GetExecutingAssembly();
            using (var stream = asm.GetManifestResourceStream(name))
            {
                if (stream == null)
                {
                    return fallback;
                }
                using (var reader = new StreamReader(stream))
                {
                    return reader.ReadToEnd();
                }
            }
        }
    }
}
