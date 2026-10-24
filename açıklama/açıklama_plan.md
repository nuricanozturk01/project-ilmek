> **Duraklatıldı (2026-10-24).** Seri `main` dalının (`8da5f7b`) kodunu anlatır. `feat/simple-no-security` dalında TLS, sertifikalar, `ilmek-identity` ve `ilmek-network` kaldırıldı; belge 04 ve sonrası bu dalı değil `main`'i anlatacağından seri bu dalda sürdürülmedi. Karar için docs/decisions/0005-simple-no-security.md.

# İlmek — Açıklama Planı (v2, kaynakla denetlenmiş)

Bu dosya, projeyi baştan sona anlatan sıralı belgelerin planı ve ilerleme kaydıdır.
Belgeler bu dizinde (`açıklama/`) durur. Her belge tek bir konuyu anlatır.
"başla" veya "devam et" denince `next:` satırındaki belge yazılır; ardından aşağıdaki
`prev / now / next` alanları ve ilerleme tablosu güncellenir.

```
prev: 02-depo-yapisi-ve-arac-zinciri.md
now:  03-ag-topolojisi.md (yazıldı ve denetlendi)
next: 04-protokol-genel-kurallar.md
```

Plan v1'den farklar: v1 dosya adlarına ve satır sayılarına dayanıyordu. v2'de her kaynak dosya
tek bir belgeye atandı (bkz. "Kapsam çizelgesi"), eksik dosyalar eklendi, kodla doğrulanmamış
varsayımlar çıkarıldı, doğruluk kuralları yazıldı.

## Doğruluk kuralları (bağlayıcı)

1. **Önce oku, sonra yaz.** Bir belgeye atanan her kaynak dosya baştan sona okunur. Özet, dosya adı
   veya yorum satırına bakarak yazılmaz.
2. **Her iddianın kaynağı var.** Davranış anlatılırken `yol/dosya.rs:satır` ya da tip/fonksiyon adı
   verilir. Kaynağı gösterilemeyen cümle yazılmaz.
3. **Üç etiket.** Her önemli iddia şunlardan biridir ve belirsizse etiketlenir:
   - **[KOD]** kodu okuyarak doğrulandı,
   - **[ÇALIŞTIRILDI]** komut/test gerçekten çalıştırıldı, çıktı belgede,
   - **[BELGE]** yalnızca `PROTOCOL.md`/`REQUIREMENTS.md`/`docs/` söylüyor, kodda doğrulanmadı,
   - **[TEST EDİLMEDİ]** kodda var ama hiçbir testle ya da çalıştırmayla doğrulanmadı.
4. **Tahmin yasak.** Emin olunmayan şey "bilmiyorum, şuraya bakılmalı" diye yazılır. Plan v1'deki
   "keepalive", "rol ve yetkiler", "sürüm slotları" gibi doğrulanmamış başlıklar, kod okunana kadar
   belgeye girmez. (Ör. `host/slots.rs` sürüm slotu değil; `CallGate` ile uygulama başına eşzamanlı
   iş sınırıdır. Bu tür yanlışlar kod okunarak önlenir.)
5. **Örnek çıktılar gerçek olmalı.** Bir komut çıktısı gösteriliyorsa komut çalıştırılmış olur.
   Çalıştırılmadıysa çıktı uydurulmaz; "çalıştırılmadı" yazılır.
6. **Yazdıktan sonra denetim turu.** Belge bitince: (a) alıntı kodun dosyadaki metinle birebir
   aynı olduğu, (b) anılan her tip/fonksiyon/sabitin `grep` ile var olduğu, (c) satır numaralarının
   güncel olduğu kontrol edilir. Denetim sonucu belgenin sonuna kısa not olarak yazılır.
7. **Sayılar tarihlidir.** Satır/test sayıları revizyon `8da5f7b` içindir; belgeye revizyon yazılır.
8. **Kaynak çatışması.** Kod ile doküman çelişirse ikisi de gösterilir, kod "gerçekte olan" olarak
   sunulur, çelişki belgenin sonunda "Bulunan tutarsızlıklar" altında listelenir.
9. **Sahipsiz iddia yok.** Anlatım "iyi niyetli yorum" değil, açıklama: neden böyle, alternatif ne,
   bedeli ne — bu kısımlar yazarın yorumudur ve **[YORUM]** diye işaretlenir; kodun kendisinden ayrılır.

