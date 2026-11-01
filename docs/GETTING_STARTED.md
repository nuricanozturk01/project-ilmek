# Getting started

Run a node on one machine, join from others, publish and open an application.

> **No security in this branch.** Nothing is encrypted or authenticated; anyone on the network can
> join and publish. Use a network you trust ([SECURITY](SECURITY.md)).
>
> **What was tested.** Everything below ran on one Linux x86_64 machine with the node, hosts and
> clients as separate processes over `127.0.0.1`. **Not tested:** a second physical computer, real
> LAN broadcast discovery, macOS, Windows, Android ([TEST_REPORT](TEST_REPORT.md)). Steps for
> those platforms follow the tools' standard behavior and are marked as such.

## Requirements

- Two or more computers that can reach each other over IP (same Wi-Fi, switch or hotspot; no
  "AP isolation"). See [NETWORK_AND_DISCOVERY](NETWORK_AND_DISCOVERY.md).
- Rust (the toolchain in `rust-toolchain.toml` is installed by `rustup` automatically).
- Per-platform prerequisites and the desktop client: [INSTALL](INSTALL.md).

## 1. Start the node (Linux)

```sh
sudo ufw allow 47400/udp && sudo ufw allow 47401:47402/tcp   # only if a firewall is active
scripts/quickstart.sh
```

`scripts/quickstart.sh` builds the CLI and the forum backend, starts `ilmek node`, installs and
publishes the `nuri/forum` and `nuri/merhaba` examples, and prints how to reach them. The terminal
must stay open; Ctrl-C stops everything. Data goes to `./ilmek-demo` (log: `ilmek-demo/node.log`).
After updating the code, stop it and run the script again.

By hand, the same is:

```sh
cargo build -p ilmek-cli -p ilmek-example-forum
target/debug/ilmek node                       # second terminal from here on
target/debug/ilmek app pack examples/forum/app --out /tmp/forum-pkg
target/debug/ilmek app install /tmp/forum-pkg
target/debug/ilmek app set-backend nuri/forum --exe target/debug/ilmek-forum-backend
target/debug/ilmek app start nuri/forum
target/debug/ilmek app publish nuri/forum
```

Ports (defaults): discovery **47400/udp**, registry **47401/tcp**, host **47402/tcp**.

## 2. Join and use it from another machine

```sh
cargo build -p ilmek-cli
target/debug/ilmek join                       # finds the node by itself
target/debug/ilmek resolve ilmek://nuri/forum
target/debug/ilmek call ilmek://nuri/forum forum.topics.create --write \
    --body '{"title":"Hello","body":"it works"}'
target/debug/ilmek call ilmek://nuri/forum forum.topics.list
target/debug/ilmek fetch ilmek://nuri/merhaba # download and verify a package
```

Nothing has to be copied from the node. If discovery fails, pass the node's address only:
`ilmek join --registry NODE_IP:47401` (TCP only). Get the IP on Linux from the `src` field of
`ip -4 route get 1.1.1.1` (the first address of `hostname -I` may be a virtual bridge).

Success check: a topic created on the second machine appears in `ilmek call … forum.topics.list` on
the node.

## 3. Desktop client

The desktop client is a small browser for `ilmek://` addresses: address bar, catalog, one tab per
application. Build and install it per platform in [INSTALL](INSTALL.md). In development:

```sh
bun install
cd apps/client && bun run tauri dev
```

Press **Find and join** (or enter the node's address), type `ilmek://nuri/forum` in the address
bar and press **Open**. The installed client also registers itself as the handler of `ilmek://`
links, so a link clicked or typed in a browser opens the application in it. A link opened before the
device has joined only shows the join screen; join, then open the link again.

## 4. More hosts and clients

| Machine | Runs | Command |
| --- | --- | --- |
| A (`192.168.1.10`) | node: registry + host | `ilmek node` |
| B | extra host (serves its own applications) | `ilmek host start --registry 192.168.1.10:47401` |
| C, D, … | clients | `ilmek join --registry 192.168.1.10:47401` |

Start the node first. On one subnet with UDP 47400 open, `--registry` can be left out everywhere.
An extra host is a second machine running `ilmek host start`; it joins, registers itself and serves
its own applications, which you manage on that machine with `ilmek app …`
([APP_DEVELOPMENT_GUIDE](APP_DEVELOPMENT_GUIDE.md)). To test a second host on one computer use a
separate home and port: `ILMEK_HOME=~/ilmek-host2 ilmek host start --port 47412`.

Limits of this version:

- Every `ilmek node` creates its own community. To add a machine to an existing community use
  `ilmek host start` or `ilmek join`, not a second `ilmek node`.
- One registry per community; no failover.

## Troubleshooting

| Symptom | Cause / fix |
| --- | --- |
| `no node found` | Node not running, different subnet, or a firewall blocks `47401-47402/tcp` (and `47400/udp`). Open the ports or use `--registry NODE_IP:47401`. Test: `nc -vz NODE_IP 47401`. |
| `did not answer like an İlmek registry` | The node was started from an older build or another program owns the port. Rebuild and restart the node; node and clients must run the same build. |
| Node does not start | Read `$ILMEK_HOME/node.log` (quickstart: `ilmek-demo/node.log`). If a port is taken, use `--registry-port` / `--host-port`. |
| App does not open | `ilmek app list` must show `state=running` for it on the host. |
| `ilmek://` opens nothing | Open the client once by hand (Linux: the built `ilmek-client`; macOS/Windows: the installed app), then retry. |
| Client window shows old behavior | It was running during an update: quit and start it again. |

When reporting a problem include the command, its output, the node's log and both machines' IP
addresses.
