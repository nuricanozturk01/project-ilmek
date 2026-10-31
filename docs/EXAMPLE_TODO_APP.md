# Todo uygulaması

Bu uygulama İlmek ağı üzerinde çalışan örnek bir todo uygulamasıdır. Backend Spring Boot ile, frontend Angular ile yazılmıştır.

Kod konumu: `examples/tasks`

## 1. Backend neden controller kullanmıyor?

İlmek'te backend HTTP sunucusu değildir. Host, backend'i loopback üzerinden kendi protokolüyle (ILM1) çağırır. Bu yüzden `@RestController` yazılsa bile host onu çağırmaz.

Bunun yerine iş metotları `@IlmekMethod` ile işaretlenir. Örnek: `tasks.list`, `tasks.create`.

Sonuçları:

- Uygulama HTTP dinlemez, ağda ekstra bir kapı açılmaz.
- Doğrulama elle yapılır (ör. başlık 1–200 karakter). Hatalar `AppException` ve `ErrorCode` ile döner.
- Projeye `spring-boot-starter-web` eklenirse Tomcat 8080 portunda açılır. Bu, İlmek dışında bir HTTP kapısı demektir. Bu bağımlılık eklenmemelidir.
- Test için HTTP yerine şu komut kullanılır:
  `ilmek call ilmek://<ref> <metot> --body '{...}'`

## 2. Backend metotları

| Metot | Tür | Gövde | Yanıt |
|---|---|---|---|
| `tasks.list` | READ | `{}` | `{"tasks":[{id,title,done}]}` |
| `tasks.create` | WRITE | `{"title"}` | `{"task":{...}}` |
| `tasks.complete` | WRITE | `{"id","done"?}` | `{"task":{...}}` (done yoksa true) |
| `tasks.update` | WRITE | `{"id","title"}` | `{"task":{...}}` |
| `tasks.delete` | WRITE | `{"id"}` | `{"deleted":id}` |
| `tasks.clear_completed` | WRITE | `{}` | `{"removed":n}` |

- Her yanıt bir JSON nesnesidir. Host, dizi gövdeyi kabul etmez.
- Her değişiklik `tasks.events` konusuna düz bir nesne olarak yazılır: `{"type","id",...}`.
- Veri H2 dosyasında tutulur (`./data/tasks`). `WRITE_DELAY=0` ayarı sayesinde her commit diske yazılır; süreç aniden kapansa bile kayıt kaybolmaz.

## 3. Frontend

- Kod: `examples/tasks/frontend-angular`
- Angular sürümü: 22.2.4 (sabit)
- Özellikler: kalan sayısı, All / Active / Completed filtresi, ekleme, düzenleme (Enter kaydeder, Esc iptal eder), silme, tamamlananları temizleme, canlı yenileme, hata satırı.
- Sadece `window.ilmek.call` ve `window.ilmek.subscribe` kullanılır.
- Kısıtlar: service worker yok, dış URL yok, kritik CSS inline değil (CSP gereği).

### Neden `http://localhost:4200/` açılıyor?

`npm start` komutu Angular'ın geliştirme sunucusunu (`ng serve`) başlatır. Varsayılan adres `localhost:4200`'dür.

Bu sayfa İlmek dışında, sıradan bir web sayfası olarak çalışır. `window.ilmek` nesnesi yoktur, backend'e ulaşamaz. Geliştirme sunucusu yalnızca arayüz geliştirmek için kullanılır.

## 4. Doğru çalıştırma (İlmek üzerinde)

```sh
cd examples/tasks/frontend-angular && npm ci && npm run build
cd ../backend-java && ./gradlew bootJar
scripts/packaging/pack-angular-app.sh examples/tasks/frontend-angular examples/tasks <paket-klasörü>
ilmek app install <paket-klasörü>
ilmek app set-backend nuri/tasks --exe "$(command -v java)" --arg=-jar --arg=<jar-yolu>
ilmek app start nuri/tasks
ilmek app publish nuri/tasks
```

Sonra masaüstü istemcide `ilmek://nuri/tasks` adresini aç. Node ve istemci kurulumu için `docs/RUN_NODES_AND_CLIENTS.md` dosyasına bak.

Not: `pack-angular-app.sh` gerçek `ng` ile uçtan uca çalıştırılmadı. Stub `ng` ve gerçek `ilmek app pack` ile doğrulandı.

