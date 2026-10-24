# Platformlara göre kurulum ve çalıştırma

İki ayrı şey var; ikisini karıştırma:

| | Ne işe yarar | Nerede çalışır |
| --- | --- | --- |
| **Node** (`ilmek node`) | Uygulamaları yayınlar | Bir makinede, **Linux önerilir** |
| **İstemci** (`ilmek` komutu ve **İlmek** penceresi) | Uygulamaları açar | Her makinede |

Dalda **güvenlik yok**; yalnızca güvendiğin ağda kullan. Aşağıdaki durum sütunları dürüsttür:
**Denendi** = bu çalışmada bu makinede (Linux) çalıştırıldı. **Denenmedi** = adımlar standart araç
belgelerine göre yazıldı, çalıştırılmadı.

| Platform | Node | `ilmek` komutu | İlmek penceresi | Tarayıcıdan `ilmek://` |
| --- | --- | --- | --- | --- |
| Linux | Denendi | Denendi | Denendi (hata ayıklama derlemesi) | `xdg-open` ile denendi; Chrome kullanılmadı |
| macOS | Denenmedi | Denenmedi | Denenmedi | Denenmedi |
| Windows | Denenmedi | Denenmedi | Denenmedi | Denenmedi |

Her yerde ortak: kodu al (`git clone -b feat/simple-no-security …` veya `git pull`), ve **node ile
istemci aynı sürümden olsun** (eski node yeni istemciye "registry gibi cevap vermedi" der).

---

## Linux

### Gerekenler (Debian / Ubuntu / Mint)

```sh
sudo apt install build-essential curl file libssl-dev libxdo-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev   # pencere için (Tauri)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # Rust
curl -fsSL https://bun.sh/install | bash                             # Bun (Node gerekmez)
```

Yalnızca node veya `ilmek` komutu için ilk satırdaki pencere kitaplıkları ve Bun gerekmez.

### Node'u çalıştır

```sh
cd project-ilmek
sudo ufw allow 47400/udp && sudo ufw allow 47401:47402/tcp    # güvenlik duvarı açıksa
scripts/quickstart.sh                                          # açık kalsın; Ctrl-C durdurur
```

Kodu güncelledikten sonra: Ctrl-C, sonra `scripts/quickstart.sh`. Veriler, betiği çalıştırdığın
klasördeki `ilmek-demo` içinde kalır; aynı klasörden çalıştır.

### `ilmek` komutu (istemci)

```sh
cargo build -p ilmek-cli
target/debug/ilmek join                                        # node'u kendisi bulur
target/debug/ilmek call ilmek://nuri/forum forum.topics.list
```

