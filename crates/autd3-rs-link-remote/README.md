# autd3-rs-link-remote

TCP transport `Link` for [`autd3-rs`](https://crates.io/crates/autd3-rs): relays tx/rx frames to a remote server that drives the real link.

Use it to keep the realtime bus on a dedicated machine (an EtherCAT appliance, for instance) while the application runs elsewhere.

## Feature flags

| Feature | Default | Description |
|---------|---------|-------------|
| `discovery` | no | Discover remote servers over mDNS |

## Documentation

* [日本語](https://shinolab.github.io/autd3-sdk/)
* [English](https://shinolab.github.io/autd3-sdk/en/)

## License

MIT. See [LICENSE](./LICENSE).
