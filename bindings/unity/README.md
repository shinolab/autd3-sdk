# AUTD3 Unity binding

Per-crate UPM packages for using AUTD3 from Unity. The C# sources are **shared** with the
dotnet binding (`bindings/csharp/src`) and compiled by Unity itself; each package ships the
matching native cdylib. Minimum Unity version is **6 (6000.x)**.

## Layout

```
bindings/unity/
  com.shinolab.autd3-sdk.core/          # AUTD3.Core   (autd3_core)
  com.shinolab.autd3-sdk/               # AUTD3        (autd3capi), has Samples~/FocusSine
  com.shinolab.autd3-sdk.pattern/       # AUTD3.Pattern
  com.shinolab.autd3-sdk.pattern.holo/  # AUTD3.Pattern.Holo
  com.shinolab.autd3-sdk.modulation/    # AUTD3.Modulation
  com.shinolab.autd3-sdk.link.ethercrab/
  com.shinolab.autd3-sdk.link.nop/
  com.shinolab.autd3-sdk.link.remote/
  com.shinolab.autd3-sdk.link.twincat/
  com.shinolab.autd3-sdk.link.soem/     # GPL-3.0-only (opt-in; statically links SOEM)
```

Each package directory commits only `package.json`, `<Assembly>.asmdef`, `csc.rsp`, `README.md`
and (for the client) `Samples~/`. The C# sources, native cdylibs under `Plugins/`, the license
notices and all `.meta` files are staged by `cargo xtask unity build` / `unity pack` and are
**git-ignored** (do not edit them in place).

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

## Install from npmjs

The packages are published to the public npm registry. Add the scoped registry and the packages
you need to your Unity project's `Packages/manifest.json`; sibling AUTD3 packages are resolved
from the registry automatically.

```json
{
  "scopedRegistries": [
    { "name": "npmjs", "url": "https://registry.npmjs.org", "scopes": ["com.shinolab"] }
  ],
  "dependencies": {
    "com.shinolab.autd3-sdk": "0.1.0",
    "com.shinolab.autd3-sdk.link.ethercrab": "0.1.0"
  }
}
```

Packages served from `registry.npmjs.org` do **not** appear in the Package Manager's
*My Registries* tab. Unity's package listing calls the legacy `/-/all` route, which npmjs does
not implement. This affects listing only, not resolution: edit the manifest as above, or use
**+ → Add package by name**.

Versions are plain three-component SemVer and are kept in lockstep across all ten packages. The
`major.minor` pair matches the SDK release the packages were built from; the patch component
advances independently so that binding-only fixes can ship without an SDK release.

## Building the packages

```bash
cargo xtask unity build            # stage sources + host cdylib + deterministic .meta
cargo xtask unity build --soem     # also build/stage the GPL SOEM link
cargo xtask unity build --manifest # also print manifest.json file: entries
cargo xtask unity pack             # + license notices, then `npm pack` into dist/
```

`unity build` does **not** require the Unity Editor. It builds the FFI cdylibs for the host
platform and stages them into each package's `Plugins/<rid>/`.

`unity pack` additionally generates the license notices, runs `npm pack` per package and then
unpacks each tarball to check it really carries the sources, `.meta` files and cdylibs. Pass
`--native-dir <dir>` with `win-x64/`, `linux-x64/` and `osx-arm64/` subdirectories (as the
release workflow does) to ship all three RIDs instead of only the host's.

## Local install (`file:` references)

Add every package to your Unity project's `Packages/manifest.json` as a `file:` reference
(`cargo xtask unity build --manifest` prints these):

```json
{
  "dependencies": {
    "com.shinolab.autd3-sdk.core": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3-sdk.core",
    "com.shinolab.autd3-sdk": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3-sdk",
    "com.shinolab.autd3-sdk.pattern": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3-sdk.pattern",
    "com.shinolab.autd3-sdk.modulation": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3-sdk.modulation",
    "com.shinolab.autd3-sdk.link.nop": "file:/abs/path/autd3-sdk/bindings/unity/com.shinolab.autd3-sdk.link.nop"
  }
}
```

Direct `file:` references take priority over registry resolution, so sibling dependencies are
satisfied as long as every referenced package is listed. Run `cargo xtask unity build` before
opening the project so the sources, cdylibs and `.meta` exist.

The `Focus Sine` sample (client package) additionally needs `com.shinolab.autd3-sdk.link.nop`.

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
- `com.shinolab.autd3-sdk.link.soem` is GPL-3.0-only. Installing it makes the resulting build GPL.
