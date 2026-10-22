# Doküman doğrulama kaydı

Tarih: 22 Ekim 2026. Bu kayıt yalnızca teslim edilen tasarım belgeleri ve byte fixture'ları içindir.

- README'de 34, PROTOCOL.md'de 22 numaralı bölüm kontrol edildi.
- Markdown kod blokları ve tablo sütun sayıları kontrol edildi.
- TCP/TLS, portlar, frame işareti, metadata/body sınırları ve çoklu uygulama kararları eşleştirildi.
- 12 geçerli frame örneği encode/decode round-trip ile kontrol edildi; UTF-8 byte boyutları ve big-endian prefix değerleri doğrulandı.
- 5 bozuk frame örneğinin ilgili decoder kontrolü tarafından reddedildiği kontrol edildi.
- UUIDv7 örneklerinin version/variant bitleri kontrol edildi; hash ve imzalar yeni kimliklerle yeniden üretildi.
- APP_CALL hash preimage'i ve manifest hash'i yeniden hesaplandı.
- Manifest test imzası Ed25519 ile oluşturuldu ve doğrulandı. JSON'daki seed açık test verisidir; üretimde kullanılmaz.
- 24 ürün kabul testi ve 24 protokol uyumluluk kontrolü belgede bulunuyor.

## Henüz yapılmayanlar

Ürünün Rust/Tauri/Android kodu bu görevde geliştirilmedi. Gerçek Wi-Fi bağlantısı, TLS/mTLS oturumu, bütün protokol state machine'i, platform WebView izolasyonu, routing, yük testi ve güvenlik denetimi yapılmadı. Yukarıdaki fixture kontrolleri bu testlerin yerine geçmez.

Belge ayrıntılı bir uygulama sözleşmesidir; kusursuzluk veya donanım uyumluluğu garantisi değildir. Uygulama sırasında bulunan çelişki iki normatif belge birlikte güncellenerek çözülmelidir.
