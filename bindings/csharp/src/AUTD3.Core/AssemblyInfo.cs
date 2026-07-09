using System.Runtime.CompilerServices;

// Declared in source (not as csproj <InternalsVisibleTo> items) so that Unity, which
// compiles these sources via an .asmdef and never sees the csproj, grants the same access.
[assembly: InternalsVisibleTo("AUTD3")]
[assembly: InternalsVisibleTo("AUTD3.Pattern")]
[assembly: InternalsVisibleTo("AUTD3.Pattern.Holo")]
[assembly: InternalsVisibleTo("AUTD3.Modulation")]
[assembly: InternalsVisibleTo("AUTD3.Link.Ethercrab")]
[assembly: InternalsVisibleTo("AUTD3.Link.Soem")]
[assembly: InternalsVisibleTo("AUTD3.Link.Remote")]
[assembly: InternalsVisibleTo("AUTD3.Link.TwinCAT")]
[assembly: InternalsVisibleTo("AUTD3.Link.Nop")]
[assembly: InternalsVisibleTo("AUTD3.Tests")]
