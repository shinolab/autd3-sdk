# AUTD3 Unity binding

Per-crate UPM packages for using AUTD3 from Unity.
The C# sources are shared with the dotnet binding (`bindings/csharp`).

Minimum Unity version is **6 (6000.x)**.

## Install from npmjs

The packages are published to the public npm registry.

To install from the GUI, add the following registry under Edit → Project Settings → Package Manager → Scoped Registries.

- Name: npmjs
- URL: https://registry.npmjs.org
- Scope(s): com.shinolab

Then add the required packages from Window → Package Manager → My registries → npmjs.

## Building the packages

```bash
cargo xtask unity build
```

## Local install (`file:` references)

Add every package to your Unity project's `Packages/manifest.json` as a `file:` reference (`cargo xtask unity build --manifest` prints these):

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

## Coordinate system

Unity is left-handed with metres and +z forward; AUTD3's canonical frame is right-handed with millimetres.
Conversion happens at the binding boundary, so **the API speaks the Unity frame**.
The same focus the dotnet sample writes as `new Vector3(0, 0, 150)` is `new Vector3(0, 0, -0.15f)` in Unity.
Positions scale by 1000 and flip z, directions flip z and are normalised, and rotations are mirrored `(w, x, y, z) -> (w, -x, -y, z)`.

## Platform notes

- Targets are Editor + Standalone (Windows / macOS / Linux). Mobile / IL2CPP device builds are not validated yet.
- On Linux, EtherCAT raw sockets need `CAP_NET_RAW`.
