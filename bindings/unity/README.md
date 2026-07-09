# AUTD3 Unity binding

Per-crate UPM packages for using AUTD3 from Unity. The C# sources are **shared** with the
dotnet binding (`bindings/csharp/src`) and compiled by Unity itself; each package ships the
matching native cdylib. Minimum Unity version is **6 (6000.x)**.

## Layout

```
bindings/unity/
  com.shinolab.autd3.core/          # AUTD3.Core   (autd3_core)
  com.shinolab.autd3/               # AUTD3        (autd3capi), has Samples~/FocusSine
  com.shinolab.autd3.pattern/       # AUTD3.Pattern
  com.shinolab.autd3.pattern.holo/  # AUTD3.Pattern.Holo
  com.shinolab.autd3.modulation/    # AUTD3.Modulation
  com.shinolab.autd3.link.ethercrab/
  com.shinolab.autd3.link.nop/
  com.shinolab.autd3.link.remote/
  com.shinolab.autd3.link.twincat/
  com.shinolab.autd3.link.soem/     # GPL-3.0-only (opt-in; statically links SOEM)
```

Each package directory commits only `package.json`, `<Assembly>.asmdef`, `csc.rsp` and (for the
client) `Samples~/`. The C# sources, native cdylibs under `Plugins/` and all `.meta` files are
staged by `cargo xtask unity build` and are **git-ignored** (do not edit them in place).

## Why each package ships a `csc.rsp`

Unity pins the C# language version to **9.0**, but the shared sources use C# 10 features
(`global using` directives for the `Vector3`/`Quaternion` aliases, and parameterless struct
constructors that give `new FocusOption()` real defaults instead of zeros). Each package
therefore ships a per-assembly response file:

```
-langversion:10
-nullable:enable
```

Unity passes this to Roslyn for that assembly. Both features are compiler-only (no runtime
support needed), so this works on Mono and IL2CPP alike. `-nullable:enable` matches the dotnet
build (`bindings/csharp/Directory.Build.props`) and silences the CS8632 warnings Unity would
otherwise emit for the `string?` annotations.

Note: Unity's `.rsp` parser has no comment syntax — every whitespace-separated token becomes a
compiler argument, so the file must contain flags only.

`InternalsVisibleTo` is likewise declared in `AssemblyInfo.cs` **in source** rather than as
csproj `<InternalsVisibleTo>` items, because Unity compiles via the `.asmdef` and never reads
the csproj.

## Building the packages

```bash
cargo xtask unity build            # stage sources + host cdylib + deterministic .meta
cargo xtask unity build --soem     # also build/stage the GPL SOEM link
cargo xtask unity build --manifest # also print manifest.json file: entries
```

`unity build` does **not** require the Unity Editor. It builds the FFI cdylibs for the host
platform and stages them into each package's `Plugins/<rid>/`.

## Local install (before OpenUPM publish)

Add every package to your Unity project's `Packages/manifest.json` as a `file:` reference
(`cargo xtask unity build --manifest` prints these):

```json
{
  "dependencies": {
    "com.shinolab.autd3.core": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3.core",
    "com.shinolab.autd3": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3",
    "com.shinolab.autd3.pattern": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3.pattern",
    "com.shinolab.autd3.modulation": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3.modulation",
    "com.shinolab.autd3.link.nop": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3.link.nop"
  }
}
```

Direct `file:` references take priority over registry resolution, so sibling dependencies are
satisfied as long as every referenced package is listed. Run `cargo xtask unity build` before
opening the project so the sources, cdylibs and `.meta` exist.

The `Focus Sine` sample (client package) additionally needs `com.shinolab.autd3.link.nop`.

## Coordinate system

Unity is left-handed with metres and +z forward; AUTD3's canonical frame is right-handed with
millimetres. Conversion happens at the binding boundary, so **the API speaks the Unity frame**.
The same focus the dotnet sample writes as `new Vector3(0, 0, 150)` is `new Vector3(0, 0, 0.15f)`
in Unity. Positions scale by 1000 and flip z, directions flip z and are normalised, and rotations
are mirrored `(w, x, y, z) -> (w, -x, -y, z)`.

Because `Vector3`/`Quaternion` are the *environment's* types, their static members follow that
environment's spelling: `Vector3.zero` / `Quaternion.identity` in Unity versus `Vector3.Zero` /
`Quaternion.Identity` in dotnet.

Also note that the `AUTD3` namespace contains a `Nop` **command**, so the Nop *link* must be
written fully qualified as `new AUTD3.Link.Nop()`.

## Platform notes

- Targets are Editor + Standalone (Windows / macOS / Linux). Mobile / IL2CPP device builds are
  not validated yet.
- On Linux, EtherCAT raw sockets need `CAP_NET_RAW`; setting that on the Editor is impractical,
  so real-hardware use is primarily Windows (TwinCAT / ethercrab + Npcap).
- `com.shinolab.autd3.link.soem` is GPL-3.0-only. Installing it makes the resulting build GPL.