Bulamazsa yalnızca node'un IP'sini ver: `target/debug/ilmek join --registry NODE_IP:47401`.
IP için `ip -4 route get 1.1.1.1` çıktısındaki `src` (`hostname -I`'in ilk adresi sanal köprü olabilir).

### İlmek penceresini aç

```sh
cd apps/client
bun install
bun run tauri build --debug --no-bundle        # arayüzü derler, programı üretir
cd ../..
target/debug/ilmek-client                      # pencere açılır
```

Pencerede **Bul ve katıl**'a bas (node aynı makinede olsa da çalışır), adres çubuğuna
`ilmek://nuri/forum` yaz, **Aç**. Doğrudan açmak için: `target/debug/ilmek-client ilmek://nuri/forum`.

Dikkat: pencere için düz `cargo build -p ilmek-client-app` kullanma. Tauri, arayüzü yalnızca kendi
`tauri build` komutuyla programa gömer (aksi halde geliştirme sunucusunu arar; bu denenmedi). Yukarıdaki
`tauri build` komutunu kullan. Başka klasörde veri istersen `ILMEK_DATA_DIR=/yol target/debug/ilmek-client`
(varsayılan `~/.local/share/net.ilmek.client`).

### Tarayıcıdan `ilmek://` aç

İstemci her açılışta `ilmek://` için bu kullanıcıya bir `.desktop` kaydı yazar
(`~/.local/share/applications/ilmek-client-handler.desktop`, programın o anki yolunu gösterir).
Programı en az bir kez aç, sonra:

```sh
xdg-open "ilmek://nuri/forum"        # çalışıyorsa pencere açılır (denendi)
```

Chrome/Firefox adres çubuğuna `ilmek://nuri/forum` yazınca aynı kaydı kullanır ve bir kez "açılsın mı?"
diye sorar (Chrome ile **denenmedi**). İstemci zaten açıksa yeni sekme aynı programda açılır.
Programı başka yere taşırsan bir kez yeniden aç ki kayıt güncellensin.

### Paket olarak kur (.deb)

```sh
cd apps/client && bun run tauri build --bundles deb && cd ../..
sudo dpkg -i target/release/bundle/deb/Ilmek_0.1.0_amd64.deb
sudo apt -f install        # eksik bağımlılık çıkarsa (libwebkit2gtk-4.1-0, libgtk-3-0)
ilmek-client               # veya uygulama menüsünden "Ilmek"
```

Paket derlendi, `dpkg-deb -I` ile bakıldı (`Package: ilmek`) ve `dpkg --dry-run -i` paketi kabul etti;
gerçek kurulum (root gerekir) bu çalışmada yapılmadı. Kurulum `ilmek://` kaydını da getirir
(`.desktop` içinde `MimeType=x-scheme-handler/ilmek`); programı bir kez açmak gerekmez.

Uygulamanın ürün adı **ASCII** olmalı (`Ilmek`): Debian paket adı `İlmek`'ten türetilince
`dpkg` "invalid package name … character not allowed" ile reddeder (pencere başlığı yine "İlmek").

---

## macOS

### Gerekenler

```sh
xcode-select --install                                           # C derleyicisi
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust
brew install oven-sh/bun/bun                                     # Bun (Node gerekmez)
```

### `ilmek` komutu (istemci)

```sh
cd project-ilmek
cargo build -p ilmek-cli
target/debug/ilmek join                                          # node'u kendisi bulur
target/debug/ilmek call ilmek://nuri/forum forum.topics.list
```

Bulamazsa: `target/debug/ilmek join --registry NODE_IP:47401`.

### İlmek penceresi ve tarayıcı bağlantısı

macOS `ilmek://` şemasını yalnızca **kurulu bir `.app`** için kaydeder; `tauri dev` ile çalışmaz.

```sh
bun install
cd apps/client && bun run tauri build --bundles app && cd ../..
osascript -e 'quit app "Ilmek"' 2>/dev/null                      # açıksa kapat
rm -rf "/Applications/Ilmek.app"
cp -R "target/release/bundle/macos/Ilmek.app" /Applications/
open "/Applications/Ilmek.app"                                   # imzasız: ilk sefer sağ tık > Aç
```

Pencerede **Bul ve katıl**'a bas. Sonra:

```sh
open "ilmek://nuri/forum"                                        # önce Terminal'den doğrula
```

Chrome'da adres çubuğuna `ilmek://nuri/forum` yaz, "açılsın mı?" sorusuna **Aç** de. Chrome
yazdığını arama sanırsa bağlantılı bir sayfa kullan:

```sh
echo '<a href="ilmek://nuri/forum">Forum</a>' > /tmp/ilmek.html && open -a "Google Chrome" /tmp/ilmek.html
```

Kodu güncellediysen `.app`'i yukarıdaki gibi yeniden derle ve değiştir; CLI'yi ayrıca `cargo build -p ilmek-cli`
ile güncelle. Terminal çalışıp Chrome sormuyorsa Chrome'u tamamen kapat (Cmd+Q) ve
`chrome://settings/handlers` içinde `ilmek`'in engelli olmadığına bak.

---

## Windows

### Gerekenler

1. **Visual Studio Build Tools 2022**, "Desktop development with C++" iş yüküyle.
2. **Rust** (MSVC): <https://rustup.rs> (`rustup-init.exe`).
3. **WebView2**: Windows 11'de hazır; Windows 10'da <https://developer.microsoft.com/microsoft-edge/webview2/>.
4. **Bun** (PowerShell): `powershell -c "irm bun.sh/install.ps1 | iex"`.

### `ilmek` komutu (istemci), PowerShell

```powershell
cd project-ilmek
cargo build -p ilmek-cli
target\debug\ilmek.exe join                                      # node'u kendisi bulur
target\debug\ilmek.exe call ilmek://nuri/forum forum.topics.list
```

Bulamazsa: `target\debug\ilmek.exe join --registry NODE_IP:47401`. İlk çalıştırmada Windows Güvenlik
Duvarı "ağa erişim izni" sorabilir; **Özel ağlar**'a izin ver.

### İlmek penceresi ve tarayıcı bağlantısı

```powershell
bun install
cd apps\client
bun run tauri build --bundles nsis
cd ..\..
```

Kurulum dosyası `target\release\bundle\nsis\` altında oluşur (`…-setup.exe`); çalıştır ve kur. Program
`ilmek://` şemasını kullanıcı için kaydeder (kurulumda ve her açılışta). Pencerede **Bul ve katıl**'a bas,
sonra Chrome'a `ilmek://nuri/forum` yaz veya Başlat > Çalıştır'a yaz.

### Windows'ta node

`ilmek node` Windows'ta denenmedi (arka uç süreçleri ve yol varsayımları Linux/macOS içindir).
Node'u Linux'ta çalıştır, Windows'u istemci olarak kullan.

---

## Sorun giderme (hepsi için)

| Belirti | Neden / çözüm |
| --- | --- |
| `no node found` | Node açık değil, ya da güvenlik duvarı `47401-47402/tcp` (ve `47400/udp`) kapatıyor. Çözüm: portları aç, ya da `--registry NODE_IP:47401` |
| `did not answer like an İlmek registry` | Node eski derlemeden başlamış: yeniden derle ve node'u yeniden başlat |
| Pencere açılır, katılma ekranında kalır | **Bul ve katıl**'a bas, sonra bağlantıyı tekrar aç |
| `ilmek://` hiçbir şey açmıyor | Programı (Linux: derlenmiş `ilmek-client`; macOS/Windows: kurulu uygulama) en az bir kez elle aç |
| Çalışan ama eski davranış | Program açıktı: kapat, yeniden aç |
