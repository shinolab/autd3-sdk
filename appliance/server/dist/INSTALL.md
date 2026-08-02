# AUTD3 remote master appliance

## Install

```sh
sudo useradd --system --home-dir /var/lib/autd3 --shell /usr/sbin/nologin autd3
sudo install -m 0755 autd3-remote-server /usr/local/bin/
sudo install -d /usr/local/libexec/autd3 /etc/autd3 /data
sudo install -m 0755 tune-appliance.sh run-server autd3-wifi-init /usr/local/libexec/autd3/
sudo install -m 0644 remote-server.toml /etc/autd3/
sudo install -m 0644 autd3-remote-server.service autd3-wifi-init.service /etc/systemd/system/
sudo install -m 0755 -o root -g root autd3-admin /usr/local/sbin/
sudo install -m 0440 -o root -g root sudoers-autd3-admin /etc/sudoers.d/autd3-admin
sudo systemctl daemon-reload
sudo systemctl enable --now autd3-remote-server
sudo systemctl enable autd3-wifi-init      # only if the board is to use Wi-Fi
```

`autd3-wifi-init` unblocks the radio at boot and re-applies the regulatory domain that
`PUT /network/wifi` stored in `/data/regdomain`. Without it the domain reverts to the world
domain on every boot, which costs the 5GHz band and channels 12/13.

`/etc/autd3` must be writable by the `autd3` user for `PUT /config` to work:

```sh
sudo chown -R autd3:autd3 /etc/autd3
```

The unit runs under `ProtectSystem=strict`, so everything outside its `ReadWritePaths` is
read-only for the server *and* for the `autd3-admin` helper it calls through `sudo`. The
shipped list covers what the privileged endpoints write:

| Path                                     | Written by                                            |
| ---------------------------------------- | ----------------------------------------------------- |
| `/etc/autd3`                             | `PUT /config`                                          |
| `/usr/local/bin`                         | `POST /update` (the helper installs the new binary)    |
| `/etc/NetworkManager/system-connections` | `PUT /network/wifi` (the keyfile)                      |
| `/data`                                  | `PUT /network/wifi` (the regulatory domain)            |

`/data` and the NetworkManager directory are optional entries, so the unit still starts when
they are absent; the endpoint that needs the missing one fails instead. Keep the list in sync
if the layout is changed: dropping a path makes the endpoint fail with a read-only file system.

## Before it works

1. Edit `/etc/autd3/remote-server.toml`: set `bus.interface` to the port wired to the AUTD3 devices, and `rt.affinity` to the isolated core.
2. Keep the EtherCAT port free of IP configuration. On NetworkManager: `nmcli device set eth0 managed no`.
3. Append the kernel parameters from `cmdline.txt.example` and reboot.
4. `autd3-remote-server --interface eth0 --probe` reports how many devices are on the bus.

## Finding the appliance

The server advertises `_autd3._tcp.local` on every interface except the EtherCAT port, so a
client finds it without being given an address. The instance name defaults to
`autd3-<board serial>`; the log line at startup shows it.

```sh
journalctl -u autd3-remote-server | grep 'advertising the appliance'
```

## Control API

`http://<instance>.local:8081/` serves a status page. The same port answers JSON:

| Method    | Path            | What it does                                             |
| --------- | --------------- | -------------------------------------------------------- |
| GET       | `/status`       | bus state, device AL states, timings, connected client   |
| GET / PUT | `/config`       | the TOML config; a `PUT` is validated, then applied on the next restart |
| POST      | `/bus/open`     | ask the bus to open                                       |
| POST      | `/bus/close`    | ask the bus to close                                      |
| POST      | `/bus/probe`    | count the devices without leaving the bus open           |
| POST      | `/restart`      | restart the server process                                |
| POST      | `/reboot`       | reboot the appliance                                      |
| POST      | `/shutdown`     | power the appliance off                                   |
| POST      | `/update`       | replace the server binary with the request body           |
| PUT       | `/network/wifi` | store Wi-Fi credentials                                   |
| GET       | `/logs`         | tail of the journal (`?lines=N`)                          |

```sh
curl -s http://autd3-0a1b2c3d.local:8081/status | jq .bus.actual
curl -s -X POST http://autd3-0a1b2c3d.local:8081/bus/probe
curl -s -X POST --data-binary @autd3-remote-server \
    http://autd3-0a1b2c3d.local:8081/update
```

**The control API has no authentication.**
Anyone who can reach port 8081 can reconfigure the appliance and replace its binary.
Put it on a network you trust, or turn the privileged half off:

```sh
sudo rm /etc/sudoers.d/autd3-admin      # reboot / shutdown / update / wifi stop working
# and set control.allow_admin = false in /etc/autd3/remote-server.toml
```

To also restore the tighter sandbox the unit ships commented out:

```sh
sudo systemctl edit autd3-remote-server
# [Service]
# NoNewPrivileges=yes
# CapabilityBoundingSet=CAP_NET_RAW CAP_SYS_NICE CAP_IPC_LOCK
```

Set `control.enabled = false` to drop the HTTP listener entirely.

## Checks

```sh
systemctl status autd3-remote-server
journalctl -u autd3-remote-server -f
```

The server logs a bus health summary every minute: device states, recoveries, stale and lost cycles, and the worst SYNC0 phase deviation.
