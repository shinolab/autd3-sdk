using System.Runtime.CompilerServices;

// Declared in source (not as csproj <InternalsVisibleTo> items) so that Unity, which
// compiles these sources via an .asmdef and never sees the csproj, grants the same access.
[assembly: InternalsVisibleTo("AUTD3")]
[assembly: InternalsVisibleTo("AUTD3.Pattern.Holo")]
