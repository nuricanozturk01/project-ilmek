# Network setup: NODE-A and NODE-B

> **Not yet applied to real hardware.** `ilmek network` is implemented and tested against a fake
> machine (a scratch root directory and a recording command runner) and by running `plan` on a
> development PC. `apply` has never run on a real node, no radio capability has been checked, and
> no Wi-Fi link has been brought up. Treat the first real `apply` as an experiment: do it with
> console access, as described below. Status per acceptance item is in
> [TEST_REPORT](TEST_REPORT.md).

## Target topology

```
 phones/PCs ~~Wi-Fi ILMEK-A~~ [wlan0 NODE-A wlan1] ~~Wi-Fi ILMEK-TRANSIT~~ [wlan1 NODE-B wlan0] ~~Wi-Fi ILMEK-B~~ phones/PCs
 10.77.10.0/24                10.77.10.1  10.77.254.1/30   10.77.254.2  10.77.20.1               10.77.20.0/24
```

The addresses are the V1 reference plan from REQUIREMENTS §9 and live only in configuration
(`configs/node-a.toml.example`, `configs/node-b.toml.example`); change them consistently if they
collide with an existing network. NODE-A's second radio is an access point for the transit link and
NODE-B's second radio is a station on it. There is no NAT and no default route: the İlmek networks
are only reachable from each other. The registry sits on NODE-A (`10.77.10.1:47401`, enrollment on
`:47402`); hosts are separate machines, for example `10.77.10.10` (HOST-1) and `10.77.20.10`
(HOST-2) with DHCP reservations.

## Requirements on each node

- Linux with `iproute2`, `nftables`, `systemd`, `dnsmasq`, `hostapd`; `wpa_supplicant` on a node
  whose transit radio is a station.
- **Two Wi-Fi adapters whose drivers support the roles in use.** Access-point mode is not provided
  by every adapter. Check before anything else: `iw list` must show `AP` under
  "Supported interface modes" for each radio used as an access point, and `iw dev` shows the
  interface names. `ilmek doctor` collects this (read-only); it needs `iw` installed.
- A regulatory domain (`country` in the node file). The tool will not generate an access point
  without one.
- Console or other out-of-band access to the node for the first apply.

## Procedure

1. **Inventory.** Run `ilmek doctor` on the node and record the output in
   [HARDWARE_INVENTORY](HARDWARE_INVENTORY.md). Do not continue if an adapter cannot be an
   access point.
2. **Configuration.** Copy the example to `node.toml`; set interface names, channels, `country`,
   DHCP range and MAC-based reservations. Create the passphrase files outside the configuration,
   readable by root only (8–63 printable ASCII characters on the first line):

   ```sh
   sudo install -d -m 700 /etc/ilmek/secrets
   sudo install -m 600 /dev/stdin /etc/ilmek/secrets/user.pass      # type the passphrase, Ctrl-D
   sudo install -m 600 /dev/stdin /etc/ilmek/secrets/transit.pass
   ```
3. **Review the plan** (changes nothing):

   ```sh
   ilmek network plan --config node.toml
   ```

   The plan lists every file it will write (path and mode), every command with its undo, what is
   missing (`MISSING: hostapd is not installed` blocks `apply`), and notes such as NetworkManager
   being present. Passphrases appear as `@PASSPHRASE@`; the real value is read from the file at
   apply time and never printed, never put on a command line.
4. **Apply** (root, with console access):

   ```sh
   sudo ilmek network apply --config node.toml --yes
   ```

   Without `--yes` the plan is printed and nothing is changed. The command refuses to touch the
   interface that carries the default route or your current SSH session unless you pass
   `--allow-management-interface`; do not pass that over a remote connection. It prints the
   snapshot ID when done.
5. **Check** (see the verification list below).
6. **Undo** if needed:

   ```sh
   sudo ilmek network status                     # lists snapshots and their phase
   sudo ilmek network rollback                   # newest snapshot not yet rolled back
   sudo ilmek network rollback --snapshot NODE-A-1760000000
   ```

## What `apply` changes

