> **Dikkat:** Bu belge `main` dalının (TLS, sertifika, iki node, `ilmek-identity`/`ilmek-network` crate'leri) revizyon `8da5f7b` halini anlatır. `feat/simple-no-security` dalında bu yapı değişti; güncel anlatım için [README](../README.md), [docs/ARCHITECTURE.md](../docs/ARCHITECTURE.md) ve [docs/REHBER.md](../docs/REHBER.md) okunmalıdır. Bu seri bu dal için güncellenmedi.

# 02 — Depo Yapısı ve Araç Zinciri

Revizyon: `8da5f7b` (çalışma ağacında yalnızca `CLAUDE.md` değişik ve `açıklama/` izlenmiyor).
Etiketler: **[KOD]** okunarak doğrulandı, **[ÇALIŞTIRILDI]** komut koşuldu, **[BELGE]** yalnızca belge söylüyor,
**[TEST EDİLMEDİ]**, **[YORUM]** yazarın yorumu.

## 1. Amaç

Bu belge depoyu "dışarıdan" anlatır: hangi klasör ne için var, Rust ve JavaScript tarafı nasıl bir
çalışma alanı (workspace) olarak kurulmuş, hangi araç hangi sürümde sabitlenmiş, crate'ler birbirine
hangi yönde bağlı. Okuyunca bir dosyanın nerede durması gerektiğini ve bir bağımlılık eklerken
nereye bakacağını bilirsin. Protokolün ya da kodun içine girmez; o 04'ten itibaren.

## 2. Önce bilmen gerekenler

[01 — Genel bakış](01-genel-bakis.md): roller (registry, host, istemci), İlmek'in ne olduğu.

## 3. Kavramlar

- **Workspace (Cargo):** Birden çok crate'i tek `Cargo.lock` ve tek `target/` ile derleyen küme.
- **Crate:** Rust'ın derleme birimi. `lib` (kütüphane) veya `bin` (çalıştırılabilir) olur.
- **`[workspace.dependencies]`:** Bağımlılık sürümlerinin tek yerde yazıldığı tablo; crate'ler
  `foo.workspace = true` diyerek oradan alır.
- **pnpm workspace:** JavaScript tarafında aynı fikir: tek `pnpm-lock.yaml`, paketler `packages/*` ve `apps/*`.
- **Lockfile:** Dolaylı bağımlılıklar dahil tam sürüm ve özet (hash) kaydı. Tekrarlanabilir derleme için.
- **Lint:** Derleyiciye/araçlara "bu desene izin yok" diyen kural.

## 4. Kod turu

### 4.1 Kök dizin **[KOD]**

| Yol | Ne |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Rust workspace kökü ve kilit dosyası (git'te izleniyor) |
| `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml` | Rust araç ayarları |
| `package.json`, `pnpm-workspace.yaml`, `pnpm-lock.yaml`, `.nvmrc` | JS workspace ve Node sürümü |
| `.prettierrc.json`, `.prettierignore`, `.gitignore` | Biçim/yoksayma |
| `crates/` | 13 Rust kütüphane/araç crate'i |
| `apps/client/` | Masaüstü istemci: React arayüzü + `src-tauri/` Rust kabuğu |
| `packages/sdk-js/` | `@ilmek/sdk` (JS/TS SDK) |
| `examples/` | 5 örnek uygulama (2'sinin Rust backend'i var) |
| `configs/` | `*.toml.example` örnek yapılandırmalar (host, registry, iki düğüm) |
| `deploy/systemd/` | systemd birimleri |
| `scripts/` | `e2e`, `fixtures`, `lib`, `packaging`, `smoke` betikleri |
| `tests/protocol/fixtures/` | Protokol fixture dosyaları |
| `docs/` | Belgeler: `ARCHITECTURE`, `REQUIREMENTS`, `SECURITY`, `OPERATIONS`, `IPC`, `decisions/` (ADR 0001–0004) vb. |
| `PROTOCOL.md`, `WIRE_EXAMPLES.json`, `DOCUMENT_CHECKS.md`, `README.md` | Normatif protokol, fixture, belge denetimi, operasyonel README |
| `açıklama/` | Bu seri |

`target/`, `node_modules/` ve `dist/` `.gitignore` içinde (`.gitignore:1,6-8`); `*.db`, `*.db-wal`, `*.db-shm` ve `.env` de.
Sır ya da veritabanı yanlışlıkla commit'lenmesin diye. **[KOD]**

Not: Kökte `justfile`, `deny.toml` ve `xtask/` **yok** (`ls` ile bakıldı **[ÇALIŞTIRILDI]**). Yani otomatik
mimari-bağımlılık testi ve `cargo deny` bu depoda tanımlı değil. Kontrol, aşağıdaki §4.6'daki elle
çıkarılan graf ile yapıldı.

### 4.2 Cargo workspace (`Cargo.toml`)

- `resolver = "3"` (`Cargo.toml:2`), `edition = "2024"`, `rust-version = "1.96"`, `license = "UNLICENSED"`,
  `publish = false` (`[workspace.package]`). **[KOD]**
- **16 üye** (`Cargo.toml:3-20`): 13 `crates/*`, `apps/client/src-tauri` (paket adı `ilmek-client-app`),
  `examples/forum/backend` (`ilmek-example-forum`), `examples/notes/backend` (`ilmek-example-notes`). **[KOD]**
- `[workspace.dependencies]` iki gruba ayrılır: (a) 12 dahili crate, `path = "crates/..."`; (b) dış
  bağımlılıklar, tam sürüm numarasıyla (ör. `tokio = "1.54.1"`, `rustls = "0.23.46"`, `rusqlite = "0.40.2"`
  `bundled` + `backup`, `sha2 = "0.11.1"`). **[KOD]** Her üye crate dış bağımlılığı `x.workspace = true`
  ile alıyor; yalnızca `features` ekliyor (ör. `tokio = { workspace = true, features = [...] }`). **[KOD]**
- Kullanılan temel dış crate'ler ve neye hizmet ettikleri (`description` alanları ve bağımlılık listelerinden): `rustls`/`tokio-rustls`
  (TLS), `rcgen`/`x509-parser` (sertifika), `ed25519-dalek` (yayıncı imzası), `rusqlite` (SQLite),
  `serde`/`serde_json` (JSON), `uuid` (v7), `clap` (CLI), `tauri` (masaüstü kabuğu), `proptest` (özellik testi).
  `proptest` yalnızca `ilmek-protocol` dev-bağımlılığında. **[KOD]**
- HTTP/WebSocket kütüphanesi (`axum`, `hyper`, `tungstenite`, ...) workspace tablosunda **yok**. Bu, 01'deki
  "ILMEK/1 trafiği HTTP/WebSocket değildir" kuralıyla uyumlu; ama tauri kendi içinde WebView kaynak eşlemesi için
  bağımlılık getirebilir, bu `Cargo.lock` düzeyinde incelenmedi. **[TEST EDİLMEDİ]**

### 4.3 Lint kuralları (`[workspace.lints]`, `Cargo.toml:69-86`)

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_debug_implementations = "warn"
unreachable_pub = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "warn"
panic = "deny"
todo = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
module_name_repetitions = "allow"
doc_markdown = "allow"
missing_errors_doc = "allow"
```

- 16 `Cargo.toml`'un hepsinde `[lints] workspace = true` var **[ÇALIŞTIRILDI]** (`grep -L '^workspace = true'` boş döndü).
  Yani bu kurallar her crate'e uygulanıyor.
- `unsafe_code = "forbid"` **workspace lint** olarak veriliyor; crate başında `#![forbid(unsafe_code)]` yazılı değil
  (`grep -rln 'forbid(unsafe'` 0 sonuç **[ÇALIŞTIRILDI]**). Etkisi aynı: `unsafe` yazılırsa derleme hatası. **[YORUM]**
- `unwrap`/`panic`/`todo`/`dbg!`/`println!` üretim kodunda yasak; `expect` uyarı. `clippy.toml` testlerde `unwrap`,
  `expect`, `panic` izni veriyor (`clippy.toml:1-3`). **[KOD]**
- Tek `allow` ifadesi: `crates/ilmek-host/tests/support/mod.rs:2` `#![allow(dead_code, reason = "each test binary uses a different subset")]`
  (test yardımcı modülü; her test binary'si bir alt küme kullandığı için). Bu kapsamda tek, gerekçeli istisna. **[ÇALIŞTIRILDI]** (`grep "allow("`)
- `clippy::nursery` etkin değil (yalnızca `all` + `pedantic`). **[KOD]**
- `rustfmt.toml`: yalnızca `edition = "2024"` ve `max_width = 100`. `cargo fmt --check` bu revizyonda çıkış kodu 0 verdi
  **[ÇALIŞTIRILDI]**. `clippy` bu belge için koşturulmadı. **[TEST EDİLMEDİ]**

### 4.4 Araç sürümleri

| Araç | Pin | Nerede | Makinede |
|---|---|---|---|
| Rust | `channel = "1.96.0"` + `rustfmt`, `clippy` | `rust-toolchain.toml` | `cargo 1.96.0`, `rustc 1.96.0` **[ÇALIŞTIRILDI]** |
| MSRV | `rust-version = "1.96"` | `Cargo.toml` | toolchain ile aynı |
| Node | `.nvmrc` = `24`; `engines.node = ">=24"` | `.nvmrc`, `package.json` | `v24.17.0` **[ÇALIŞTIRILDI]** |
| pnpm | `packageManager = "pnpm@12.14.0"` | `package.json` | `12.14.0` **[ÇALIŞTIRILDI]** |
| TypeScript | `6.0.3` (tam sürüm) | `apps/client`, `packages/sdk-js` | — |
| Prettier | `3.9.10` | kök `devDependencies` | — |

JS bağımlılıkları `^`/`~` olmadan tam sürümle yazılı (`"react": "19.3.1"`, `"vite": "8.3.5"`, `"vitest": "5.0.5"` vb.). **[KOD]**

**TypeScript neden 6.0:** `docs/decisions/0003-typescript-version.md`: npm'deki en yeni `typescript` 7.0.x, ama
`typescript-eslint` (zorunlu `strict-type-checked` profili için) 7.0'ı desteklemiyor; lint gevşetilmeden 6.0.3'te
kalınıyor, `typescript-eslint` 7.x'i destekleyince tek satırlık yükseltme. **[BELGE]** (ADR metni okundu; npm'in bugünkü durumu doğrulanmadı).

`pnpm-workspace.yaml`: `packages/*` ve `apps/*` ve `allowBuilds: { esbuild: true }` (esbuild'in kurulum betiğine izin). **[KOD]**
`pnpm-lock.yaml` içinde `importers` olarak `.`, `apps/client`, `packages/sdk-js` var. **[KOD]** Lockfile'lar:
`Cargo.lock` 5451 satır, `pnpm-lock.yaml` 4090 satır; ikisi de git'te izleniyor. **[ÇALIŞTIRILDI]**

### 4.5 Frontend yapılandırması **[KOD]**

- `.prettierrc.json`: `printWidth: 100`, `singleQuote: true`, `semi: false`, `trailingComma: "all"`. (Plan v2'de dosya adı `.prettierrc` yazıyordu; gerçek ad `.prettierrc.json`.)
- `.prettierignore`: `target`, `node_modules`, `dist`, `pnpm-lock.yaml`, `docs/REQUIREMENTS.md`, `tests/protocol/fixtures`, `examples/**/assets`.
  Yani özgün gereksinim metni, fixture dosyaları ve örnek uygulama varlıkları biçimlendirmeden muaf (bayt aynılığı için). **[YORUM]**
- `tsconfig.json` (ikisinde de): `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, `noImplicitOverride`, `verbatimModuleSyntax`, `isolatedModules`, `target ES2022`.
  `apps/client` ek olarak `jsx: react-jsx`, `types: [vite/client, @testing-library/jest-dom]`.
  `packages/sdk-js/tsconfig.build.json` `tsconfig.json`'u genişletip `noEmit: false`, `declaration: true`, `rootDir: src`, `outDir: dist` yapıyor.
- ESLint (flat config, her iki pakette): `js.configs.recommended` + `tseslint.configs.strictTypeChecked` + `stylisticTypeChecked`;
  kurallar: `no-explicit-any`, `no-floating-promises`, `no-misused-promises`, `consistent-type-imports` hata; `no-console` yalnızca `warn`/`error` izinli.
  `apps/client` ayrıca `jsx-a11y` ve `react-hooks` ekliyor.
- Betikler: `apps/client` → `dev` (vite), `build` (`tsc --noEmit && vite build`), `typecheck`, `test` (vitest), `lint`, `tauri`.
  `packages/sdk-js` → `build` (`tsc -p tsconfig.build.json` + esbuild ile `src/inject.ts`'ten tek dosyalı IIFE `dist/ilmek-sdk.iife.js`), `typecheck`, `test`, `lint`.
  Kökte `pnpm -r build|test|typecheck|lint` ve `prettier --write/--check .`.
- UI çatısı React 19 + Vite 8 + Tauri 2 (`@tauri-apps/api`, `@tauri-apps/cli` 2.12.4); `vite.config.ts`: port 1420, `strictPort`, test ortamı `jsdom`.
  (Bu seri için eski kök talimat dosyasındaki "Vue" varsayımı bu projede geçerli değil; kod React.) **[KOD]**

Çalıştırılmayanlar: `pnpm install/build/test/lint` bu belge için koşturulmadı. **[TEST EDİLMEDİ]** (Sonuçlar 09, 28, 29, 34 belgelerinde.)

### 4.6 Crate'ler ve bağımlılık yönü

Üretim bağımlılıkları (`[dependencies]`, yalnızca workspace içi crate'ler), `Cargo.toml` dosyalarından elle okunup
`cargo metadata --offline --no-deps` çıktısıyla karşılaştırıldı; ikisi aynı **[ÇALIŞTIRILDI]**:

| Crate | Rolü (`description`) | Dahili üretim bağımlılıkları |
|---|---|---|
| `ilmek-protocol` | Tel biçimi: frame, katı metadata, tanımlayıcılar | — (yaprak) |
| `ilmek-store` | Registry, host ve örnek backend'ler için SQLite erişimi | — (yaprak) |
| `ilmek-network` | Düğüm ağ planı, geri alma | — (yaprak) |
| `ilmek-identity` | Topluluk CA'sı, cihaz sertifikaları, yayıncı anahtarları | protocol |
| `ilmek-package` | Paket oluştur/imzala/doğrula/kopyala | identity, protocol |
| `ilmek-core` | ILMEK/1 taşıma: TLS 1.3 kimlik kontrolleri, bağlantılar | identity, protocol |
| `ilmek-app-sdk` | Host'lanan backend'ler için Rust SDK | core, protocol, store |
| `ilmek-client` | Cihaz kaydı, güvenilir bağlantı, registry/host istemcileri | core, identity, package, protocol |
| `ilmek-registry` | İsim sahipliği, kayıt, güven snapshot'ları, lease | core, identity, protocol, store |
| `ilmek-host` | Çok uygulamalı host: paket, backend gözetimi, ILMEK/1 servisi | app-sdk, client, core, identity, package, protocol, store |
| `ilmek-runtime` | İstemci çalışma zamanı: oturum, sayfa köprüsü, yerel depo | client, core, identity, package, protocol, store |
| `ilmek-cli` | `ilmek` komutu | client, core, host, identity, network, package, protocol, registry, store |
| `ilmek-testing` | Entegrasyon testi ortak yardımcıları | app-sdk, client, core, host, identity, package, protocol, registry, store |
| `ilmek-client-app` | Tauri kabuğu | client, core, protocol, runtime |
| `ilmek-example-forum`/`-notes` | Örnek backend'ler | app-sdk, store |

Gözlemler:

1. **Döngü yok;** protokol en altta. `protocol → identity → core → {registry, client, app-sdk}` ve onların üstünde `host`, `runtime`, `cli`. **[KOD]**
2. **Registry ve host birbirine üretimde bağlı değil.** `ilmek-host` yalnızca testte `ilmek-registry` kullanıyor (`dev-dependencies`);
   `ilmek-registry` yalnızca testte `ilmek-client`. Registry'ye host bağlantısı ağ üzerinden (`ilmek-client` ile). **[KOD]** [YORUM]: bu, "registry proxy değildir" kuralının bağımlılık düzeyindeki izidir.
3. **`ilmek-runtime`, `ilmek-host`'a yalnızca testte bağlı** (dev). İstemci runtime'ı host kodunu içermez. **[KOD]**
4. **`ilmek-cli` hem `registry` hem `host`'u** bağladığından tek ikili hepsini sunabiliyor (`serve` alt komutları 30. belgede). **[KOD]**
5. `ilmek-network` hiçbir İlmek crate'ine bağlı değil: ağ yapılandırma aracı protokolden bağımsız. **[KOD]**
6. `ilmek-store` yalnızca `rusqlite`, `serde`, `serde_json`, `sha2`, `hex`, `thiserror`, `tokio (rt)` kullanıyor; `ilmek-protocol` sadece `serde`, `serde_json`, `sha2`, `base64`, `uuid`, `thiserror` (`Cargo.toml` dosyaları). **[KOD]**
7. `ilmek-testing`, üretim bağımlılığı olarak neredeyse her şeyi çekiyor; yalnızca test yardımcısı olarak kullanılmalı. Hiçbir üretim crate'i bunu `[dependencies]` altında listelemiyor (yukarıdaki tabloda yok). **[KOD]**

[YORUM] Bu yapı, CLAUDE.md'deki "protokol/kimlik/registry/host/SDK/UI sorumlulukları ayrı" kuralını crate seviyesinde karşılıyor.
Ancak bunu CI'da zorlayan bir mimari test **yok**; yalnızca `Cargo.toml` düzeni koruyor.

Kaynak dosya sayısı: `crates/`, `apps/client/src-tauri/` ve `examples/` altında 182 `.rs` dosyası (`find`, `target`/`node_modules` hariç) **[ÇALIŞTIRILDI]**.

### 4.7 Crate içi dizilim (kısa harita) **[KOD]**

`ls` çıktısından; ayrıntı ilgili belgelerde:

- `ilmek-protocol/src`: `address, digest, frame, ids, ipc, lib, manifest, message, strict_json, wire(.rs + dizin)`; `tests/`: `address, fixtures, framing, ids, ipc, manifest, strict_json, wire`.
- `ilmek-identity/src`: `pki, profile, publisher, storage, lib`. `ilmek-core/src`: `clock, listener, peer, session(.rs + dizin), tls(.rs + dizin), transport, trust`.
- `ilmek-store/src`: `backup, migrate, unsigned, lib`. `ilmek-registry/src`: `admin, catalog, enroll, files, gate, init, limiter, operations, peers, registry, schema, server, service, settings, snapshots, time_util, trust_api, error, lib`.
- `ilmek-host/src`: 18 modül (`host, manager, install, launcher, supervisor, service, link, subscriptions, registry_link, publish, slots, ...`); `tests/` içinde `e2e, example_config, install, resilience, support/`.
- `ilmek-client/src`: `app, connector, device, download, enroll, error, registry_client, resolver, retry, lib`; **tests dizini yok**: bu crate'in davranışı diğer crate'lerin testleri üzerinden (ör. `ilmek-registry/tests`, `ilmek-host/tests`) doğrulanıyor. **[KOD]** (kapsam değerlendirmesi 34. belgede)
- `tests/` dizini olmayan crate'ler: `ilmek-client`, `ilmek-testing` (yardımcı crate). `ilmek-app-sdk` yalnızca `tests/sdk.rs` içeriyor.
- `apps/client`: `src/{app,features,shared}`, `test/{setup.ts,ui.test.tsx}`, `src-tauri/{src,capabilities,permissions,tauri.conf.json,build.rs,icons,gen}`.
- `packages/sdk-js`: `src/{index,inject,sdk,types,uuid7}.ts`, `test/{inject,sdk,uuid7}.test.ts`.
- `examples/{announcements,forum,hello-html,isolation-probe,notes}/app`; `forum` ve `notes` ayrıca `backend/` (Rust).

## 5. Akış: yeni bir bağımlılık eklemek **[YORUM]** (depodaki düzenden çıkarım)

1. Rust: sürümü kök `Cargo.toml` `[workspace.dependencies]` altına tam numarayla yaz; crate'te `x.workspace = true`; gerekiyorsa `features`. `Cargo.lock` güncellenir ve commit'lenir.
2. JS: ilgili `package.json`'a tam sürüm yaz, `pnpm install`, `pnpm-lock.yaml` commit'lenir.
3. Yeni iç crate: `members` ve `[workspace.dependencies]`'e ekle, `[lints] workspace = true` koy.

## 6. Neden böyle? **[YORUM]**

- **Tek sürüm tablosu:** crate'ler arası sürüm kayması olmaz; yükseltme tek satır.
- **Tam sürüm sabitleme:** "güncel stable, pinli" kuralı (kök talimat); tekrarlanabilir kurulum. Bedel: güncelleme elle.
- **Lint'in workspace düzeyinde olması:** her crate aynı katılıkta; yeni crate `[lints] workspace = true` demeyi unutursa kural dışı kalır (şu an unutan yok).
- **Registry/host ayrı crate:** aynı ikili (`ilmek serve`) altında birleşebilir ama kod bağımlılığı olmadan; mikro-servis benzeri ayrılabilirlik.

## 7. Güvenlik ve hata durumları

- `unsafe` yasağı derleme zamanında (workspace lint). **[KOD]**
- `.gitignore` sır dosyası (`.env`) ve veritabanlarını dışlıyor; ancak sertifika/anahtar dosyaları için ayrı desen yok. Örnek anahtarlar nerede tutuluyor sorusu 10. belgede. **[KOD]**
- Bağımlılık güvenliği (`cargo audit`/`cargo deny`/`pnpm audit`) için depoda yapılandırma görmedim; çalıştırılmadı. **[TEST EDİLMEDİ]**

## 8. Testler

Bu belge yalnızca yapılandırmayı anlatıyor. Doğrulama: `cargo fmt --check` (çıkış 0), `cargo metadata --offline --no-deps` (graf), `grep`/`ls` (yukarıda işaretli). `cargo build/test`, `clippy`, `pnpm` komutları koşturulmadı.

## 9. Sınırlar ve bilinen eksikler

- Mimari bağımlılık testi, `cargo deny`, `just` giriş noktası yok (kök talimat bunları başka bir projeden devralmış olabilir; bu depoda gerek duyulmuş ama yazılmamış).
- Tam `cargo metadata` (bağımlılık çözümü dahil) çevrimdışı çalışmadı (çıktı JSON değildi); yalnızca `--no-deps` kullanıldı. Dış bağımlılıkların tam ağacı incelenmedi.
- `ilmek-client` için `tests/` yok.

## 10. Anlaşılma kontrolü

1. Cargo workspace'te sürüm numarası nerede, bir crate'te nasıl çağrılır?
2. Hangi iki crate hiçbir iç crate'e bağlı değil (üretimde)? (Üç: `protocol`, `store`, `network`.)
3. `ilmek-host` ile `ilmek-registry` arasında üretim bağımlılığı var mı? Test bağımlılığı?
4. `unsafe` yasağı nerede ve nasıl uygulanıyor?
5. TypeScript neden 7 değil 6.0.3?
6. `.prettierignore` neden `tests/protocol/fixtures` ve `docs/REQUIREMENTS.md`'yi içeriyor?
7. Hangi bileşen TypeScript SDK'yı tek dosyalı IIFE'ye paketler ve hangi araçla?

Cevaplar: (1) kök `Cargo.toml` `[workspace.dependencies]`; crate'te `x.workspace = true`. (2) `ilmek-protocol`, `ilmek-store`, `ilmek-network` (üç tane). (3) Üretimde hayır; `ilmek-host` dev-bağımlılığı olarak `ilmek-registry` var. (4) `[workspace.lints.rust] unsafe_code = "forbid"` + her crate `[lints] workspace = true`. (5) `typescript-eslint` 7.0'ı desteklemiyor (ADR 0003). (6) Bayt-bayt korunması gereken özgün/fixture içerik biçimlendirilmesin diye **[YORUM]**. (7) `packages/sdk-js` `build` betiği, `esbuild`, çıktı `dist/ilmek-sdk.iife.js`.

## 11. Denetim notu

- Alıntılanan lint bloğu `Cargo.toml` metniyle karşılaştırıldı; satır aralıkları (`69-86` lint, `3-20` üyeler) `grep -n` ile teyit edildi. `.gitignore` satırları `cat -n` ile okundu.
- Bağımlılık tablosu iki bağımsız yoldan (elle okuma + `cargo metadata --no-deps`) doğrulandı.
- "5 örnek, 16 üye, 13 crate, 182 `.rs`" sayıları bu revizyonda sayıldı.
- Tutarsızlıklar: (a) plan v2 `.prettierrc` diyor, gerçek `.prettierrc.json`; (b) plan v2 "crate'ler `clippy.toml`/`rustfmt.toml`" derken `nursery` yok; (c) kök talimat dosyasındaki Vue/axum/PostgreSQL varsayımları bu projede geçersiz (kod: React, SQLite, TLS üzerinden özel protokol).

## 12. Köprü

Sıradaki: [03 — Ağ topolojisi](03-ag-topolojisi.md): fiziksel ve mantıksal ağ, portlar, düğümler, test edilmeyenler.
