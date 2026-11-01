# Install and build, by platform

Two separate things:

| | Purpose | Runs on |
| --- | --- | --- |
| **Node** (`ilmek node`) | publishes applications | one machine, **Linux recommended** |
| **Client** (`ilmek` command and the **İlmek** window) | opens applications | any machine |

No security in this branch; use a trusted network. Status columns are honest: **Tested** = run on
the Linux development machine; **Not tested** = steps follow the standard tool documentation and
were not run.

| Platform | Node | `ilmek` command | İlmek window | `ilmek://` from a browser |
| --- | --- | --- | --- | --- |
| Linux | Tested | Tested | Tested (debug build) | Tested with `xdg-open`; Chrome not used |
| macOS | Not tested | Not tested | Not tested | Not tested |
| Windows | Not tested | Not tested | Not tested | Not tested |

Everywhere: get the code (`git clone <repository> && cd project-ilmek`), and run the **same build**
on the node and on clients (an older node answers a newer client with "did not answer like an
İlmek registry").

## Linux

### Prerequisites (Debian / Ubuntu / Mint)

```sh
sudo apt install build-essential curl file libssl-dev libxdo-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev   # window only (Tauri)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # Rust
curl -fsSL https://bun.sh/install | bash                             # Bun, window only (no Node needed)
```

For the node and the `ilmek` command only Rust and a C compiler are needed.

### Node and command

See [GETTING_STARTED](GETTING_STARTED.md): `scripts/quickstart.sh`, then `cargo build -p ilmek-cli`
and `target/debug/ilmek join` on clients.

### Window

```sh
bun install
cd apps/client && bun run tauri build --debug --no-bundle && cd ../..
target/debug/ilmek-client                      # opens the window
target/debug/ilmek-client ilmek://nuri/forum   # or open an address directly
```

Press **Find and join**, enter `ilmek://nuri/forum`, press **Open**. Do not build the window with a
plain `cargo build -p ilmek-client-app`: Tauri embeds the UI only through its own `tauri build`.
`ILMEK_DATA_DIR=/path` selects another data directory (default `~/.local/share/net.ilmek.client`).

### `ilmek://` links

On every start the client writes a per-user handler
(`~/.local/share/applications/ilmek-client-handler.desktop`, pointing at the program's current
path). Start it once, then `xdg-open "ilmek://nuri/forum"` opens the application (tested). Typing
the address in Chrome/Firefox uses the same handler (not tested). If you move the program, start it
once more. A second start with a link hands it to the running instance, which opens a new tab.

### `.deb` package

```sh
cd apps/client && bun run tauri build --bundles deb && cd ../..
sudo dpkg -i target/release/bundle/deb/Ilmek_0.1.0_amd64.deb
sudo apt -f install        # if dependencies are missing
```

The package was built and accepted by `dpkg --dry-run -i`; it was not installed. It registers
`ilmek://` itself (`MimeType=x-scheme-handler/ilmek`). The product name must stay ASCII (`Ilmek`):
Debian rejects package names derived from `İlmek`.

## macOS (not tested)

```sh
xcode-select --install                                           # C compiler
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
brew install oven-sh/bun/bun
cargo build -p ilmek-cli
target/debug/ilmek join
```

macOS registers `ilmek://` only for an **installed `.app`**, not for `tauri dev`:

```sh
bun install
cd apps/client && bun run tauri build --bundles app && cd ../..
cp -R "target/release/bundle/macos/Ilmek.app" /Applications/
open "/Applications/Ilmek.app"            # unsigned: first time right-click > Open
open "ilmek://nuri/forum"                 # verify from a terminal first
```

Press **Find and join** in the window before opening links. If Chrome treats the typed address as a
search, click a link instead
(`echo '<a href="ilmek://nuri/forum">Forum</a>' > /tmp/ilmek.html && open -a "Google Chrome" /tmp/ilmek.html`).
If the terminal works but Chrome does not ask, quit Chrome completely (Cmd+Q) and check that
`ilmek` is not blocked in `chrome://settings/handlers`. After updating the code, rebuild and replace
the `.app` and rebuild the CLI.

## Windows (not tested)

Prerequisites: Visual Studio Build Tools 2022 ("Desktop development with C++"), Rust (MSVC,
<https://rustup.rs>), WebView2 (preinstalled on Windows 11), Bun
(`powershell -c "irm bun.sh/install.ps1 | iex"`).

```powershell
cargo build -p ilmek-cli
target\debug\ilmek.exe join
bun install
cd apps\client
bun run tauri build --bundles nsis
```

The installer is created under `target\release\bundle\nsis\` (`…-setup.exe`). The program registers
`ilmek://` for the user. Windows Firewall may ask for network access: allow **private networks**.
`ilmek node` on Windows has not been tried (backend process and path assumptions are Linux/macOS);
run the node on Linux and use Windows as a client.
