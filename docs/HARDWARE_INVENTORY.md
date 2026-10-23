# Hardware inventory

Acceptance item **A01** asks for verified radio roles on the nodes and verified end-device support.
**It is `BLOCKED`:** the two routing nodes, the Android phones and the other computers have not
been provided or inspected. Only the development machine below has been looked at, with read-only
commands (`ilmek doctor` and `lspci`, `lsusb`, `/sys`). Nothing has been inferred about hardware
that has not been seen.

## Development machine (not a node, not an end device in the test plan)

Inspected 2026-10-23; revision `42167b8` plus uncommitted changes; user `nuricanozturk`, not root.

| Item | Observed |
| --- | --- |
| OS / kernel | Linux Mint 22.3 (Ubuntu 24.04 base), Linux 7.0.0-38-generic, x86_64 |
| CPU / RAM | 16 logical CPUs (AMD), 32 GB (`/proc/meminfo` MemTotal 32 532 992 kB) |
| Ethernet | Realtek RTL8125 2.5GbE `eno1`, driver `r8169`, 192.168.0.104/24, default route |
| Wi-Fi | MEDIATEK MT7922 (802.11ax) PCIe `wlp11s0`, driver `mt7921e`, 192.168.0.103/24, **also carries a default route**, `phy0` not rfkill-blocked |
| Other radios | Bluetooth `hci0` soft-blocked (outside V1); USB `13d3:3614 IMC Networks Wireless_Device` (purpose not determined) |
| Virtual | `docker0`, `virbr0` (both down) |
| Tools present | `ip`, `nft`, `dnsmasq`, `wpa_supplicant`, `systemctl`, `nmcli` |
| Tools missing | **`iw`** (so the interface modes/combinations of `phy0` could not be read), **`hostapd`** |
| Toolchains | rustc/cargo 1.96.0, Node v24.17.0, pnpm 12.14.0 |
| Display | X11 `:0` (used for the WebKitGTK isolation probe) |

What this does and does not tell us:

- The MT7922 *might* support AP mode and concurrent interfaces; that has **not** been checked
  (needs `iw list`). It is irrelevant to the plan anyway: this machine is the development PC, and
  its Wi-Fi carries a default route, so `ilmek network apply` refuses it by default and must not be
  run here.
- A loopback test on this machine is not a Wi-Fi test.

## Required devices (REQUIREMENTS §6, §8)

| Role | Device | Information needed | Status |
| --- | --- | --- | --- |
| NODE-A | old PC, Linux | model, CPU/RAM, two Wi-Fi adapters: chipset, driver, `iw list` modes (AP, managed) and "valid interface combinations", regulatory domain, whether Linux is installed or can be installed without losing data | **not provided** |
| NODE-B | old PC, Linux | same | **not provided** |
| PHONE-1, PHONE-2 | Android | model, Android version, System WebView version, whether USB debugging is available | **not provided** |
| HOST-1 | computer | OS and version, CPU architecture, IP reservation, MAC address | **not provided** |
| HOST-2 | computer | same | **not provided** |
| Third computer | Windows/macOS/Linux | OS and version, Wi-Fi adapter | **not provided** |
| Radio environment | — | distances, walls, test location, observed signal | **unknown** |

## How to complete A01

On each Linux node, as root or a normal user (read-only):

```sh
sudo apt install iw            # or the distribution's equivalent
ilmek doctor > doctor-NODE-A.txt
iw list > iw-list-NODE-A.txt   # confirm "AP" under "Supported interface modes" for the radios used as access points
```

Attach the outputs here with the date and the `ilmek` revision. A01 passes only when each node has
two radios that support the roles in [NETWORK_SETUP](NETWORK_SETUP.md) and the end devices are
identified. Until then the physical-network items (A02–A05, A07–A10 on real devices, A14, A15, A20)
cannot be run.
