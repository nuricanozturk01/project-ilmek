# Var olan Spring + Angular uygulamasını İlmek'e taşımak

Kısa cevap: sıradan bir Spring MVC REST + Angular `HttpClient` uygulaması, kodu değişmeden İlmek üzerinde çalışır. Sadece bağımlılık ve bir satır ayar eklenir. Kanıt: `examples/rest-notes`.

## Ne eklenir, ne değişmez

| | Değişmez | Eklenir |
|---|---|---|
| Backend | `@RestController`, bean validation, JPA, repository | Gradle: `dev.ilmek:starter` ve `dev.ilmek:ilmek-spring-web-adapter`; `@AutoConfigurationPackage` satırı |
| Frontend | Bileşenler, `HttpClient` çağrıları (`/api/...`) | `provideIlmekHttp()` ve `@ilmek/angular-http` bağımlılığı |
| Paket | | `app/app.toml` (`ilmek app init` ile üret) |

## Nasıl çalışır

Angular'daki `/api/...` çağrısı `window.ilmek.call("http.read" | "http.write", ...)` olur. Backend'de adaptör bu isteği uygulamanın kendi `DispatcherServlet`'ine (MockMvc ile) verir ve yanıtı geri döndürür. `GET/HEAD/OPTIONS` okuma, `POST/PUT/PATCH/DELETE` yazma sayılır. Yazmalar aynı işlem kimliğiyle tekrar gelirse kontrolcü ikinci kez çalışmaz, ilk sonuç döner.

Adaptör HTTP portunu kapatır (`server.port=-1`): uygulama ağda hiçbir portu dinlemez. Bunu `examples/rest-notes` e2e testi doğruladı (jar'ın hiçbir TCP portu açık değil).

## Adımlar

1. Backend `build.gradle.kts`: starter ve adaptör bağımlılıklarını ekle. Starter'ı `includeBuild` ile kullanıyorsan `settings.gradle.kts` içinde adaptör için `dependencySubstitution` gerekir (bkz. `java/ilmek-spring-boot-starter/web-adapter/README.md`).
2. Ana sınıfa `@AutoConfigurationPackage(basePackages = {"<uygulama.paketi>", "dev.ilmek.sdk.store"})` ekle. Uygulamanın kendi paketini listede tut, yoksa kendi varlıkların bulunmaz.
3. Frontend: `@ilmek/angular-http` bağımlılığını ekle, `app.config.ts` içinde `provideIlmekHttp()` kullan. Uygulamada `.npmrc` içine `install-links=true` yaz (aksi halde iki Angular kopyası oluşur ve interceptor çalışmaz).
4. Derle, paketle, yayınla: `docs/DOCUMENTATION_MAP.md` bölüm 4.

## Çalışmayanlar

- Spring Security (giriş, oturum, CSRF): desteklenmiyor; başlangıçta uyarı yazılır.
- WebFlux (reaktif): desteklenmiyor, adaptör devreye girmez.
- Dosya yükleme (multipart, FormData, Blob), ikili (binary) yanıtlar: desteklenmiyor.
- WebSocket / SSE: yok, bunun yerine İlmek `subscribe` olayları kullanılır (kod değişikliği gerekir).
- Dış adresler (CDN, Google Fonts, dış API): sayfa tarafında engelli.
- Cookie ve `Authorization` başlıkları iletilmez.
- Yük boyutu: istek veya yanıt 200 KiB üstündeyse 413 döner.
- `X-Ilmek-Principal` başlığı cihaz kimliğidir, doğrulanmış kullanıcı değildir; güvenlik kararı için kullanma.

## Bilinen tuzaklar

- Kontrolcü beklenmedik hata fırlatırsa (500), bu sonuç da saklanır: aynı işlem kimliğiyle tekrar gelen yazma kontrolcüyü yeniden çalıştırmaz, 500'ü tekrar döndürür.
- Kontrolcünün kendi işlemleri (transaction), starter'ın işlemine yalnızca aynı transaction manager ile katılır; bu belgelendi, test edilmedi.
- Jar İlmek dışında başlatılınca HTTP açmaz (`server.port=-1`); elle `--server.port=...` verilirse açılır.

## Doğrulama durumu

| Kontrol | Sonuç |
|---|---|
| `examples/rest-notes` backend testleri (Boot 4.1.2 ve 3.5.16) | 11 + 11 geçti |
| Adaptör testleri (Java 17/21/25 × Boot 3.5/4.1) | 33, altı hücrede geçti |
| Angular testleri (gerçek `provideIlmekHttp` + sahte `window.ilmek`) | 11 geçti |
| Gerçek host + jar e2e (`rest_notes_e2e`) | 3/3 geçti |
| `ilmek app install/start/publish` ile node'a kurulum | Çalıştırılmadı |
| Masaüstü istemcide görünüm | Çalıştırılmadı |
