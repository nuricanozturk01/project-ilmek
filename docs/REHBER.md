# İlmek — Kısa Rehber (Türkçe)

Bu dosya "bu proje ne, ne yapıyor, nasıl denerim" sorularının cevabıdır. Ayrıntı için:
[ARCHITECTURE](ARCHITECTURE.md), [OPERATIONS](OPERATIONS.md), [NETWORK_SETUP](NETWORK_SETUP.md),
[SECURITY](SECURITY.md), [TEST_REPORT](TEST_REPORT.md).

## 1. İlmek nedir?

İnternet olmadan, yerel Wi-Fi üzerinde çalışan bir "uygulama ağı". Biri bir bilgisayarda küçük bir
uygulama (HTML/CSS/JS + istenirse arka uç) yayınlar; ağdaki herkes `ilmek://nuri/forum` gibi bir
adresle onu açar. Trafik: `Wi-Fi → IP → TCP → TLS 1.3 → ILMEK/1`. HTTP, WebSocket, bulut yok.

## 2. Parçalar ve işleri

| Parça | Ne yapar | Komut / yer |
| --- | --- | --- |
| **Registry** | Topluluğun "rehber"i: kimler üye, hangi isim kimin, hangi uygulama hangi host'ta. Üyelik sertifikalarını (CA) verir, iptal listesini tutar. Uygulama trafiğini **taşımaz**. | `ilmek registry serve` |
| **Host** | Uygulamaları barındıran makine. Aynı anda çok uygulama çalıştırır; her birinin kendi verisi, süreci, yaşam döngüsü var. | `ilmek host serve` |
| **Yayıncı** | Uygulamayı paketleyip imzalayan kişi. | `ilmek app pack/publish` |
| **İstemci** | Adresi çözer, paketi indirip doğrular, uygulamayı izole bir pencerede açar. | `apps/client` (masaüstü) |
| **Arka uç (backend)** | Uygulamanın sunucu tarafı (Rust). Veriyi SQLite'a yazar. Host onu yerel olarak başlatır. | `examples/forum/backend` |
| **Node** | İki Wi-Fi kartlı Linux PC: biri kullanıcı ağı (AP), biri iki node arası bağlantı. | `ilmek network …` |
| **`ilmek` CLI** | Hepsini yöneten tek komut. | `target/debug/ilmek` |

Kritik fikirler: üyelik = sertifika (CA parmak izi + tek kullanımlık token ile alınır); paketler
Ed25519 ile imzalı ve dosya hash'li; yazmalar **operation ID** taşır, bu yüzden bağlantı kopsa da
iki kez yazılmaz; sayfanın (HTML/JS) hiçbir yere doğrudan erişimi yok, sadece SDK köprüsü var.

## 3. Bir adresi açınca ne olur?

1. İstemci registry'ye sorar: "`nuri/forum` kimde?" → host adresi, sürüm, hash.
2. İstemci host'a **doğrudan** bağlanır (registry araya girmez).
3. Manifest + imza indirilir; her dosyanın boyutu ve SHA-256'sı doğrulanır; sonra etkinleşir.
4. Sayfa `window.ilmek.call(...)` der → istemci kimliğiyle host'a gider → host arka uca iletir.
5. Arka uç iş verisini ve tekrar-önleme kaydını **tek SQLite işleminde** yazar.

## 4. Depoda ne var?

`crates/` Rust çekirdeği (protokol, TLS, registry, host, istemci, paketleme, ağ, CLI) ·
`apps/client` masaüstü istemci · `packages/sdk-js` JS SDK · `examples/` forum, notlar, duyurular,
merhaba · `configs/` örnek ayarlar · `deploy/systemd/` servis dosyaları · `scripts/` denemeler ·
`docs/` dokümanlar (`REQUIREMENTS.md` = orijinal şartname, `PROTOCOL.md` = protokol).

## 5. Neler test edildi, neler edilmedi?

- **Edildi (bu bilgisayarda, loopback üzerinde, ayrı süreçlerle):** protokol, kimlik, registry, host,
  paketleme, güncelleme, yedek/geri yükleme, çoklu uygulama, çökme izolasyonu, iptal, Linux
  WebView izolasyonu. 230 otomatik test + 4 betik geçti.
- **Edilmedi:** gerçek Wi-Fi, iki gerçek node, Android, Windows, macOS. `ilmek network apply`
  hiçbir gerçek makinede çalıştırılmadı. Android istemcisi henüz yok. Tam tablo:
  [TEST_REPORT](TEST_REPORT.md).

## 6. Nasıl denerim?