- Writes `/etc/ilmek/{hostapd-<if>.conf, wpa-<if>.conf, dnsmasq.conf, firewall.nft}` and
  `/etc/sysctl.d/90-ilmek.conf`; secrets files are mode 600.
- Writes systemd units `ilmek-hostapd-<if>`, `ilmek-wpa-<if>` (station), `ilmek-network`,
  `ilmek-firewall`, `ilmek-dnsmasq` so the setup survives a reboot, and enables them.
- Sets `net.ipv4.ip_forward=1` (the previous value is recorded and restored on rollback).
- `ilmek-network.service` assigns the addresses and the static route to the other user network
  (`ip route replace 10.77.20.0/24 via 10.77.254.2 dev wlan1` on NODE-A).
- `dnsmasq` serves **DHCP only** (`port=0`, no DNS), on the user interface, with the node as the
  router and the configured reservations. No fake "internet is available" answers are given.
- If NetworkManager is present, a drop-in marks the two radios unmanaged and NetworkManager is
  reloaded. **Anything NetworkManager was doing on those radios stops.**
- Adds the nftables table `inet ilmek` and nothing else: from outside the İlmek subnets the İlmek
  ports are dropped on input, and forwarding **through this node** is limited to İlmek TCP ports
  and basic ICMP between the İlmek subnets (policy drop in that table's forward chain). Other
  tables and rules are untouched. If the node must forward unrelated traffic, review this first.

## How rollback works

Before each change `apply` records it in a snapshot directory (default
`/var/lib/ilmek/network/<node>-<unix time>/`, mode 700): original contents of files that existed,
created directories, and the undo command of each command. The snapshot is written *before* the
change is made, so a crash half-way can still be rolled back. If any step fails, `apply` rolls back
automatically and reports whether that succeeded. `rollback` continues past individual failures
and lists them. It also saves `ip addr`, `ip route` and `nft list ruleset` output under `state/`
for comparison.

Manual fallback if the tool itself is unavailable (the files and units are those listed above):

```sh
sudo systemctl disable --now ilmek-dnsmasq ilmek-firewall ilmek-network ilmek-hostapd-wlan0 ilmek-hostapd-wlan1
sudo nft delete table inet ilmek
sudo rm /etc/systemd/system/ilmek-*.service /etc/sysctl.d/90-ilmek.conf /etc/ilmek/*.conf /etc/ilmek/firewall.nft
sudo rm -f /etc/NetworkManager/conf.d/90-ilmek-unmanaged.conf
sudo systemctl daemon-reload && sudo systemctl reload NetworkManager
sudo sysctl -w net.ipv4.ip_forward=<previous value, see state/ip_forward.txt in the snapshot>
```

## Verification checklist (to be run on the real nodes)

| Acceptance | Check | Command / observation |
| --- | --- | --- |
| A02 | both access networks hand out addresses | a client on ILMEK-A gets `10.77.10.x`, on ILMEK-B `10.77.20.x`, gateway = its node |
| A03 | the transit path is really Wi-Fi | `iw dev wlan1 link` on NODE-B shows the NODE-A SSID and signal; no cable between nodes |
| A04 | routing both ways | from HOST-1 `ping 10.77.20.10`, from HOST-2 `ping 10.77.10.10`; `ip route get` on each node |
| A05 | no internet needed | WAN unplugged, mobile data off; applications still work |
| A06 | no HTTP/WS fallback | packet capture on a node shows only TCP 47401/47402 carrying TLS, no HTTP |
| A15 | transit loss | switch NODE-B's radio off; locally running services keep working, cross-network calls fail with a clear error |

Connectivity at the İlmek layer: `ilmek diagnose ilmek://nuri/forum --device <device-dir>` checks
trust snapshot, name resolution, host reachability and package download step by step.

## Known limits

- Only the two-node plan is implemented. A third node, alternative routes and dynamic routing are
  later work (REQUIREMENTS §9).
- The tool does not decide which channel is quiet or whether two radios on one machine can run
  concurrently; those are hardware facts a person has to confirm.
- `apply` supports Linux with systemd only.
