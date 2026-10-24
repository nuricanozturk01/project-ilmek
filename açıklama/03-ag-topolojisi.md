> **Dikkat:** Bu belge `main` dalının (TLS, sertifika, iki node, `ilmek-identity`/`ilmek-network` crate'leri) revizyon `8da5f7b` halini anlatır. `feat/simple-no-security` dalında bu yapı değişti; güncel anlatım için [README](../README.md), [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) ve [docs/REHBER.md](../docs/REHBER.md) okunmalıdır. Bu seri bu dal için güncellenmedi.

# 03 — Ağ Topolojisi

Revizyon: `8da5f7b`. Etiketler: **[KOD]** kod okundu, **[ÇALIŞTIRILDI]** komut koştu, **[BELGE]** yalnızca belge söylüyor,
**[TEST EDİLMEDİ]**, **[YORUM]** yazarın yorumu.

## 1. Amaç

İlmek'in fiziksel ve mantıksal ağının nasıl kurulduğunu anlatır: iki yönlendirici düğüm, iki kullanıcı ağı, aradaki Wi-Fi köprüsü,
adresler, portlar, DHCP/DNS durumu ve güvenlik duvarı. Ayrıca bu ağın **bugüne kadar gerçekte hiç kurulmadığını** açıkça kaydeder.
Düğümleri kuran aracın iç işleyişi (`ilmek network`) 31. belgede; burada yalnızca sonuçta ortaya çıkan topoloji.

## 2. Önce bilmen gerekenler

[01](01-genel-bakis.md) (roller), [02](02-depo-yapisi-ve-arac-zinciri.md) (hangi crate ne).

## 3. Kavramlar

- **Düğüm (node):** Ağ cihazı gibi davranan eski Linux bilgisayar: IP yönlendirme, DHCP, Wi-Fi erişim noktası, güvenlik duvarı. Uygulama çalıştırmak zorunda değil.
- **Kullanıcı ağı:** Telefon/bilgisayarların bağlandığı Wi-Fi ağı (`ILMEK-A`, `ILMEK-B`).
- **Transit ağ:** İki düğümü birbirine bağlayan Wi-Fi bağlantısı (`ILMEK-TRANSIT`).
- **AP (access point) / station:** Wi-Fi'yi yayan taraf / ona bağlanan taraf.
- **DHCP:** Bağlanan cihaza otomatik IP veren servis. **DNS:** isimden IP bulan servis.
- **nftables:** Linux güvenlik duvarı. **Rezervasyon:** MAC adresine sabit IP verme.
- **Kontrol/veri düzlemi:** Burada: registry'ye giden kayıt/çözümleme trafiği ile uygulama host'una giden trafik. Registry uygulama trafiğini taşımaz.

## 4. Kod turu

### 4.1 Hedef topoloji **[BELGE]** (`docs/NETWORK_SETUP.md:10-23`, `docs/REHBER.md` §3)

```
 telefonlar/PC'ler ~~ ILMEK-A ~~ [wlan0 NODE-A wlan1] ~~ ILMEK-TRANSIT ~~ [wlan1 NODE-B wlan0] ~~ ILMEK-B ~~ telefonlar/PC'ler
 10.77.10.0/24                  10.77.10.1 10.77.254.1/30   10.77.254.2  10.77.20.1                 10.77.20.0/24
```

| Parça | Değer (referans plan) |
|---|---|
| Kullanıcı ağı A | `10.77.10.0/24`, SSID `ILMEK-A`, kanal 6, 2.4 GHz, node `10.77.10.1` |
| Kullanıcı ağı B | `10.77.20.0/24`, SSID `ILMEK-B`, kanal 1, 2.4 GHz, node `10.77.20.1` |
| Transit | `10.77.254.0/30`; NODE-A `.1` (AP, kanal 11), NODE-B `.2` (station, kanalı AP'den alır) |
| Rotalar | NODE-A: `10.77.20.0/24 via 10.77.254.2`; NODE-B: `10.77.10.0/24 via 10.77.254.1` |
| Registry | NODE-A üzerinde, `10.77.10.1:47401` (kontrol), `:47402` (kayıt) |
| Host'lar | Ayrı bilgisayarlar: HOST-1 `10.77.10.10`, HOST-2 `10.77.20.10` (DHCP rezervasyonu) |

Bu değerler `configs/node-a.toml.example`, `node-b.toml.example`, `registry.toml.example`, `host.toml.example` dosyalarıyla
birebir uyumlu (okundu) **[KOD]**. Adresler programa gömülü değil: `NodeConfig` açıklaması "No address is built into the program"
(`crates/ilmek-network/src/config.rs:9`). **[KOD]**

Önemli kararlar (belgeden ve koddan):
- **NAT yok, varsayılan rota yok.** İlmek ağları internete çıkmaz, yalnızca birbirine ulaşır (`NETWORK_SETUP.md:20`; plan notu `plan.rs:148-150`:
  "Nothing is masqueraded and no default route to an upstream network is created"). **[KOD]**
- **Wi-Fi hem erişim hem düğümler arası bağ.** Bluetooth V1 dışı. İkinci radyo NODE-A'da AP, NODE-B'de station.
  Kodda `TransitMode::{AccessPoint, Station}` (`config.rs:82-87`). **[KOD]**
- **Uygulama düğümde değil, ayrı host'ta.** Düğüm "yolu açar" (REHBER §3). Registry NODE-A'ya konabilir ama zorunlu değil
  ("any reachable machine works", `registry.toml.example:1`). **[BELGE]**

### 4.2 DNS yok, DHCP var **[KOD]**

`render::dhcp` (`render.rs:37-56`) dnsmasq yapılandırması üretir:

```
port=0
interface=<kullanıcı arayüzü>
bind-interfaces
no-hosts
dhcp-leasefile=/var/lib/ilmek/dnsmasq.leases
dhcp-range=<başlangıç>,<bitiş>,<netmask>,<kira>
dhcp-option=option:router,<node adresi>
dhcp-host=<mac>,<ip>,<ad>      # her rezervasyon için
```

`port=0` dnsmasq'in DNS özelliğini kapatır; yani düğüm **yalnızca DHCP** sunar ve router seçeneğini kendine ayarlar. Kod yorumu:
"DHCP only: DNS is switched off, and no fake internet reachability is advertised" (`render.rs:37`). **[KOD]**

Kodun geri kalanında da ağ isim çözümlemesi kullanılmıyor: `ToSocketAddrs`, `lookup_host`, `DnsName` için `crates/` ve
`apps/client/src-tauri/src` içinde `grep` sonuç vermedi **[ÇALIŞTIRILDI]**. Uç noktalar IPv4 `SocketAddr` olarak tutuluyor:
- `CommunityProfile.registry_control` ve `registry_enroll`: `SocketAddrV4` (`ilmek-identity/src/profile.rs:27-28`).
- Host `advertised`: somut IPv4 `adres:port` olmalı; `0.0.0.0` ve IPv6 reddedilir (`ilmek-host/src/settings.rs:203-207`).
- TLS sunucu kimliği sertifikadaki **IP SAN** ile doğrulanır (`pki.rs:193` `SanType::IpAddress`); ayrıntı 10–11. belgelerde.

Sonuç **[YORUM]**: ağ, isim sunucusuna ihtiyaç duymadan çalışacak şekilde tasarlanmış; "isim" kavramı (`ilmek://nuri/forum`) registry'nin
içinde çözülür, DNS'te değil. Bedel: yalnızca IPv4; host IP'si değişirse yeniden kayıt gerekir, bu yüzden DHCP rezervasyonu öneriliyor.

### 4.3 Portlar **[KOD]**

| Port | Anlam | Nerede tanımlı |
|---|---|---|
| `47401` | Kontrol/uygulama: registry'de mTLS kontrol, host'ta uygulama servisi | örnek config'ler (`registry.toml.example:3`, `host.toml.example:5-6`); yalnızca `ilmek-network` `default_ports` (`config.rs:126-128`) kodda sabit |
| `47402` | Registry kayıt (enrollment): sunucu-doğrulamalı TLS + tek kullanımlık token | `registry.toml.example:4` |

Port numaraları registry/host kodunda sabit değil: `Settings.control_listen`, `enroll_listen`, `listen`, `advertised` yapılandırmadan okunur
(`registry/settings.rs:24-25`, `host/settings.rs:121-122`). İstemci, registry'yi bootstrap profilindeki adresten bulur
(`registry_control`, `registry_enroll`). Yani 47401/47402 bir **gelenek**; değiştirilirse güvenlik duvarının `firewall.ports` listesi de değişmeli. **[KOD]**
(Not: protokolün normatif metni port için ayrıca bir kural koyuyorsa bu, 04. belgede `PROTOCOL.md` okunurken karşılaştırılacak. **[BELGE]** okunmadı.)

### 4.4 Registry hangi uç noktaları kabul eder **[KOD]**

`registry.toml` içindeki `allowed_endpoints = ["10.77.10.0/24", "10.77.20.0/24"]` host'ların kendi adreslerini hangi ağlarda ilan edebileceğini sınırlar;
boş liste her adresi reddeder (`registry/settings.rs:26-27`). Host kaydında `endpoint_allowed` kontrol edilir ve dışındaki adres
"endpoint is outside the allowed networks" ile reddedilir (`registry/catalog.rs:220-221, 305-307`). Amaç: çok ağ kartlı bir bilgisayarın
VPN/internet adresini yanlışlıkla yayınlaması engellenir; `host.toml.example:1-2` aynı gerekçeyi `advertised` alanı için söylüyor. **[KOD]**

### 4.5 Güvenlik duvarı (düğüm üzerinde) **[KOD]** (`render.rs:58-75`)

`inet ilmek` adlı **ayrı bir nftables tablosu** üretilir; başka tablolara dokunulmaz.

```
set subnets { type ipv4_addr; flags interval; elements = { <alt ağlar> } }
chain input   { type filter hook input priority 0; policy accept;
                ip saddr != @subnets tcp dport { 47401, 47402 } drop }
chain forward { type filter hook forward priority 0; policy drop;
                ct state established,related accept
                ip saddr @subnets ip daddr @subnets tcp dport { 47401, 47402 } accept
                ip saddr @subnets ip daddr @subnets icmp type { echo-request, echo-reply,
                                                         destination-unreachable, time-exceeded } accept }
```

- `input`: düğümün **kendisine** gelen trafikte, alt ağlar dışından gelen İlmek TCP portları düşürülür. Politika "accept": başka hiçbir şey engellenmez.
- `forward`: düğümün **üzerinden geçen** trafik varsayılan olarak düşürülür; yalnızca alt ağlar arasında İlmek TCP portları ve temel ICMP geçer.
- Alt ağlar belirtilmezse: kullanıcı ağı + transit ağ + her rota hedefi (`config.rs:144-153`); referans plan için `10.77.10.0/24`, `10.77.254.0/30`, `10.77.20.0/24` — bu birim testle doğrulandı (`config::tests::the_reference_configuration_is_valid_with_derived_firewall_subnets`) **[ÇALIŞTIRILDI]**.
- **Dikkat:** `forward` politikası "drop" olduğundan bu düğüm İlmek dışı trafiği de iletmez. `NETWORK_SETUP.md:95-96` bunu açıkça uyarı olarak yazar. **[BELGE]**
- Kurallar yalnızca `ip` (IPv4) üzerine yazılı; IPv6 için ayrı bir kural yok ve `sysctl` yalnızca `net.ipv4.ip_forward = 1` yapar (`render.rs:77-79`). IPv6'nın nasıl davrandığı ölçülmedi. **[TEST EDİLMEDİ]**
- Düğümün güvenlik duvarı, **host makinelerinin** kendi güvenlik duvarını değiştirmez; host'a doğrudan (aynı ağdan) gelen trafiği düğüm görmez. **[YORUM]**

### 4.6 Düğümü kurmak: kısa özet (ayrıntı 31. belgede)

`ilmek network plan --config node.toml` değişiklik yapmaz; `apply --yes` (root) yapar; `rollback` geri alır. `apply`:
- Yönetim arayüzüne (varsayılan rota ya da mevcut SSH oturumu) dokunmayı varsayılan olarak reddeder, `--allow-management-interface` ile aşılır
  (`apply.rs:132-137`; `system.rs:16-17` `management_interfaces`). **[KOD]**
- `hostapd`, `wpa_supplicant`, `dnsmasq`, `nft`, `ip` eksikse `MissingPrerequisites` ile durur (`apply.rs:125-127`). **[KOD]**
- NetworkManager varsa iki arayüzü "unmanaged" yapan bir drop-in yazar (`render.rs:81-88`): NetworkManager'ın o radyolardaki işi durur. **[KOD]**
- Üretilen systemd birimleri `ilmek-hostapd-<if>`, `ilmek-wpa-<if>`, `ilmek-network`, `ilmek-firewall`, `ilmek-dnsmasq` (`render.rs:98-176`); yeniden başlatmada ağ geri gelir. **[KOD]**

## 5. Akış: A ağındaki telefon B ağındaki host'taki uygulamayı açar **[BELGE]** + **[KOD]** (kısmen)

1. Telefon `ILMEK-A`'ya bağlanır, DHCP ile `10.77.10.x` alır; ağ geçidi NODE-A (`dhcp-option=option:router`). **[KOD]** (yapılandırma üretimi); gerçek cihazda **[TEST EDİLMEDİ]**.
2. Telefon registry'yi bootstrap profilindeki `10.77.10.1:47401` adresinden çözümler (isim → host uç noktası). Registry uygulama trafiğini taşımaz.
3. Telefon doğrudan `10.77.20.10:47401`'e bağlanır. Paket NODE-A'ya gider; rota `10.77.20.0/24 via 10.77.254.2` onu transit Wi-Fi üzerinden NODE-B'ye, o da HOST-2'ye iletir (`render.rs:144-151` rota satırları). **[KOD]**
4. NODE-A'nın `forward` kuralı: kaynak ve hedef alt ağlarda, hedef port 47401 → geçer. Dönüş trafiği `established,related` ile geçer.
5. Üstteki TLS 1.3, düğümleri içerikten uzak tutar; düğüm yalnızca IP/port/boyut görür. **[BELGE]**

Adım 1–5'in tamamının tek bir gerçek ağda çalıştığına dair elimizde ölçüm **yok**.

## 6. Neden böyle? **[YORUM]**

- **Düğüm yalnızca IP düzeyinde:** uygulama mantığı düğümde yok; bu yüzden eski, zayıf bilgisayarlar düğüm olabilir ve düğüm arızası yalnızca ağlar arası bağı keser (A15 senaryosu).
- **DNS'siz:** DNS sunucusu, saldırı yüzeyi ve bağımlılık ekler; IP SAN'lı sertifikalar ve bootstrap profili aynı işi kapalı ağda daha az parçayla yapar.
- **Ayrı nftables tablosu:** mevcut kuralları bozmadan eklenir/silinir; `rollback` tek tabloyu siler.
- **Sabit 2 düğüm:** daha fazla düğüm ve dinamik yönlendirme "later work" (`NETWORK_SETUP.md:135-136`).

## 7. Güvenlik ve hata durumları

- Kablosuz parolalar yapılandırmada değil, root-only dosyada (`passphrase_file`); planda `@PASSPHRASE@` görünür, komut satırına girmez (`render.rs:7-8`). **[KOD]**
- `country` (iki büyük harf) zorunlu; olmadan AP üretilmez (`config.rs:166-168`). SSID/arayüz adı/MAC/kanal doğrulanır; üretilen dosyalara girecek değerler güvenli alfabeyle sınırlı (`config.rs:264-324`). **[KOD]**
- DHCP: aralık kullanıcı ağı içinde olmalı, düğümün kendi adresi aralıkta olamaz, rezervasyonlar aralık dışında olmalı (`config.rs:197-233`). Rotalar doğrudan bağlı ağlarla çakışamaz (`config.rs:235-257`). **[KOD]**
- Transit kaybı: NODE-B'nin radyosu kapanırsa yerel servisler çalışır, ağlar arası çağrılar hata verir (A15). **[BELGE]**, **[TEST EDİLMEDİ]**.
- Radyo/sürücü uyumsuzluğu (AP modu desteklenmiyor, iki radyo aynı anda çalışmıyor) kodla tespit edilmez; insan `iw list` ile bakmalı (`NETWORK_SETUP.md:137-138`).

## 8. Testler

`cargo test -p ilmek-network --offline` bu belge için koşturuldu **[ÇALIŞTIRILDI]**: birim 7/7 ve `tests/node.rs` 12/12 geçti. Kapsadıkları:
yapılandırma doğrulama (reddedilen bozuk değerler), alt ağ türetme, örnek `configs/*.example` dosyalarının yüklenip sırsız planlanması,
parolanın plana sızmaması, root/yönetim arayüzü reddi, eksik araç raporu, NetworkManager drop-in'i, station modunda `wpa_supplicant`,
`apply` + `rollback` (dosya kaldırma/eski içeriği geri yazma), ortada hata olunca otomatik geri alma, anlık görüntü fazları.
Bu testler **sahte makine** (geçici kök dizin + komut kaydedici) üzerinde çalışır; gerçek `ip`, `nft`, `hostapd` çalıştırmaz.

Doğrulamadıkları: gerçek Wi-Fi bağlantısı, AP modu desteği, kanal çakışması, gerçek DHCP dağıtımı, gerçek nftables kurallarının sözdizimi
(üretilen metin `nft -c` ile denenmedi **[TEST EDİLMEDİ]**), gerçek yönlendirme, telefonların bağlanması. Dolayısıyla A02–A05, A15 ve ağa bağlı diğer
kabul maddeleri **kanıtlanmış değil**; rapor durumu 36. belgede.

## 9. Sınırlar ve bilinen eksikler

- **`apply` hiçbir gerçek düğümde çalıştırılmadı.** `NETWORK_SETUP.md:3-8` ve `HARDWARE_INVENTORY.md:1-6` bunu açıkça yazıyor. **[BELGE]**
- **A01 BLOCKED:** iki düğüm, Android telefonlar ve diğer bilgisayarlar sağlanmadı. Yalnızca geliştirme makinesi (Linux Mint 22.3, MT7922 Wi-Fi, 16 CPU/32 GB) salt-okunur komutlarla incelendi; `iw` ve `hostapd` kurulu değil, dolayısıyla Wi-Fi'nin AP modu desteği **bilinmiyor**. Bu makine düğüm değildir ve `apply` onu varsayılan olarak reddeder (Wi-Fi'si varsayılan rotayı taşıyor). **[BELGE]**
- Loopback testi Wi-Fi testi değildir; tek makinedeki testler topolojiyi kanıtlamaz.
- Yalnızca iki düğüm planı; üçüncü düğüm, alternatif rota, dinamik yönlendirme yok. Yalnızca Linux + systemd. Yalnızca IPv4.
- Gerekenler (donanım envanterinden): iki düğüm için her birinde iki Wi-Fi adaptörü (yonga, sürücü, `iw list` modları ve geçerli arayüz kombinasyonları, düzenleme bölgesi), iki Android telefon, iki host + üçüncü bilgisayar (OS, mimari, MAC), radyo ortamı bilgisi.

## 10. Anlaşılma kontrolü

1. İki kullanıcı ağı ve transit ağın CIDR'leri nelerdir? Transit ağda kaç adres gerekir ve neden `/30`?
2. Düğüm DNS veriyor mu? Kodda bu nasıl sağlanıyor?
3. 47401 ve 47402 portları kodda mı sabit? Değiştirilirse başka nerede değişmeli?
4. `input` ve `forward` zincirleri neyi farklı korur? Neden `forward` politikası `drop`, `input` politikası `accept`?
5. `allowed_endpoints` ne işe yarar ve hangi hatayı önler?
6. `apply` neden yönetim arayüzünü reddeder, nasıl aşılır?
7. Hangi kabul maddeleri hâlâ kanıtlanmadı ve nedeni nedir?
8. Tek makinedeki loopback testi topoloji hakkında ne söyler?

Cevaplar: (1) `10.77.10.0/24`, `10.77.20.0/24`, `10.77.254.0/30`; iki düğüm adresi için; `validate` `prefix > 30` olanı reddeder (`config.rs:186`). (2) Hayır; `port=0` ile DNS kapalı, yalnızca DHCP (`render.rs:42`). (3) Hayır, yapılandırma; `firewall.ports` da güncellenmeli. (4) `input` düğümün kendisini, `forward` düğümden geçen trafiği korur; `forward` yalnızca İlmek trafiğini geçirir, `input`'ta ise düğümün başka servislerini (SSH vb.) kilitlememek için politika `accept`. [YORUM] (5) Host'un yanlış ağda (VPN/internet) adres ilan etmesini; `catalog.rs:305`. (6) Uzaktan bağlantıyı kopartmamak; `--allow-management-interface` (konsol erişimi yoksa kullanma). (7) A02–A05, A15 (ve diğer fiziksel ağ maddeleri): gerçek donanım sağlanmadı. (8) Hiçbir şey; yalnızca yerel TCP/TLS yolunu sınar.

## 11. Denetim notu

- Alıntılanan `dhcp` ve `firewall` şablonları `render.rs` metninden okunup özetlendi (tam birebir alıntı değil, kısaltılmış; kısaltmalar kod bloğunda `;` ile gösterildi).
- Anılan tip/fonksiyonlar `grep`/okuma ile doğrulandı: `NodeConfig`, `TransitMode`, `Firewall`, `firewall_subnets`, `render::{dhcp,firewall,sysctl,network_manager}`, `management_interfaces`, `endpoint_allowed`, `CommunityProfile.registry_{control,enroll}`.
- Satır numaraları bu revizyonda `cat -n` çıktısından alındı.
- Tutarsızlık yok: `NETWORK_SETUP.md` ile kod (DHCP-only, ayrı tablo, rota, güvenlik duvarı) uyumlu. Belge `ilmek diagnose` komutunu anıyor (`NETWORK_SETUP.md:130`); komutun varlığı 30. belgede doğrulanacak, burada kontrol edilmedi.
- Protokol belgesindeki port kuralı okunmadı; 04'te karşılaştırılacak.

## 12. Köprü

Sıradaki: [04 — Protokol genel kuralları](04-protokol-genel-kurallar.md): `PROTOCOL.md` §1–3, 6, 7, 18, 19, 21.
