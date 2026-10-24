> **Dikkat:** Bu belge `main` dalının (TLS, sertifika, iki node, `ilmek-identity`/`ilmek-network` crate'leri) revizyon `8da5f7b` halini anlatır. `feat/simple-no-security` dalında bu yapı değişti; güncel anlatım için [README](../README.md), [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) ve [docs/REHBER.md](../docs/REHBER.md) okunmalıdır. Bu seri bu dal için güncellenmedi.

# 01 — Genel Bakış: İlmek nedir, ne yapar, nasıl bir bütündür

Revizyon: `8da5f7b` (2026-10-23). Satır/test sayıları bu revizyona aittir.

Etiketler (bkz. plan): **[KOD]** kodu okuyarak doğrulandı · **[ÇALIŞTIRILDI]** komut gerçekten
çalıştırıldı · **[BELGE]** yalnızca dokümanda yazıyor, bu belgede kodda doğrulanmadı ·
**[TEST EDİLMEDİ]** · **[YORUM]** yazarın açıklaması, kodun kendisi değil.

---

## 1. Amaç

Bu belge bittiğinde şunları anlamış olmalısın:

- İlmek hangi problemi çözüyor ve neyi bilerek çözmüyor.
- Sistemde hangi roller var (registry, host, yayıncı, istemci, arka uç, yönlendirici düğüm) ve
  her biri neyi bilir, neyi bilmez.
- Bir adres (`ilmek://nuri/forum`) açıldığında baştan sona ne olur.
- Katmanlar neler: neyi hazır araçlara bıraktık, neyi kendimiz yazdık.
- Depoda ne var, ne **yok**; neyin gerçekten test edildiği, neyin edilmediği.

Sonraki belgeler (02 → 37) bu resmin parçalarını kodla birlikte açar. Burada kod azdır; amaç
zihinde doğru çerçeveyi kurmaktır.

## 2. Önce bilmen gerekenler

Hiçbir önkoşul yok. Yeni terimler ilk geçtikleri yerde tanımlanır. Proje daha basit bir dille
`docs/REHBER.md` içinde de anlatılıyor (760 satır, Türkçe). Bu serinin farkı, her iddiayı koda
bağlaması ve sonraki belgelerde kodu satır satır açmasıdır. REHBER ile çelişen bir yer bulursam
belgenin sonunda "Bulunan tutarsızlıklar" altında yazarım.

---

## 3. Problem ve çözüm

### 3.1 Problem

Bir mahalle, okul, atölye ya da afet bölgesi düşün: internet yok ya da güvenilmiyor, ama insanların
telefonları ve bilgisayarları var. Bu cihazların birbirine yerel Wi-Fi ile bağlanıp ortak
uygulamalar (forum, not defteri, duyuru panosu) kullanabilmesini istiyoruz. Hiçbir bulut hizmeti,
mobil veri ya da genel internet gerekmeden.

### 3.2 Çözümün bir cümlesi

> Aynı yerel ağdaki cihazlar `ilmek://namespace/uygulama` biçimindeki adresi bir **registry**'ye
> (isim defteri) sorar, cevapta gelen **host**'a (uygulamayı barındıran bilgisayar) doğrudan bağlanır,
> host'tan gelen **imzalı paketi** doğrular ve içindeki HTML/CSS/JavaScript'i **izole bir pencerede**
> çalıştırır.

Bu cümle `README.md`'nin ve `docs/REQUIREMENTS.md` §1'in özetidir **[BELGE]**.

### 3.3 Web'e benzer ama web değil

| | Web | İlmek |
|---|---|---|
| Adres | `https://site.com/sayfa` | `ilmek://nuri/forum` |
| İsim çözümü | Genel DNS | Topluluğun registry'si |
| Taşıma | HTTP(S) | Kendi protokolü ILMEK/1 (TLS 1.3 üstünde) |
| Kimlik | Çoğunlukla yok (sunucu sertifikası dışında) | Her cihazın topluluk sertifikası; iki yönlü (mTLS) |
| İçerik güveni | Sunucuya güvenirsin | Paket yayıncı anahtarıyla **imzalı**, her dosya hash'lenmiş |
| Veri nerede | Sunucuda (bulutta) | Uygulamayı barındıran host'ta |

---

## 4. Katmanlar: neyi hazır aldık, neyi yazdık

Sabit mimari karar (`PROTOCOL.md` §1, `docs/REQUIREMENTS.md` "Dosyalar ve kesin protokol kararı"):

```
Wi-Fi  →  IP  →  TCP  →  TLS 1.3  →  ILMEK/1
```

| Katman | Kim sağlıyor | Biz ne yapıyoruz |
|---|---|---|
| Wi-Fi (radyo, sürücü) | Donanım + işletim sistemi | Hiçbir şey. Sadece yapılandırıyoruz (`ilmek network`). |
| IP (yönlendirme) | Linux çekirdeği | Hiçbir şey. Paketleri biz taşımıyoruz, tünel yazmıyoruz. |
| TCP | İşletim sistemi | Hiçbir şey. |
| TLS 1.3 | `rustls` (bakımı yapılan kütüphane) | Kimlik doğrulama kuralları (hangi sertifika kabul). |
| ILMEK/1 | **Biz** | Frame formatı, mesajlar, durum makinesi, yetki, idempotency, paket aktarımı. |

`rustls` kullanıldığını `crates/ilmek-core/src/tls.rs` ve ilgili `Cargo.toml` gösteriyor
**[KOD, ayrıntı belge 11'de]**. "Kendi protokolümüz" ifadesi yalnızca en üst katman için doğrudur.
Radyoyu, IP'yi, TCP'yi, TLS algoritmalarını yeniden icat etmiyoruz.

### 4.1 HTTP / WebSocket / gRPC neden yok

`PROTOCOL.md` §1: HTTP, HTTPS, WebSocket ve gRPC üzerinden tünelleme yasak **[BELGE]**.
Bu yasağın kod tarafındaki yansımasını kontrol ettim:

**[ÇALIŞTIRILDI]** `cargo tree -p <crate> -e normal --prefix none` çıktısında `hyper`, `reqwest`,
`tungstenite`, `tokio-tungstenite`, `axum`, `h2`, `http`, `tonic`, `warp`, `actix-web`, `isahc`,
`ureq` adlı bağımlılık sayısı şu crate'lerde **0**: `ilmek-protocol`, `ilmek-core`,
`ilmek-registry`, `ilmek-host`, `ilmek-client`, `ilmek-cli`.

Sınırı: bu yalnızca bu listedeki isimleri aradı. "Hiçbir yerde HTTP kütüphanesi yok" demek için
tam bağımlılık listesini okumak gerekir; `Cargo.lock` ve Tauri tarafı (masaüstü kabuğu, WebView'ın
kendi kaynak eşlemesi) ayrıca bakılacak (belge 29). Bu rapor `docs/TEST_REPORT.md` A06'da da
"PARTIAL" olarak geçiyor: gerçek ağda paket yakalama yapılmadı **[BELGE]**.

**Neden?** **[YORUM]** Amaç, uygulama trafiğinin "bir web sunucusuna HTTP isteği" gibi
davranmasını engellemek ve bağlantı başına kimlik, yetki ve idempotency kurallarını protokolün
kendisinde sabitlemek. Bedeli: tarayıcıyla doğrudan konuşulamaz; bu yüzden ayrı bir istemci
(masaüstü/Android) gerekir.

---

## 5. Sabit tanımlayıcılar (tek tek kodda doğrulandı)

| Ne | Değer | Kodda |
|---|---|---|
| Protokol sürümü | `ILMEK/1` | `crates/ilmek-protocol/src/lib.rs:1` (yorum), spesifikasyon |
| TLS ALPN | `ilmek/1` (7 ASCII byte) | `crates/ilmek-core/src/tls.rs:24` → `pub const ALPN: &[u8] = b"ilmek/1";` **[KOD]** |
| Frame işareti (magic) | `ILM1` (4 byte) | `crates/ilmek-protocol/src/frame.rs:5` → `MAGIC: [u8; 4] = *b"ILM1"` **[KOD]** |
| Frame ön eki uzunluğu | 12 byte | `frame.rs:6` `PREFIX_LEN` **[KOD]** |
| Azami metadata | 65 536 byte | `frame.rs:7` `MAX_METADATA_LEN` **[KOD]** |
| Azami gövde | 262 144 byte | `frame.rs:8` `MAX_BODY_LEN` **[KOD]** |
| RPC hash öneki | `ILMEK-CALL-V1` + NUL | `crates/ilmek-protocol/src/digest.rs:7` **[KOD]** |
| Manifest imza öneki | `ILMEK-MANIFEST-V1` + NUL | `digest.rs:8` **[KOD]** |
| Adres şeması | `ilmek://` | `crates/ilmek-protocol/src/address.rs` (belge 05) |
| CLI komutu | `ilmek` | `crates/ilmek-cli/src/cli.rs:8` `name = "ilmek"` **[KOD]** |
| JS SDK | `@ilmek/sdk`, arayüz `IlmekAppSDK` | `packages/sdk-js` (belge 28) |
| UUID alanları | UUIDv7 | `crates/ilmek-protocol/src/ids.rs` (`Uuid7`) (belge 05) |

Bunlar `docs/REQUIREMENTS.md` "Adlandırma sözleşmesi" tablosuyla birebir uyumlu.

### 5.1 Portlar hakkında dikkat

Spesifikasyon ve ARCHITECTURE.md TCP `47401` (kontrol + uygulama trafiği) ve `47402`
(yalnızca enrollment/katılım) diyor **[BELGE]**. Kodu arayınca şunu buldum:

- Registry ayarlarında portlar **zorunlu alan**: `control_listen` ve `enroll_listen` birer
  `SocketAddr`, `serde(default)` yok (`crates/ilmek-registry/src/settings.rs:24-25`) **[KOD]**.
  Yani 47401/47402 kodda bir "varsayılan sabit" değil; örnek yapılandırma dosyasında yazan değerler:
  `configs/registry.toml.example:3-4` (`10.77.10.1:47401`, `10.77.10.1:47402`) **[KOD]**.
- Host için de örnek dosyada `listen = "10.77.10.10:47401"` var (`configs/host.toml.example:5`).
- `ilmek-network` güvenlik duvarında varsayılan olarak bu iki portu açar
  (`crates/ilmek-network/src/config.rs:127` → `vec![47401, 47402]`) **[KOD]**.

Sonuç: "Port 47401" bir **kural değil, örnek yapılandırma ve ağ aracının varsayılanıdır**.
Bir kurulumda farklı port seçilebilir; önemli olan registry kaydındaki adresin gerçek portu
taşımasıdır (PROTOCOL.md §2: "Adres kayıtları gerçek portu taşır").

---

## 6. Roller

Aşağıdaki tablo `docs/ARCHITECTURE.md` "Roles" tablosunun açılmış halidir; çalıştırma komutları
`crates/ilmek-cli/src/cli.rs` içindeki `Command` enum'una karşılık gelir **[KOD]**
(`Community`, `Identity`, `Publisher`, `Registry`, `Host`, `App`, `Resolve`, `Diagnose`, `Fetch`,
`Call`, `Doctor`, `Network`, `Backup`).

| Rol | Ne yapar | Kalıcı durumu | Komut |
|---|---|---|---|
| **Registry** | Topluluk CA'sı; üyelik (enrollment); hangi isim kimin; hangi uygulama hangi host'ta; iptal listesi. Uygulama trafiğini **taşımaz**. | `registry.db`, CA sertifikası/anahtarı, `community.json` **[BELGE: ARCHITECTURE]** | `ilmek registry serve` |
| **Host** | Aynı anda **birden çok** uygulamayı barındırır; her uygulamanın kendi `app_id`'si, veri dizini, süreci vardır. | `host.db`, `apps/<app_id>/{versions,data,logs}` **[BELGE]** | `ilmek host serve` |
| **Yayıncı** | Uygulamayı paketleyip imzalayan kişi/anahtar. Bir `namespace`'in sahibi. | Yayıncı anahtar dosyası (yalnız sahibi okur) | `ilmek app pack`, `app publish` |
| **İstemci** | Adres açar, paketi indirir/doğrular, uygulamayı izole çalıştırır. Cihaz = üye. | Cihaz kimliği, paket önbelleği, uygulama başına yerel depolama | `apps/client` (masaüstü) |
| **Arka uç (backend)** | Uygulamanın sunucu kısmı; host tarafından başlatılan **ayrı süreç**. | Uygulamanın kendi SQLite dosyası | `examples/*/backend` |
| **Yönlendirici düğüm** (NODE-A/B) | Wi-Fi erişim noktası + alt ağlar arası IP yönlendirme. Uygulama içeriğini işlemez. | `/etc/ilmek`, systemd, geri alma anlık görüntüsü | `ilmek network ...` |

Önemli ilişkiler:

- Bir bilgisayar birden fazla rol üstlenebilir (denemelerde tek makinede hepsi çalışır). Ama
  yazılım olarak roller ayrıdır. **[BELGE: REQUIREMENTS §5]**
- Host, yönlendirici düğüm olmak zorunda değil; uygulamalar düğümde çalışmaz ancak orada ayrı bir
  host kurarsan çalışır. **[BELGE: ARCHITECTURE]**
- Registry bir **proxy değildir**: istemci registry'den sadece "kimde, hangi sürüm, hangi hash"
  bilgisini alır; sonra host'a **doğrudan** bağlanır. **[BELGE: ARCHITECTURE, REQUIREMENTS §7]**
  Bu mantığın kodu belge 25 (istemci) ve belge 18 (katalog)'de gösterilecek.

### 6.1 Şekil

```
 istemci ──resolve (mTLS)──────▶ registry
   │                               ▲
   │ doğrudan bağlantı (mTLS)      │ kayıt, lease/heartbeat (host, yayıncı)
   ▼                               │
  host ──IPC (loopback, token)──▶ arka uç (ayrı süreç)
```

(`docs/ARCHITECTURE.md` şeması; okların yönleri kod okunarak belgelerde doğrulanacak.)

---

## 7. Bir uygulamanın hayatı (yüksek seviyeden)

Aşağıdaki akışlar `docs/ARCHITECTURE.md` "Main flows" ve `docs/REHBER.md` §4–5'in özetidir
**[BELGE]**. Her adımın kodu sonraki belgelerde açılır; yanında hangi belgeye bakacağın yazıyor.

### Adım 0 — Topluluğu kurmak (yönetici)

`ilmek community init` → topluluk CA'sı, registry'nin sertifikası ve `community.json` profili
üretilir; **CA parmak izi** ekrana yazılır. (Belge 10, 16)

### Adım 1 — Üye olmak (enrollment)

1. Yönetici `ilmek community invite` ile **tek kullanımlık, süreli** bir token üretir.
   Varsayılan ömür 15 dakika: `crates/ilmek-registry/src/admin.rs:19`
   `DEFAULT_INVITE_TTL_SECONDS = 15 * 60`, en çok 7 gün (`:20`) **[KOD]**.
2. Yeni cihaza **ayrı bir kanaldan** (elden, QR vb.) üç şey verilir: `community.json`, CA parmak
   izi, token. Parmak izi zorunludur; profil dosyasına tek başına güvenilmez **[BELGE]**.
3. Cihaz kendi özel anahtarını üretir (anahtar cihazdan çıkmaz), CSR hazırlar, registry'nin
   **enrollment portuna** (ayrı uç) sunucu doğrulamalı TLS ile gönderir.
4. Registry sertifikayı **CA'nın seçtiği kimlikle** imzalar; CSR'daki isim istekleri yok sayılır
   **[BELGE: ARCHITECTURE]**. Cihaz sertifikası 30 gün geçerli:
   `crates/ilmek-registry/src/enroll.rs:18` `LEAF_LIFETIME_DAYS: i64 = 30` **[KOD]**.

(Belge 10, 17, 25)

### Adım 2 — Uygulama yayınlamak (yayıncı + host)

1. `ilmek app pack`: dosyalar hash'lenir, sıralı `manifest.json` yazılır, yayıncı Ed25519
   anahtarıyla ayrı bir imza dosyası (`manifest.sig`) üretilir (belge 19, 08).
2. `ilmek app install`: host paketi **hazırlık dizininde** açar, imza/boyut/hash/yol kurallarını
   doğrular, yalnızca güvenilen yayıncı anahtarıyla imzalıysa kabul eder; sonra **tek hamlede**
   etkinleştirir. Doğrulama başarısızsa aktif sürüm değişmez (belge 21).
3. `ilmek app start`: arka uç ayrı süreç olarak başlar (belge 21).
4. `ilmek app publish`: registry'ye isim, sürüm, manifest hash'i, host adresi kaydedilir. Host,
   kaydı yaşatmak için periyodik **heartbeat** ile "kira" (lease) yeniler (belge 18, 23).

### Adım 3 — Adres açmak (istemci)

`ilmek://nuri/forum` için:

1. **Çözme:** İstemci registry'ye sorar; host adresi, sürüm, manifest hash'i, yayıncı anahtarı gelir.
2. **Bağlanma:** İstemci host'a doğrudan mTLS ile bağlanır. Sertifika zinciri topluluk CA'sına
   çıkmalı; ayrıca sunucunun IP SAN'ı ve kimliği kayıttakiyle eşleşmeli (belge 11).
3. **İndirme:** *control* bağlantısında manifest alınır; dosyalar **ayrı bir bulk bağlantısında**
   gelir. İstemci imzayı ve her dosyanın boyut/SHA-256 değerini kontrol eder; hepsi doğruysa
   paketi yerine taşır. Kesilen indirme doğrulanmış baytlardan devam eder (belge 26).
4. **Çalıştırma:** Sayfa platform WebView'ında sıkı bir içerik politikasıyla açılır. Sayfanın
   dışarıyla tek kanalı SDK köprüsüdür (belge 27, 28, 29).

### Adım 4 — Uygulamayı kullanmak (çağrı + güvenilir yazma)

1. Sayfa `sdk.call(method, input)` der. Runtime manifest iznini kontrol eder, **doğrulanmış
   uygulama kimliğini kendisi ekler** ve `APP_CALL` mesajını mTLS ile gönderir.
2. Host çağıranı **sertifikadan** çıkarır (istekteki iddiaya bakmaz), uygulamayı/metodu doğrular
   ve loopback IPC ile arka uca iletir (belge 22).
3. Yazma işlemlerinde istemci bir **operation ID** üretir. Arka uç iş verisini ve "bu işlem
   yapıldı" kaydını **aynı SQLite transaction'ında** yazar. Cevap yolda kaybolursa istemci aynı
   operation ID ve aynı gövdeyle tekrar dener; kopya kayıt oluşmaz, ilk sonuç döner (belge 24, 07).

### Adım 5 — İptal

Registry sertifika/yayıncı anahtarı iptallerini sürümlü, sayfalı bir **trust snapshot** olarak
yayınlar. Host ve istemciler bunu düzenli yeniler. Host varsayılanı 60 000 ms:
`crates/ilmek-host/src/settings.rs:59-61` `default_trust_ms() -> 60_000` **[KOD]**. Son tam
snapshot 24 saatten eskiyse uzak işlemler reddedilir: `crates/ilmek-core/src/trust.rs:17`
`MAX_SNAPSHOT_AGE_MS = 24 * 60 * 60 * 1000` **[KOD]**, hata kodu `TRUST_STALE` **[BELGE]**
(belge 14, 17).

---

## 8. Kimin neye güvendiği (güven sınırları)

`docs/ARCHITECTURE.md` "Process and trust boundaries" **[BELGE]**:

1. **Ağ ↔ host/registry:** mTLS 1.3, ALPN `ilmek/1`, topluluk CA'sına çıkan zincir; istemci,
   sunucunun IP SAN'ını ve peer kimliğini kontrol eder. TLS oturumları devam ettirilmez.
2. **Host ↔ arka uç:** yalnızca loopback; süreç başına token; uygulama kimliği başlangıçta
   bağlanır. Arka uç **güvenilir** yerel süreçtir. Ayrı süreç olması bir *sandbox* değildir.
3. **Sayfa ↔ runtime:** uygulamanın HTML/JS'i **güvenilmez**. Tek kanalı SDK köprüsüdür;
   köprüde host, adres veya dosya yolu alanı yoktur.
4. **Operatör ↔ ayrıcalıklı kurulum:** Wi-Fi/IP/güvenlik duvarı değişiklikleri uygulama
   runtime'ından ayrıdır, root ister ve geri alınabilir.

Bu dört madde ileride tek tek koda bağlanacak (belge 35). Şimdilik bir sonuç yeter:
**kimlik her zaman bağlantıdan (sertifikadan) türetilir, uygulamanın söylediğinden değil.**

---

## 9. Depoda ne var

Cargo workspace üyeleri `Cargo.toml` içinde **[KOD]**: 13 crate (`ilmek-protocol`, `ilmek-identity`,
`ilmek-core`, `ilmek-store`, `ilmek-registry`, `ilmek-client`, `ilmek-package`, `ilmek-app-sdk`,
`ilmek-host`, `ilmek-network`, `ilmek-testing`, `ilmek-cli`, `ilmek-runtime`), Tauri kabuğu
(`apps/client/src-tauri`) ve iki örnek arka uç (`examples/forum/backend`,
`examples/notes/backend`).

Kaynak büyüklüğü **[ÇALIŞTIRILDI]** (`.rs/.ts/.tsx/.js/.sql` satırları, `node_modules` hariç):

| Parça | Satır (yaklaşık) |
|---|---|
| ilmek-host | 4 824 |
| ilmek-core | 3 560 |
| ilmek-registry | 3 324 |
| ilmek-protocol | 3 168 |
| ilmek-runtime | 2 414 |
| ilmek-network | 2 306 |
| ilmek-cli | 1 832 |
| ilmek-client | 1 776 |
| ilmek-app-sdk | 1 621 |
| ilmek-store | 1 083 |
| apps/client (UI) | 1 016 |
| ilmek-identity | 700 |
| ilmek-package | 720 |
| ilmek-testing | 574 |
| apps/client/src-tauri | 610 |
| packages/sdk-js | 317 |

(Bu satır sayıları testleri de içerir, çünkü `src` içi `#[cfg(test)]` kodu ve `tests/` ayrı sayılmadı.
Kabaca ölçek göstermek içindir, kesin kod boyutu ölçümü değildir.)

Dokümanlar: `README.md`, `PROTOCOL.md` (607 satır, normatif), `docs/REQUIREMENTS.md`
(924 satır, orijinal şartname), `docs/ARCHITECTURE.md`, `SECURITY.md`, `OPERATIONS.md`,
`NETWORK_SETUP.md`, `IPC.md`, `APP_DEVELOPMENT.md`, `HARDWARE_INVENTORY.md`, `TEST_REPORT.md`,
`PROGRESS.md`, `REHBER.md`, `docs/decisions/0001–0004`. Ayrıntılı klasör haritası belge 02'de.

### 9.1 Bu işte ne "hazır" ne "yazdığımız"

Hazır: `rustls` (TLS), SQLite (veri), Tauri (masaüstü kabuğu), platform WebView, Linux ağ araçları
(`hostapd`, `wpa_supplicant`, `dnsmasq`, `nftables`, `ip` — `docs/ARCHITECTURE.md`'ye göre
`ilmek-network` bunları *yapılandırır*) **[BELGE]**.

Yazılan: protokol, kimlik/CA yönetimi, registry, çok uygulamalı host, paket formatı ve doğrulaması,
istemci kütüphanesi ve runtime köprüsü, SDK'lar, CLI, ağ kurulum aracı, örnek uygulamalar, testler.

---

## 10. Ne **yapmıyor** (bilerek)

`docs/ARCHITECTURE.md` "What is deliberately not built (V1)" **[BELGE]**:

- Bluetooth, mesh/WDS/VLAN, ağ parçaları arası depola-ilet (store-and-forward)
- CRDT veya veritabanı birleştirme
- Registry için yüksek erişilebilirlik (failover): tek yetkili yazar
- NAT veya genel internete herhangi bir çıkış

Ek olarak `docs/REQUIREMENTS.md` §12: NODE-A üzerindeki registry **tek yazardır**; registry
yokken yeni isim çözme ve yayınlama çalışmayabilir, önbellekteki kayıtlarla mevcut sunuculara
erişim sürebilir **[BELGE]**.

---

## 11. Neyin test edildiği, neyin edilmediği (dürüst durum)

### 11.1 Bu oturumda çalıştırdıklarım

**[ÇALIŞTIRILDI]**, revizyon `8da5f7b`, Linux x86_64 geliştirme makinesi:

| Komut | Sonuç |
|---|---|
| `cargo fmt --check` | temiz |
| `cargo clippy --workspace --all-targets -- -D warnings` | temiz |
| `cargo test --workspace` | 230 geçti, 0 başarısız |

Smoke betikleri (`scripts/smoke/*`) ve izolasyon probu **bu oturumda yeniden çalıştırılmadı**.
Sonuçları `docs/TEST_REPORT.md`'de duruyor (E1-5 … E1-8, önceki oturumda) **[BELGE]**.

### 11.2 Henüz yapılmayan

`README.md` ve `docs/TEST_REPORT.md`, açıkça şunu söylüyor **[BELGE]**:

- Her şey **tek Linux makinede, loopback üzerinde, ayrı süreçlerle** denendi.
- **Gerçek Wi-Fi**, **iki düğümlü yönlendirme**, **Android**, **Windows**, **macOS**: test edilmedi.
- Android istemci henüz **yok** (TEST_REPORT A20: "not started, by decision").
- WebView izolasyonu yalnızca Linux/WebKitGTK için doğrulandı.
- `ilmek network apply --yes` gerçek bir düğümde **hiç çalıştırılmadı**.
- `.deb` paketi üretildi (`target/release/bundle/deb/İlmek_0.1.0_amd64.deb` mevcut — **[ÇALIŞTIRILDI]**
  `ls`), ama kurulup başlatılmadı.

Bu yüzden bu proje "Wi-Fi üzerinde çalışan kanıtlanmış bir ağ" değil; **protokolü ve uygulama
yığınını gerçek soketler, gerçek süreçler ve kalıcı veriyle uygulayan, tek makinede doğrulanmış bir
uygulama + ağ kurulum araçları** setidir. Fark önemlidir: loopback testi Wi-Fi testi değildir.

---

## 12. Neden böyle? **[YORUM]**

- **Registry proxy değil:** Tüm trafiği tek makineye geçirmek hem darboğaz hem tek arıza noktası
  olurdu. İsim bilgisi küçük ve seyrek değişir, veri büyük ve sık; ayrı tutmak ölçeği düşük
  donanımda mümkün kılar.
- **Host'ta çok uygulama, ayrı süreçler:** Bir uygulamanın çökmesi diğerlerini düşürmesin diye
  (`REHBER.md` ve `TEST_REPORT.md` A13: bir arka uç çökerken diğerleri ayakta).
- **İmzalı paket + iki yönlü TLS:** Ağ güvenilmez varsayılır; Wi-Fi parolasını bilen biri
  uygulama yayınlayamaz ya da başkası gibi davranamaz.
- **Operation ID + aynı transaction'da tekilleştirme:** Zayıf Wi-Fi'de cevap kaybı olağan;
  istemci "oldu mu bilmiyorum" durumunda tekrar denemek zorundadır; kopya oluşmaması için sunucu
  tarafında kalıcı tekilleştirme gerekir.
- **Kendi protokolü:** Şartnamenin açık isteği. Bedeli: tarayıcı kullanılamaz, istemci yazılmak
  zorunda; kazanç: kimlik/yetki/idempotency kurallarının protokolde sabit olması.

---

## 13. Anlaşılma kontrolü

1. Registry neden uygulama trafiğinin içinden geçmez? Çözümlemeden sonra istemci kiminle konuşur?
2. ILMEK/1 hangi katmanın üstünde çalışır; hangi katmanları biz yazmadık?
3. `47401` ve `47402` portlarının farkı ne? Bu değerler kodda sabit mi, yapılandırma mı?
4. Bir cihaz topluluğa katılırken üç bilgiyi neden ayrı kanaldan alır? CA parmak izi olmasa ne olurdu?
5. Arka uç ayrı bir süreç ise neden "sandbox" sayılmaz? Hangi parça güvenilmez kabul edilir?
6. Cevabı kaybolan bir yazma isteği neden iki kez uygulanmaz?
7. Bir üye iptal edildiğinde, host bunu en geç ne kadar sürede öğrenir? Registry'ye 24 saatten fazla
   ulaşılamazsa ne olur?
8. Bu depoda neyin gerçek Wi-Fi'da denendiğini söyleyebilir misin?

Cevaplar: (1) §3.2 ve §6; (2) §4; (3) §5.1; (4) §7 Adım 1; (5) §8 madde 2–3; (6) §7 Adım 4;
(7) §7 Adım 5 (host varsayılanı 60 sn, 24 saat sonra `TRUST_STALE`); (8) hiçbir şey, §11.2.

---

## 14. Denetim notu (yazım sonrası)

- Alıntılanan sabitler `grep` ile dosyada bulundu: `ALPN` (`tls.rs:24`), `MAGIC`/`PREFIX_LEN`/
  `MAX_METADATA_LEN`/`MAX_BODY_LEN` (`frame.rs:5-8`), `CALL_DOMAIN`/`MANIFEST_DOMAIN`
  (`digest.rs:7-8`), `MAX_SNAPSHOT_AGE_MS` (`trust.rs:17`), `LEAF_LIFETIME_DAYS` (`enroll.rs:18`),
  `DEFAULT_INVITE_TTL_SECONDS`/`MAX_INVITE_TTL_SECONDS` (`admin.rs:19-20`), `default_trust_ms`
  (`host/settings.rs:59-61`), `control_listen`/`enroll_listen` (`registry/settings.rs:24-25`),
  `name = "ilmek"` (`cli.rs:8`). Denetimde üç satır numarası kaymıştı; düzeltildi.
- Satır numaraları 8da5f7b içindir; dosya değişirse kayar.
- `ilmek-node` adlı crate'in olmadığı `grep` ile doğrulandı (`Cargo.toml`, `crates/*/Cargo.toml`).
- Doğrulanmayan: registry/host kalıcı dosya listeleri (§6 tablosu) ve enrollment'ın CSR işleme
  ayrıntıları yalnızca ARCHITECTURE.md'ye dayanır; koda belge 10, 16, 17'de bakılacak.
- Bulunan tutarsızlık: `docs/REQUIREMENTS.md` §11 paket adı olarak `ilmek-node` sayıyor; depoda
  `ilmek-node` adlı crate **yok**, düğüm işi `ilmek-network` crate'inde. `docs/ARCHITECTURE.md`
  da `ilmek-network` diyor. Davranış değişikliği değil, adlandırma farkı; belge 02 ve 31'de ele alınacak.
- Bulunan başka tutarsızlık: `docs/PROGRESS.md` "nothing committed" diyor, ama `git log`
  commit'ler gösteriyor ve `git status` temizdi. PROGRESS.md eski kalmış olabilir.

Sonraki: [02 — Depo yapısı ve araç zinciri](02-depo-yapisi-ve-arac-zinciri.md)