## 5. Desteklenen sürümler

Starter ve örnek backend Java 17 hedefiyle derlenir (`--release 17`). Spring Boot sürümü `-PbootVersion` ile seçilir; verilmezse 4.1.2 kullanılır.

Her hücre iki komutla çalıştırıldı: starter testleri ve örnek backend testleri. Sonuçlar (Linux x86_64, Gradle 9.4.1, kod sürümü `9d6ab48`):

| JDK | Boot 3.5.16 | Boot 4.1.2 |
|---|---|---|
| 17 (Temurin 17.0.20) | starter 131, örnek 22: geçti | starter 131, örnek 22: geçti |
| 21 (GraalVM 21.0.7) | starter 131, örnek 22: geçti | starter 131, örnek 22: geçti |
| 25 (GraalVM CE 25.4) | starter 131, örnek 22: geçti | starter 131, örnek 22: geçti |

Her hücrede 0 başarısız, 0 atlanan test vardı. JDK 21, Gradle'ın otomatik indirmesiyle değil, makinede zaten kurulu olan `~/.jdks/graalvm-jdk-21.0.7` ile denendi.

Bir hücreyi çalıştırmak için (`JDK_YOLU` ve `SURUM` değiştirilir):

```sh
cd java/ilmek-spring-boot-starter
./gradlew -PbootVersion=SURUM -Dorg.gradle.java.home=JDK_YOLU :starter:test --rerun-tasks

cd ../../examples/tasks/backend-java
./gradlew -PbootVersion=SURUM -Dorg.gradle.java.home=JDK_YOLU test --rerun-tasks
```

`SURUM` olarak `3.5.16` veya `4.1.2` yazılır. Varsayılan bileşimin (Boot 4.1.2, JDK 25) jar'ı `./gradlew bootJar` ile üretilip gerçek host ile denendi: `cargo test -p ilmek-testing --test java_backend_e2e -- --nocapture` sonucu `6 passed; 0 failed`.

Örnek backend'de yapılan değişiklikler:
- `extra["jackson-bom.version"] = "2.22.4"` kaldırıldı. Boot 4'te bu özellik Jackson 3 BOM'unu belirler; 2.22.4 verilirse yanlış sürüm seçilir. Yerine `jackson-bom:2.22.4` platformu doğrudan eklendi, iki Boot hattında da aynı çalışır. `io.spring.dependency-management` eklentisi de kaldırıldı; Spring BOM'u Gradle platformu olarak ekleniyor.
- `@EntityScan` yerine `@AutoConfigurationPackage` kullanıldı, çünkü `@EntityScan` Boot 4'te başka pakete taşındı.

Test sırasında bulunan bir hata düzeltildi: bir olay onayına (`EventAck`) verilen yanıt, pompanın sıradaki olayından sonra kuyruğa girebiliyordu. `boundaryCursorWindowsEventsUntilAcknowledged` testi bu yüzden Boot 4.1.2 ile JDK 17 ve 25'te arada bir başarısız oldu (ilk çalıştırmada 2 hücrede). Yanıt artık pompa uyandırılmadan önce kuyruğa alınıyor; düzeltmeden sonra 12 hücre ve ek 3 tekrar geçti.

Angular: frontend yalnızca `window.ilmek` ve statik dosyaları kullanır. Aynı kaynak, repo dışındaki geçici bir klasörde Angular 22.2.4 (repodaki sürüm) ve 21.2 (`@angular/cli` 21.2.29, çekirdek paketler 21.2.25, TypeScript 5.9) ile `ng build --configuration production` ile derlendi. İkisi de `dist/frontend-angular/browser` çıktısı verdi ve `scripts/packaging/pack-angular-app.sh` iki çıktıyı da kabul edip paketledi. Angular 20 denenmedi.

## 6. Doğrulama durumu

| Kontrol | Sonuç |
|---|---|
| Starter testleri | 131 geçti |
| Todo backend testleri | 22 geçti, bootJar üretiliyor |
| Frontend testleri | 23 geçti, build temiz |
| Uçtan uca test (gerçek host + jar) | 6/6 geçti (HEAD `936e688`) |
| İstemcide stil görünümü | Yapılmadı (engellendi) |
| Java 17/21/25 x Spring Boot 3.5.16/4.1.2 | 12 hücre geçti (bölüm 5) |