### A) 2 dakika: hazır betikler (önerilen başlangıç)

```sh
cd ~/Desktop/project-ilmek
cargo build -p ilmek-cli -p ilmek-example-forum -p ilmek-example-notes
scripts/smoke/local-processes.sh   # kayıt → yayın → indirme → çağrı → yeniden başlatma → iptal
scripts/smoke/multi-app.sh         # 1 host'ta 3 uygulama + 2. bağımsız host; biri düşünce diğerleri çalışır
scripts/smoke/backup-restore.sh    # canlı yedek → veriyi sil → geri yükle → durum aynı mı?
cargo test --workspace             # tüm otomatik testler
```

Her biri sonunda `ALL STEPS PASSED` yazar. Geçici dizinde çalışır, işi bitince temizler.

### B) 10 dakika: elle, adım adım (tek makine, 127.0.0.1)

```sh
cd ~/Desktop/project-ilmek && cargo build -p ilmek-cli -p ilmek-example-forum
export PATH=$PWD/target/debug:$PATH ; R=$PWD
mkdir -p /tmp/demo && cd /tmp/demo
cat > registry.toml <<'E'
data_dir = "/tmp/demo/registry"
control_listen = "127.0.0.1:47401"
enroll_listen = "127.0.0.1:47402"
allowed_endpoints = ["127.0.0.0/8"]
E
cat > host.toml <<'E'
data_dir = "/tmp/demo/host"
device_dir = "/tmp/demo/host-device"
listen = "127.0.0.1:47411"
advertised = "127.0.0.1:47411"
E

# 1) Topluluğu kur (CA parmak izi yazdırılır) ve registry'yi başlat
FP=$(ilmek community init --config registry.toml --advertise-ip 127.0.0.1 2>/dev/null)
ilmek registry serve --config registry.toml 2>registry.log &

# 2) Cihazlar: host, yayıncı, alice (her biri tek kullanımlık token ile katılır)
enroll() { # AD ROLLER [ekstra invite argümanları]
  n=$1; r=$2; shift 2
  ilmek community invite --config registry.toml --role "$r" "$@" 2>/dev/null > $n.token
  ilmek identity enroll --profile registry/community.json --fingerprint $FP \
        --device $n-device --name $n --token-file $n.token 2>/dev/null | awk '{print $2}'
}
HOST=$(enroll host member,host --server-ip 127.0.0.1)
PUB=$(enroll publisher member,publisher)
enroll alice member >/dev/null

# 3) Yayıncı kaydı: "nuri" ad alanı, imza anahtarı, host izni
KEY=$(ilmek publisher keygen --out publisher.key 2>/dev/null)
PID=$(ilmek community add-publisher --config registry.toml --name nuri 2>/dev/null)
ilmek community add-publisher-device --config registry.toml --publisher $PID --peer $PUB
ilmek community add-publisher-key    --config registry.toml --publisher $PID --key $KEY >/dev/null
ilmek community grant-namespace      --config registry.toml --publisher $PID --namespace nuri
ilmek community grant-host           --config registry.toml --publisher $PID --host $HOST
ilmek host trust-publisher --config host.toml --key $KEY

# 4) Host'u başlat, forum'u kur ve yayınla
ilmek host serve --config host.toml 2>host.log &
ilmek app pack $R/examples/forum/app --key publisher.key --out forum-pkg --publisher-id $PID >/dev/null
ilmek app install     --config host.toml forum-pkg
ilmek app set-backend --config host.toml nuri/forum --exe $R/target/debug/ilmek-forum-backend
ilmek app start       --config host.toml nuri/forum
ilmek app publish     --config host.toml nuri/forum --device publisher-device

# 5) Kullan
ilmek resolve  ilmek://nuri/forum --device alice-device
ilmek diagnose ilmek://nuri/forum --device alice-device
ilmek call ilmek://nuri/forum forum.topics.create --write \
      --body '{"title":"Merhaba","body":"ilk konu"}' --device alice-device
ilmek call ilmek://nuri/forum forum.topics.list --device alice-device
ilmek app list --config host.toml

# 6) Yedek / geri yükleme denemesi
ilmek backup host --config host.toml --out /tmp/demo/host-backup
ilmek backup verify --dir /tmp/demo/host-backup

# Temizlik
kill %1 %2 ; rm -rf /tmp/demo
```

`ilmek app list` çıktısında `state=running` görmelisin. Host'u öldürüp yeniden başlatırsan konular
yerinde durur.

### C) Masaüstü istemci

