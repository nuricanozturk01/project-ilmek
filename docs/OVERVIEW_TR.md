# İlmek — Türkçe özet

Bu özet `feat/simple-no-security` dalının gerçek davranışını anlatır. Ayrıntılar İngilizce belgelerdedir:
[README](../README.md), [GETTING_STARTED](GETTING_STARTED.md), [ARCHITECTURE](ARCHITECTURE.md),
[PROTOCOL](../PROTOCOL.md).

> **Bu dalda güvenlik yoktur.** Trafik şifrelenmez, kimse kimliğini kanıtlamaz, paketler imzalanmaz
> (yalnızca SHA-256 ile bozulma denetlenir). Ağa bağlanan herkes katılıp uygulama yayınlayabilir.
> Yalnızca güvendiğiniz, kapalı bir ağda kullanın. Güvenli tasarım `main` dalındadır.

## Ne işe yarar?

Kendi küçük "internetinizi" kurarsınız. Bir makine **node** olur: içinde bir **kayıt sunucusu**
(registry: hangi uygulama nerede?) ve bir **uygulama sunucusu** (host) çalışır. Uygulamalarınızı oraya
yayınlarsınız. Aynı ağdaki diğer bilgisayarlar node'u kendiliğinden bulur ve uygulamaları
`ilmek://isim/uygulama` adresiyle açar. İnternet, bulut veya HTTP gerekmez; her şey İlmek'in kendi
protokolü (**ILMEK/1**) ile doğrudan TCP üzerinden konuşur.

## Terimler

| Terim | Anlamı |
| --- | --- |
| **Node** | Kayıt sunucusunu (ve genellikle bir uygulama sunucusunu) çalıştıran makine: `ilmek node` |
| **Kayıt sunucusu (registry)** | "nuri/forum şu makinede, şu sürümde" bilgisini tutar; uygulama trafiğini taşımaz |
| **Uygulama sunucusu (host)** | Uygulama paketlerini ve arka uç süreçlerini çalıştırır; birden çok uygulama taşır |
| **Arka uç (backend)** | Bir uygulamanın veriyi tuttuğu ayrı süreç. Statik uygulamaların arka ucu olmaz |
| **İstemci** | Uygulamayı açan cihaz: `ilmek` komutu veya masaüstü penceresi |
| **Ad alanı (namespace)** | Adresteki ilk bölüm: `ilmek://`**`nuri`**`/forum`. İlk yayınlayan cihaz sahiplenir |
| **Cihaz kimliği** | Cihazın kendi ürettiği rastgele numara; kanıtlanmaz, yalnızca cihazları ayırır |

## Hızlı başlangıç

Linux makinede (node), proje klasöründe:

```sh
sudo ufw allow 47400/udp && sudo ufw allow 47401:47402/tcp   # yalnızca güvenlik duvarı açıksa
scripts/quickstart.sh                                        # derler, node'u başlatır, örnekleri yayınlar
```

Aynı ağdaki başka bir makinede:

```sh
cargo build -p ilmek-cli
target/debug/ilmek join                                      # node'u kendisi bulur
target/debug/ilmek call ilmek://nuri/forum forum.topics.list
```

Bulamazsa yalnızca node'un IP adresini verin: `ilmek join --registry NODE_IP:47401`. Masaüstü pencere
ve platforma göre kurulum: [INSTALL](INSTALL.md).

## Veriler nerede?

`$ILMEK_HOME` (varsayılan `~/.ilmek`) altında: `registry/` (kayıt veritabanı), `host/` (paketler,
uygulama verileri, günlükler), `device/` (bu makinenin istemci kimliği), `registry.toml` ve
`host.toml` (ayarlar). Yedekleme: `ilmek backup …` ([BACKUP_AND_OPERATIONS](BACKUP_AND_OPERATIONS.md)).

## Uygulama yazmak

HTML/CSS/JavaScript sayfası ve isteğe bağlı bir arka uç (Rust veya Spring Boot). Sayfa yalnızca
`window.ilmek` SDK'sı üzerinden kendi arka ucuna konuşur; başka bir adrese bağlanamaz. Yazma işlemleri
işlem kimliği taşır: yanıt kaybolsa bile yeniden gönderilen işlem iki kez uygulanmaz.
[APP_DEVELOPMENT_GUIDE](APP_DEVELOPMENT_GUIDE.md), [JAVA_ANGULAR](JAVA_ANGULAR.md).

## Ne test edildi?

Tek bir Linux makinede, node, host, arka uç ve istemci **ayrı süreçler** olarak `127.0.0.1` üzerinde:
UDP keşif, otomatik katılma, yayınlama, indirme, çağırma, yeniden başlatmada veri, ikinci host,
yedek/geri yükleme. **Yapılmadı:** ikinci gerçek bilgisayar, gerçek ağda UDP yayın keşfi, macOS,
Windows, Android. Ayrıntı: [TEST_REPORT](TEST_REPORT.md).

## Sık sorulanlar

**`ilmek join` "no node found" diyor.** Node çalışıyor mu? Güvenlik duvarında 47400/udp ve
47401-47402/tcp açık mı? Aynı alt ağda mısınız? Değilse `--registry IP:47401` kullanın.

**İki Wi-Fi kartı gerekiyor mu?** Hayır. İki makinenin birbirine IP ile ulaşabilmesi yeter.

**Neden şifreleme yok?** İlk sürümün en basit biçimde çalışması için bilinçli olarak çıkarıldı
([karar 0005](decisions/0005-simple-no-security.md)); sonra protokolün yeni bir sürümüyle eklenecek.