## Her belgenin ortak yapısı

1. **Amaç** — bu belge neyi açıklıyor, okuyunca ne anlaşılmış olacak.
2. **Önce bilmen gerekenler** — hangi önceki belgeler/kavramlar gerekli.
3. **Kavramlar** — terimler, sade dille (yeni terim ilk geçtiği yerde tanımlanır).
4. **Kod turu** — dosya dosya, tip tip, fonksiyon fonksiyon; önemli parçalar birebir alıntı.
5. **Akış** — adım adım örnek senaryo (hangi dosya/fonksiyon, hangi sırayla).
6. **Neden böyle?** — tasarım kararı, alternatifler, bedeli **[YORUM]**.
7. **Güvenlik ve hata durumları** — neyin ters gidebileceği ve kodun nasıl savunduğu.
8. **Testler** — bu parçayı hangi test dosyası/fonksiyonu doğruluyor, neyi doğrulamıyor.
9. **Sınırlar ve bilinen eksikler**.
10. **Anlaşılma kontrolü** — 5-8 soru (cevapları belgenin altında, kapalı bölümde değil, düz metin).
11. **Denetim notu** — kural 6'nın sonucu.
12. **Köprü** — sonraki belgeye bağlantı.

## Belge listesi (37 belge)

### Bölüm A — Zemin

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 01 | `01-genel-bakis.md` | Proje ne, kim için, hangi problemi çözüyor. Roller: registry, host, yayıncı, istemci, yönlendirici düğüm. Registry proxy değildir. Portlar 47401 (kontrol/uygulama) ve 47402 (kayıt/enrollment). Bir uygulamanın hayatı (yayınla → çöz → indir → çalıştır → çağır → iptal). Sabit tanımlayıcılar. HTTP/WebSocket neden yok. Neyin test edildiği/edilmediği. `docs/REHBER.md` ile ilişkisi. | `README.md`, `docs/ARCHITECTURE.md`, `docs/REHBER.md`, `docs/REQUIREMENTS.md` |
| 02 | `02-depo-yapisi-ve-arac-zinciri.md` | Cargo workspace (üyeler, `[workspace.dependencies]`, lint'ler), pnpm workspace, `rust-toolchain.toml`, `.nvmrc`, lockfile'lar, `clippy.toml`, `rustfmt.toml`, `.prettierrc`; crate bağımlılık yönü (`cargo metadata` ile doğrulanarak); klasör haritası. | kök dosyalar, her `Cargo.toml`, `package.json` |
| 03 | `03-ag-topolojisi.md` | Fiziksel: iki düğüm, AP + düğümler arası Wi-Fi, uç cihazlar, bağımsız host. Mantıksal: DNS/DHCP yok mu var mı (kodla doğrulanır), adresleme, port akışları, kontrol/veri düzlemi. Donanım gereksinimleri; gerçekte ne test edilmedi. | `docs/NETWORK_SETUP.md`, `docs/HARDWARE_INVENTORY.md`, `configs/*.example`, `docs/REHBER.md` §3 |

### Bölüm B — Protokol (`ilmek-protocol`, `PROTOCOL.md`)

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 04 | `04-protokol-genel-kurallar.md` | `PROTOCOL.md` §1–3, 6, 7, 18, 19, 21: mimari karar, uç türleri, TLS güven zinciri, bağlantı durum makinesi (spesifikasyon düzeyi), istek/yanıt ve limitler, sürümleme, kaynak yönetimi, açık sınırlar. Her kuralın hangi belgede (koda) karşılığı olduğunun haritası. | `PROTOCOL.md` |
| 05 | `05-frame-json-ve-tanimlayicilar.md` | Byte düzeyi frame, uzunluk önekleri, magic, kodlama/çözme; katı JSON (BOM, yinelenen anahtar, derinlik 16); UUIDv7; `ilmek://` adresi; domain-prefix'li hash preimage'leri. | `frame.rs`, `wire/codec.rs`, `strict_json.rs`, `ids.rs`, `address.rs`, `digest.rs`, `lib.rs`, `docs/decisions/0002` |
| 06 | `06-mesaj-katalogu-ve-veri-tipleri.md` | Metadata zarfı, mesaj kataloğu, her mesajın `data` nesnesi, doğrulanmış skaler tipler, hata sözleşmesi (kodlar), yetki matrisi. | `message.rs`, `wire.rs`, `wire/types.rs`, `wire/data.rs`, `PROTOCOL.md` §4, 8, 9, 13, 14 |
| 07 | `07-yazma-idempotency-abonelik-bulk.md` | Protokol düzeyinde: işlem kimliği ve idempotency, olay aboneliği ve imleç (cursor), bulk dosya aktarımı kuralları, host kontrol mesajları. (Kodun uygulanışı ilgili sonraki belgelere bağlanır.) | `PROTOCOL.md` §9–12 |
| 08 | `08-manifest-ve-paket-semasi.md` | Manifest alanları, doğrulama, yol kuralları, izinler, imza girdisi (preimage), boyut/hash. | `manifest.rs`, `PROTOCOL.md` §15, `tests/manifest.rs` |
| 09 | `09-fixture-ve-protokol-testleri.md` | `WIRE_EXAMPLES.json`, `tests/protocol/fixtures`, `scripts/fixtures/regen_ilmek_fixtures.py`, `docs/decisions/0001`; protokol crate'inin tüm test dosyaları ne doğruluyor; fixture'lar tam protokolü neden kanıtlamaz. Testler çalıştırılır, çıktı belgeye girer. | `crates/ilmek-protocol/tests/*`, fixture dosyaları, `DOCUMENT_CHECKS.md` |

### Bölüm C — Kimlik ve taşıma (`ilmek-identity`, `ilmek-core`)

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 10 | `10-kimlik-ca-ve-anahtarlar.md` | Topluluk CA'sı, yaprak sertifika profili, IP SAN, geçerlilik, yayıncı Ed25519 anahtarı ve manifest imzası, bootstrap profili, owner-only dosya saklama, Windows boşluğu. | `ilmek-identity/src/*`, `tests/pki.rs`, `tests/publisher.rs`, `docs/decisions/0002` |
| 11 | `11-tls-ve-es-kimligi.md` | TLS 1.3 yapılandırması, ALPN, iki aşamalı kimlik doğrulama (rustls + ek kontrol), oturum devam ettirme kapalı, `peer.rs` (doğrulanmış eş kimliği), `clock.rs` (zaman portu). Reddedilen durumlar. | `tls.rs`, `tls/{client,server,error}.rs`, `peer.rs`, `clock.rs`, `tests/tls.rs` |
| 12 | `12-transport-ve-dinleyici.md` | Sınırlı okuma, bağlantı başına öncelikli tek yazıcı, kısmi/birleşik okuma, zaman aşımı; TCP kabul döngüsü, bağlantı sınırı, zarif kapanış. | `transport.rs`, `listener.rs`, `tests/transport.rs` |
| 13 | `13-oturum-sunucu-tarafi.md` | HELLO durum makinesi sunucu tarafı, istek yönlendirme, modlar (kontrol/bulk), eşzamanlılık, iptal/kapanış. `server.rs` uzun olduğundan birden fazla alt başlıkta satır satır. | `session.rs`, `session/modes.rs`, `session/server.rs`, `tests/session.rs` |
| 14 | `14-oturum-istemci-ve-iptal-bilgisi.md` | İstemci tarafı oturum (`client.rs`); iptal (revocation) bilgisi ve tazelik (`trust.rs`): snapshot sayfaları, 60 sn yenileme, 24 saat eski kuralı (kodla doğrulanır). | `session/client.rs`, `trust.rs`, `tests/trust.rs` |

### Bölüm D — Depolama ve Registry (`ilmek-store`, `ilmek-registry`)

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 15 | `15-sqlite-migrasyon-ve-yedek.md` | `Store` açma (WAL, `synchronous=FULL`, `foreign_keys=ON` — doğrulandı), ileri-yönlü migrasyon, şema sürümü kontrolü (yeni DB reddi), `unsigned.rs`, çevrimiçi yedek, ağaç yedeği/geri yükleme, doğrulama. | `ilmek-store/src/*`, `tests/store.rs`, `tests/backup.rs`, `docs/OPERATIONS.md` |
| 16 | `16-registry-cekirdegi.md` | Başlatma (`init.rs`), ayarlar, şema, `Registry` tipi, TLS sunucusu (`server.rs`), servis döngüsü, hız sınırlayıcı (`limiter.rs`), eş yönetimi (`peers.rs`), işlem kaydı (`operations.rs`), `files.rs`, `time_util.rs`, yetki kapısı (`gate.rs`). | `ilmek-registry/src/{lib,registry,settings,schema,init,files,time_util,server,service,limiter,peers,operations,gate,error}.rs` |
| 17 | `17-kayit-yonetim-ve-iptal.md` | Tek kullanımlık davet/token, ENROLL akışı ve CSR işleme (CA kimliği seçer), yenileme, operatör eylemleri (`admin.rs`), iptal snapshot'ları, sayfalı `GET_TRUST`. | `enroll.rs`, `admin.rs`, `snapshots.rs`, `trust_api.rs`, `tests/registry.rs`, `tests/renewal.rs` |
| 18 | `18-katalog-isimler-hostlar-leaseler.md` | `catalog.rs` baştan sona: namespace sahipliği, uygulama kayıtları, yayıncı yetkisi, host kaydı, lease ve heartbeat, çözümleme (resolve), çakışma/yarış davranışı. | `catalog.rs`, `tests/registry.rs` |

### Bölüm E — Paket ve Host (`ilmek-package`, `ilmek-host`, `ilmek-app-sdk`)

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 19 | `19-paket-olusturma-ve-dogrulama.md` | Paket dizin yapısı (`manifest.json`, `manifest.sig`, `assets/`), `pack`, `verify`, hash, yol güvenliği, hata türleri. | `ilmek-package/src/*`, `tests/package.rs` |
| 20 | `20-host-cekirdegi.md` | `Host`, `manager`, ayarlar, `paths`, `schema`, `store` (host DB erişimi), `stats` (ölçülemeyen = `None`), `logs`, `slots.rs` (`CallGate` ve `AppSlot`), çok uygulamalı model. | `host.rs`, `manager.rs`, `settings.rs`, `paths.rs`, `schema.rs`, `store.rs`, `stats.rs`, `logs.rs`, `slots.rs`, `error.rs` |
| 21 | `21-kurulum-baslatma-ve-supervisor.md` | Stage → doğrula → atomik aktivasyon (`install.rs`), süreç başlatma (`launcher.rs`), uygulama başına supervisor görevi: başlat, sağlık, yeniden başlat sınırı, durdur. Çökme/dayanıklılık testleri. | `install.rs`, `launcher.rs`, `supervisor.rs`, `tests/install.rs`, `tests/resilience.rs` |
| 22 | `22-host-servisi-ipc-ve-abonelikler.md` | Host'un ILMEK/1 uç noktası (manifest, asset, çağrı, abonelik), arka uç ile loopback IPC (`link.rs`, `ipc.rs`, `docs/IPC.md`), çağıran kimliğinin sertifikadan türetilmesi, abonelik yönetimi. | `service.rs`, `link.rs`, `subscriptions.rs`, `ilmek-protocol/src/ipc.rs`, `docs/IPC.md`, `tests/e2e.rs` |
| 23 | `23-host-registry-baglantisi-ve-yayin.md` | `registry_link.rs` (kayıt, lease yenileme, trust yenileme), `publish.rs` (uygulamayı registry'de yayınlama). | `registry_link.rs`, `publish.rs` |
| 24 | `24-rust-uygulama-sdk.md` | Backend yazarının yüzeyi: `runtime.rs`, `backend.rs`, `config.rs`, `dedupe.rs` (iş verisi + tekilleştirme aynı transaction), `events.rs` (kalıcı olay günlüğü, imleçler), `error.rs`. | `ilmek-app-sdk/src/*`, `tests/sdk.rs` |

### Bölüm F — İstemci

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 25 | `25-istemci-kutuphanesi.md` | `device`, `enroll`, `registry_client`, `resolver` (sınırlı çevrimdışı yedek), `connector`, `retry`, `app` (yeniden bağlanma, işlem değişmez), `error`. | `ilmek-client/src/{lib,device,enroll,registry_client,resolver,connector,retry,app,error}.rs` |
| 26 | `26-paket-indirme.md` | `download.rs` baştan sona: control + bulk bağlantı, ilerleme = alınan byte, devam (resume) doğrulaması, iptal yarışı, hash/boyut/imza, atomik taşıma. | `download.rs` |
| 27 | `27-runtime-ve-sayfa-koprusu.md` | Köprü izin modeli (uygulama kimliği/host/adres/dosya yolu alanı yok), manifest izinleri, yerel depolama kapsamı, aboneliklerin dayanıklılığı (imleç saklanır sonra ack), `assets.rs` (sayfanın yükleyebilecekleri), `session.rs`, `client.rs`, `events.rs`, `error.rs`. | `ilmek-runtime/src/*`, `tests/bridge.rs`, `tests/session.rs` |
| 28 | `28-js-sdk.md` | `@ilmek/sdk`: `sdk.ts`, `inject.ts`, `types.ts`, `uuid7.ts`, `index.ts`; `IlmekAppSDK` arayüzü; işlem kimliği üretimi; `scripts/packaging/sync-sdk.sh`. | `packages/sdk-js/*` |
| 29 | `29-masaustu-istemci-tauri.md` | Tauri kabuğu (`commands.rs`, `protocol.rs`, `sink.rs`, `state.rs`, `windows.rs`, `capabilities`, `permissions`, `tauri.conf.json`) ve React arayüzü (`features/{browse,join,settings,transfers}`, `shared/{ipc,i18n,format}`). WebView sınırları. Android durumu. | `apps/client/*` |

### Bölüm G — Operasyon

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 30 | `30-cli.md` | `ilmek` komut ağacı (`cli.rs`), her alt komut modülü (`community`, `identity`, `publisher`, `app`, `device`, `backup`, `doctor`, `network`, `serve`), çıktı biçimi, hata eşleme. Komutlar gerçekten `--help` ile çalıştırılıp doğrulanır. | `ilmek-cli/src/*`, `crates/ilmek-cli/tests/network.rs` |
| 31 | `31-ag-kurulumu-ilmek-network.md` | `NodeConfig` doğrulaması, `Plan`, üretilen dosyalar (`render.rs`), `apply` + anlık görüntü (değişiklikten önce kayıt), `rollback`, `status`, `system.rs` (OS sınırı), `cidr.rs`, yönetim bağlantısını koruma kuralları. | `ilmek-network/src/*`, `tests/node.rs`, `configs/*` |
| 32 | `32-dagitim-yedekleme-ve-isletim.md` | systemd birimleri, sysusers/tmpfiles, `.deb`, yedek/geri yükleme alıştırması, sertifika yenileme, `docs/OPERATIONS.md`; `scripts/smoke/*`. | `deploy/`, `scripts/`, `docs/OPERATIONS.md` |

### Bölüm H — Örnekler, test, güvenlik, durum

| # | Dosya | İçerik | Ana kaynaklar |
|---|---|---|---|
| 33 | `33-ornek-uygulamalar.md` | forum, notes (backend'li), announcements, hello-html (statik), isolation-probe: manifest, HTML/JS, backend, SDK kullanımı, veri. `docs/APP_DEVELOPMENT.md`. Yeni uygulama yazma adımları. | `examples/*`, `docs/APP_DEVELOPMENT.md` |
| 34 | `34-test-stratejisi.md` | `ilmek-testing` yardımcıları, birim/entegrasyon/e2e katmanları, smoke betikleri, izolasyon probu (`scripts/e2e`), testlerin çalıştırılıp sonucun kaydı. Neyi kanıtlıyor, neyi kanıtlamıyor. | `ilmek-testing/*`, tüm `tests/`, `scripts/`, `docs/TEST_REPORT.md` |
| 35 | `35-guvenlik-modeli.md` | Güven sınırları, tehdit→savunma→kod karşılığı tablosu, güvenilir backend ≠ sandbox, sayfa izolasyonu, anahtar saklama, DoS sınırları. | `docs/SECURITY.md` + koda bakarak doğrulama |
| 36 | `36-kabul-matrisi-ve-kalan-isler.md` | A01–A24 ve P01–P24 durumları (`docs/TEST_REPORT.md`'den, mümkün olanlar yeniden çalıştırılarak), bilinen kusurlar, bloke olanlar ve gereken donanım/erişim. | `docs/TEST_REPORT.md`, `docs/REQUIREMENTS.md` §26, `PROTOCOL.md` §20 |
| 37 | `37-uctan-uca-izleme-ve-sozluk.md` | Bir isteğin (ör. forum'a gönderi yazmak) cihazdan diske tüm yolu, adım adım dosya/fonksiyon. Terimler sözlüğü. Önerilen okuma sırası. | tüm önceki belgeler |

## Kapsam çizelgesi (hiçbir dosya sahipsiz kalmasın)

Her `.rs`/`.ts`/`.tsx` kaynağı ve test dosyası aşağıdaki belgelerden tam birine atanmıştır.
Yazım sırasında bu çizelge işaretlenir; boşluk bulunursa plan güncellenir.

| Konum | Belge |
|---|---|
| `ilmek-protocol/src/{frame,strict_json,ids,address,digest,lib}.rs`, `wire/codec.rs` | 05 |
| `ilmek-protocol/src/{message,wire}.rs`, `wire/{types,data}.rs` | 06 |
| `ilmek-protocol/src/manifest.rs` | 08 |
| `ilmek-protocol/src/ipc.rs` | 22 |
| `ilmek-protocol/tests/*`, `tests/protocol/*` | 09 (ve ilgili alt konu belgelerinde atıf) |
| `ilmek-identity/**` | 10 |
| `ilmek-core/src/{tls,tls/*,peer,clock}.rs` | 11 |
| `ilmek-core/src/{transport,listener}.rs` | 12 |
| `ilmek-core/src/{session,session/modes,session/server}.rs` | 13 |
| `ilmek-core/src/{session/client,trust}.rs` | 14 |
| `ilmek-store/**` | 15 |
| `ilmek-registry/src/{lib,registry,settings,schema,init,files,time_util,server,service,limiter,peers,operations,gate,error}.rs` | 16 |
| `ilmek-registry/src/{enroll,admin,snapshots,trust_api}.rs` | 17 |
| `ilmek-registry/src/catalog.rs` | 18 |
| `ilmek-package/**` | 19 |
| `ilmek-host/src/{host,manager,settings,paths,schema,store,stats,logs,slots,error,lib}.rs` | 20 |
| `ilmek-host/src/{install,launcher,supervisor}.rs` | 21 |
| `ilmek-host/src/{service,link,subscriptions}.rs` | 22 |
| `ilmek-host/src/{registry_link,publish}.rs` | 23 |
| `ilmek-app-sdk/**` | 24 |
| `ilmek-client/src/**` (download hariç) | 25 |
| `ilmek-client/src/download.rs` | 26 |
| `ilmek-runtime/**` | 27 |
| `packages/sdk-js/**` | 28 |
| `apps/client/**` | 29 |
| `ilmek-cli/**` | 30 |
| `ilmek-network/**` | 31 |
| `deploy/**`, `scripts/**` (fixtures hariç) | 32 |
| `examples/**` | 33 |
| `ilmek-testing/**` | 34 |
| `crates/*/tests/**` (konusuna göre) | ilgili konu belgesi + 34'te toplu bakış |

## Yazım notları

- Dil: Türkçe. Kod, tip adları, dosya yolları, protokol terimleri orijinal halde.
- Uzun dosyalar (`server.rs` 857, `catalog.rs` 720, `download.rs` 730, `service.rs` 564, `store.rs` 559)
  birden çok alt başlıkta, gerekirse belge içinde "Bölüm 1/2" diye bölünerek anlatılır.
- Çapraz bağlantı: `[bkz. 13](13-oturum-sunucu-tarafi.md)`.
- Plan değişirse önce bu dosya güncellenir (tablo, sayılar, kapsam çizelgesi).
- Sıfırdan öğrenen biri için: her belge yeni terimi ilk geçtiği yerde tanımlar; 37. belgede sözlük.
- `docs/REHBER.md` mevcut bir giriş kılavuzu. Tekrar etmek yerine, burada koda inen ayrıntı verilir
  ve REHBER'e atıf yapılır; REHBER'in koda aykırı bir ifadesi bulunursa 8. kural uygulanır.

## İlerleme tablosu

| Bölüm | Belgeler | Durum |
|---|---|---|
| A Zemin | 01–03 | 01, 02, 03 yazıldı (Bölüm A tamam) |
| B Protokol | 04–09 | yazılmadı |
| C Kimlik/taşıma | 10–14 | yazılmadı |
| D Depolama/Registry | 15–18 | yazılmadı |
| E Paket/Host | 19–24 | yazılmadı |
| F İstemci | 25–29 | yazılmadı |
| G Operasyon | 30–32 | yazılmadı |
| H Örnek/test/güvenlik/durum | 33–37 | yazılmadı |
