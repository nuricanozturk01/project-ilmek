# Nasıl çalıştırılır: node, host ve istemciler

Bu belge, İlmek'i birden çok makinede nasıl ayağa kaldıracağını anlatır. Ayrıntılı ilk demo için bkz. [TWO_MACHINE_DEMO](TWO_MACHINE_DEMO.md).

## Başlatma (kısa)

```sh
cargo build -p ilmek-cli              # bir kere derle → target/debug/ilmek
target/debug/ilmek node               # node: açık kalmalı; durdurmak için Ctrl+C
target/debug/ilmek join               # başka terminal veya başka makine: topluluğa katıl
```

Masaüstü istemci: `cd apps/client && bun run tauri dev`, açılınca **Find and join**.

## Örnek kurulum: 1 node + 1 ek host + 3 istemci

| Makine | Ne çalışır | Komut |
|---|---|---|
| A (192.168.1.10) | node (registry + host) | `target/debug/ilmek node` |
| B | ek host (kendi uygulamalarını barındırır) | `target/debug/ilmek host start --registry 192.168.1.10:47401` |
| C, D, E | istemci | `target/debug/ilmek join --registry 192.168.1.10:47401` |

Sıra: önce A'da node'u başlat, sonra B'de host'u, sonra istemcileri. Aynı alt ağda ve UDP 47400 açıksa `--registry` yazmana gerek yok; makineler node'u kendiliğinden bulur.

Her makinede önce bir kere `cargo build -p ilmek-cli` çalıştır.

## Kavramlar

- **Node** (`ilmek node`): kayıt sunucusu (registry) ve uygulama sunucusu (host) tek süreçte. İlk çalıştırmada topluluğu (community) oluşturur.
- **Host** (`ilmek host start`): ek uygulama sunucusu. Ağdaki node'u bulur, topluluğa kendiliğinden katılır ve uygulama barındırabilir.
- **İstemci** (`ilmek join`, masaüstü uygulaması): topluluğa katılır, uygulamaları çözer ve açar.

Portlar: keşif **47400/udp**, registry **47401/tcp**, host **47402/tcp** (varsayılan).

## 1. Tek makine (node)

```sh
ilmek node                  # ~/.ilmek altında veri tutar; $ILMEK_HOME ile değiştirilir
```

Yalnızca bu makinede dinlemek için `--local-only` ekle.

## 2. Uygulama yayınlama

Node çalışırken, başka bir terminalde (aynı `ILMEK_HOME`):

```sh
ilmek app pack <uygulama-klasörü> --out <paket-klasörü>
ilmek app install <paket-klasörü>
ilmek app set-backend <ad> --exe <çalıştırılabilir>     # backend varsa
ilmek app start <ad>
ilmek app publish <ad>
```

Java backend'li örnek (todo): `examples/tasks` için frontend'i önce derle ve paketle:

```sh
scripts/packaging/pack-angular-app.sh examples/tasks/frontend-angular examples/tasks <paket-klasörü>
```

(Bu betik gerçek `ng` ile tam olarak çalıştırılmadı; bkz. [TEST_REPORT](TEST_REPORT.md).)

## 3. Ek host (çok host)

Aynı topluluğa, başka makinelerde host ekle. Node'u kendiliğinden bulur:

```sh
ilmek host start                           # ağdaki node'u ara
ilmek host start --registry 192.168.1.10:47401   # adresi elle ver
```

## 4. İstemciler (çok istemci)

Her cihazda:

```sh
ilmek join                                 # ağdaki topluluğa katıl
ilmek join --registry 192.168.1.10:47401   # adresi elle ver
ilmek resolve ilmek://<ns>/<ad>            # uygulamayı çöz
ilmek fetch ilmek://<ns>/<ad>              # paketi indir ve doğrula
ilmek call ilmek://<ns>/<ad> <yöntem> --body '{...}'
```

Masaüstü istemci: `cd apps/client && bun run tauri dev`, açılınca **Find and join** düğmesi.

## 5. Sınırlar (bu sürüm)

- Her `ilmek node` kendi topluluğunu oluşturur. İki node'u tek topluluk yapmak için ikinci makinede `ilmek node` değil, `ilmek host start` kullan.
- Tek registry: topluluk tek bir node'un registry'sine bağlıdır.
- Güvenlik yok: bu dal TLS ve kimlik doğrulama kullanmaz. Yalnızca güvendiğin ağda çalıştır ([SECURITY](SECURITY.md)).
- Gerçek çok makineli ağ (Wi-Fi keşfi, macOS istemci) bu belgeyi yazarken tam denenmedi; bkz. [TEST_REPORT](TEST_REPORT.md).

## Sorun giderme

- Node başlamıyorsa: `$ILMEK_HOME/node.log` dosyasına bak; port doluysa `--registry-port` / `--host-port` ver.
- Diğer makineler bulamıyorsa: UDP 47400 güvenlik duvarında açık mı, AP isolation kapalı mı kontrol et; yoksa `--registry` ile adres ver.
- Uygulama açılmıyorsa: `ilmek app list` çıktısında `state=running` olmalı.
