## macOS

The macOS binaries are not signed or notarized with an Apple Developer ID, so Gatekeeper blocks them
with "cannot be opened because the developer cannot be verified".
Remove the quarantine attribute after installing:

```console
xattr -d com.apple.quarantine ~/.autd3/bin/autd3-console
```

If you are running an extracted archive, apply it to the extracted directory instead:

```console
xattr -dr com.apple.quarantine <extracted directory>
```