```sh
cd apps/client && pnpm install && pnpm tauri dev      # geliştirme modu
# veya paket: target/release/bundle/deb/İlmek_0.1.0_amd64.deb  (sudo dpkg -i …)
```

İstemci topluluğa `community.json` + parmak izi + token ile katılır. **Uyarı:** `.deb` kurulup
açılmadı; arayüzü baştan sona elle denemedim, yalnızca otomatik izolasyon probunda pencereyi
sürdüm (`scripts/e2e/isolation-probe.sh`, ekran gerekir).

### D) Gerçek donanım (iki node) — sırasıyla

1. Her node'da `sudo apt install iw hostapd dnsmasq nftables` ve `ilmek doctor`; `iw list` içinde
   kullanacağın iki kartın da **AP** modunu desteklediğini doğrula.
2. `configs/node-a.toml.example` / `node-b.toml.example` dosyalarını kopyala; arayüz adı, kanal,
   `country`, MAC adreslerini düzenle; parolaları `/etc/ilmek/secrets/*.pass` içine (mod 600) yaz.
3. `ilmek network plan --config node.toml` → ne yapacağını oku (hiçbir şey değiştirmez).
4. **Konsol erişimi varken** `sudo ilmek network apply --config node.toml --yes`.
5. Sorun olursa `sudo ilmek network rollback`. Ayrıntı ve doğrulama listesi:
   [NETWORK_SETUP](NETWORK_SETUP.md).
6. Sonra registry'yi NODE-A'da, host'u ayrı bir bilgisayarda B bölümündeki gibi kur.

### E) Android

Henüz yok; sen test edeceksin. Bu bilgisayarda Android SDK/NDK/emülatör var ama Rust Android
hedefleri kurulu değil (`rustup target add aarch64-linux-android …` gerekir).

## 7. Aklına gelebilecek sorular

- **İnternet gerekir mi?** Hayır. Normal çalışma için hiçbir dış servis yok.
- **Registry düşerse?** Daha önce çözülmüş adresler istemci önbelleğinden açılır, mevcut host'lara
  erişim sürer. Yeni isim çözme ve yayınlama çalışmaz. Güven bilgisi en fazla **24 saat** eski
  kalabilir; sonra uzak işler `TRUST_STALE` ile reddedilir. Yedek registry yok.
- **Sertifikalar ne zaman biter?** Cihaz/host: **30 gün**, registry sertifikası varsayılan 30 gün.
  Otomatik yenileme yok; takvime not al. Yenileme: [OPERATIONS](OPERATIONS.md).
- **Bir üyeyi nasıl atarım?** `ilmek community revoke-peer --config registry.toml --peer <id>`;
  yaklaşık bir dakika içinde (güven yenileme aralığı) oturumları kesilir.
- **Kaç uygulama çalışır?** Sabit sınır yok; makinenin gücüne bağlı.
- **Uygulamalar birbirinin verisini görür mü?** Hayır: ayrı `app_id`, ayrı veri dizini, ayrı süreç
  (denendi). Ama arka uç **güvenilir yerel süreçtir**, sanal alan değildir: yalnızca güvendiğin
  yayıncıların arka ucunu kur.
- **Kötü niyetli bir HTML uygulaması ne yapabilir?** Dışarıya bağlanamaz, dosya okuyamaz, başka
  uygulamaya erişemez (Linux'ta 29 saldırı denemesi engellendi). Windows/macOS/Android'de bu
  henüz doğrulanmadı.
- **Anahtarlar nerede?** Sahibine özel dosyalar (0600). CA anahtarını kaybeden/kaptıran topluluğu
  kaybeder; yedeği `--include-keys` ile alırsan şifreli sakla. Windows'ta dosya izni sıkılaştırması yok.
- **Veri nerede?** Registry: `registry.db`; host: `host.db` + `apps/<id>/data/app.db`. Yol ayarlanabilir.
- **`network apply` bilgisayarımı bozar mı?** Bu geliştirme bilgisayarında çalıştırma: Wi-Fi'ın
  varsayılan rotayı taşıdığı için araç zaten reddeder. Node'larda önce `plan`, konsol erişimi ve
  `rollback` yolu hazırken `apply`.
- **Başarısız olursa?** `ilmek diagnose ilmek://… --device …` adım adım nerede koptuğunu gösterir;
  hata tablosu [OPERATIONS](OPERATIONS.md) içinde.

## 8. Bilinen eksikler (kısa)

Android istemcisi yok · Windows/macOS derlenmedi · gerçek Wi-Fi/iki node denenmedi · AppImage
yok · servis dosyaları gerçek systemd'de çalıştırılmadı · CI yok · hiçbir şey commit edilmedi.
