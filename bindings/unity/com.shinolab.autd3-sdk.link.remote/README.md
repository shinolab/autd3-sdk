# AUTD3 Link Remote (Unity)

Link that forwards frames to a remote server or the simulator.

## Install

Add the scoped registry and the package to `Packages/manifest.json`; sibling
AUTD3 packages are resolved from the registry automatically.

```json
{
  "scopedRegistries": [
    { "name": "npmjs", "url": "https://registry.npmjs.org", "scopes": ["com.shinolab"] }
  ],
  "dependencies": {
    "com.shinolab.autd3-sdk.link.remote": "0.1.0"
  }
}
```

Packages served from `registry.npmjs.org` do **not** appear in the Package Manager's
*My Registries* tab, because npmjs does not implement the legacy `/-/all` route that
Unity's package listing relies on. Resolution is unaffected: edit the manifest as above,
or use **+ → Add package by name**.

## Platforms

Editor and Standalone builds on Windows (`win-x64`), Linux (`linux-x64`) and Apple
Silicon macOS (`osx-arm64`). Intel macOS is not shipped.

## Notes

Minimum Unity version is 6 (6000.x). The API speaks Unity's left-handed, metre frame;
conversion to the AUTD3 canonical frame happens at the binding boundary.

See the [Unity binding README](https://github.com/shinolab/autd3-sdk/blob/main/bindings/unity/README.md) for the
coordinate system, the full package list and local development with `file:` references.

## License

MIT. See `LICENSE.md`. Third-party notices are in `THIRD-PARTY-LICENSES.md`.
