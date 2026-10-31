# Dokümantasyon haritası (Türkçe)

Bu belge iki şeyi anlatır: hangi araçları kullanacağın ve hangi sorunun cevabının hangi dokümanda olduğu.

## 1. Sürüm durumu

Desteklenen aralık (hepsi çalıştırıldı, ayrıntı `docs/EXAMPLE_TODO_APP.md` bölüm 5):

| Bileşen | Desteklenen | Varsayılan | Not |
|---|---|---|---|
| Java | 17, 21, 25 | derleme hedefi 17 (`--release 17`) | 17 taban sürümdür |
| Spring Boot | 3.5.x (3.5.16 denendi), 4.1.x (4.1.2 denendi) | 4.1.2 | `-PbootVersion=...` ile seçilir |
| Angular | 22.2.4 (repodaki sürüm), 21.2.x (denendi) | 22.2.4 | Frontend Angular sürümüne bağlı değil; yalnızca `window.ilmek` ve statik dosyaları kullanır |

Spring Boot 4.2.0 yalnızca RC olduğu için hedeflenmedi. Starter kendi Jackson 2 sürümünü (2.22.4) kullanır; Boot 4'ün Jackson 3'ü ile çakışmaz (testle doğrulandı).

## 2. Ne kullanırsın?

**Backend (Java)**
- Dil: Java 17, 21 veya 25
- Çerçeve: Spring Boot 3.5.x veya 4.1.x (varsayılan 4.1.2)
- İlmek bağlantısı: `dev.ilmek:starter` (`java/ilmek-spring-boot-starter`)
- İş metotları: `@IlmekMethod(name, kind)` ile işaretlenir (READ veya WRITE). Controller kullanılmaz.
- Veri: JPA + H2 (örnek); kendi veritabanını da kullanabilirsin.
- Örnek: `examples/tasks/backend-java`

**Frontend (Angular)**
- Angular 22.2.4 (repodaki sürüm); 21.2 ile de derleniyor, paketleme betiği iki çıktıyı da kabul ediyor.
- İlmek bağlantısı: sayfada hazır gelen `window.ilmek` nesnesi (`call`, `subscribe`).
- Kısıtlar: service worker yok, dış URL yok, kritik CSS inline değil.
- Örnek: `examples/tasks/frontend-angular`

**Ağ ve çalıştırma**
- `ilmek` komutu (Rust): node, host, istemci.
- Masaüstü istemci: `apps/client` (Tauri).

## 3. Ne zaman neye bakmalıyım?

| Sorum | Bak |
|---|---|
| Nasıl başlatırım? (node, join) | `docs/RUN_NODES_AND_CLIENTS.md` |
| İlk demo, adım adım (Linux node + Mac istemci) | `docs/TWO_MACHINE_DEMO.md` |
| İşletim sistemine göre kurulum | `docs/INSTALL_BY_PLATFORM.md` |
| Birden çok host / ağ kurulumu | `docs/NETWORK_AND_DISCOVERY.md` |
| Var olan Spring + Angular uygulamasını taşımak (controller'lar değişmeden) | `docs/MIGRATE_EXISTING_SPRING_ANGULAR_APP.md` |
| Yeni uygulama ekleme ve yayınlama (hızlı başlangıç) | `docs/APP_DEVELOPMENT_GUIDE.md` ("Quick start") |
| Uygulamayı güncelleme, geri alma, silme | `docs/APP_DEVELOPMENT_GUIDE.md` ("Updating, rolling back, removing") |
| Yeni statik uygulama, manifest, `ilmek app` komutları | `docs/APP_DEVELOPMENT_GUIDE.md` |
| Java backend nasıl yazılır (starter API) | `java/ilmek-spring-boot-starter/README.md` |
| Örnek todo uygulaması, Java+Angular nasıl paketlenir | `docs/EXAMPLE_TODO_APP.md` |
| Rust backend ↔ host arasındaki IPC | `docs/BACKEND_IPC.md` |
| Protokol kuralları (kesin, bayt düzeyi) | `PROTOCOL.md` |
| Güvenlik sınırları, neyin korunmadığı | `docs/SECURITY.md` |
| Yedek alma, yükseltme, teşhis | `docs/BACKUP_AND_OPERATIONS.md` |
| Ne test edildi, ne edilmedi | `docs/TEST_REPORT.md` |
| Mimari ve bileşenler | `docs/ARCHITECTURE.md` |
| Ürün gereksinimleri (özgün şartname) | `docs/REQUIREMENTS.md` |
| Neden böyle karar verildi | `docs/decisions/` |
| Kaldığımız yer, açık işler | `docs/PROJECT_STATUS.md` |
| Türkçe baştan anlatım | `docs/OVERVIEW_TR.md` |

## 4. Yeni uygulama yayınlama (özet)

Ayrıntı: `docs/APP_DEVELOPMENT_GUIDE.md`. Aşağıdaki örnek Java + Angular içindir; örnek dosyalar `examples/tasks`.

1. Klasör düzeni:
   ```
   benim-uygulamam/
     app/app.toml          # manifest (aşağıya bak)
     frontend-angular/     # Angular projesi
     backend-java/         # Spring Boot projesi
   ```
2. Manifest üret: `ilmek app init --namespace <ns> --slug <ad> /tmp/iskele`, çıkan `app.toml` dosyasını `app/app.toml` olarak kopyala. `app_id` ve `publisher_id` değişmez, silme.
   Backend varsa `backend_required = true` yap; sayfa `window.ilmek.call` kullanıyorsa `permissions` içinde `app.call` olmalı (canlı güncelleme için `app.subscribe` da).
3. Frontend'i derle: `cd frontend-angular && npm ci && npm run build`
4. Backend'i derle: `cd backend-java && ./gradlew bootJar`
5. Paketle: `scripts/packaging/pack-angular-app.sh frontend-angular app /tmp/paket` (`/tmp/paket` önceden var olmamalı; `ilmek` komutu PATH'te olmalı)
6. Node çalışırken yayınla:
   ```sh
   ilmek app install /tmp/paket
   ilmek app set-backend <ns>/<ad> --exe "$(command -v java)" --arg=-jar --arg=<jar-yolu>
   ilmek app start <ns>/<ad>
   ilmek app publish <ns>/<ad>
   ```
7. İstemcide `ilmek://<ns>/<ad>` aç.

Yeni sürüm yayınlamak için: `app.toml` içindeki `version` değerini artır, paketle, `ilmek app update /tmp/paket2`, sonra `ilmek app publish <ns>/<ad>`.

Notlar:
- Namespace ilk gelene aittir; aynı namespace'e başka cihaz yayınlayamaz.
- Backend'de HTTP controller kullanma; yöntemler `@IlmekMethod` ile tanımlanır (bkz. `docs/EXAMPLE_TODO_APP.md`).
- `pack-angular-app.sh` gerçek `ng` ile uçtan uca çalıştırılmadı; stub ile ve gerçek `ilmek app pack` ile doğrulandı.

## 5. Doğrulama durumu (kısa)

- Starter: 131 test geçti.
- Todo backend: 22 test geçti, jar üretiliyor.
- Frontend: 23 test geçti, build temiz.
- Uçtan uca (gerçek host + jar): 6/6 geçti.
- İstemcide stil görünümü: yapılmadı.
- Java 17/21/25 x Spring Boot 3.5.16/4.1.2: 12 hücrenin hepsi geçti (`docs/EXAMPLE_TODO_APP.md` bölüm 5).
