#!/usr/bin/env python3
"""Builds the static project site (English and Turkish) from site/src/{en,tr}.html.

The templates hold the page content. Placeholders @@seq:NAME@@ and @@fig:NAME@@ are replaced by
diagrams drawn in plain HTML/CSS (they reflow on every screen width). Tables get data-labels so
they can stack on phones. Output: site/index.html and site/tr/index.html. Run:

    python3 scripts/build-site.py
"""
import html
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
REPO = "https://github.com/nuricanozturk01/project-ilmek"

CHROME = {
    "en": dict(
        lang="en", title="İlmek — your own small internet, without the internet",
        desc="İlmek publishes and opens applications on your own network with its own protocol, ILMEK/1, directly over TCP. No cloud, no public internet.",
        menu="Menu", skip="Skip to content", other="Türkçe", other_href="tr/", gh="GitHub", on_this="On this page",
        foot="İlmek · MIT OR Apache-2.0 ·", src="source on GitHub", prefix="", out="index.html",
    ),
    "tr": dict(
        lang="tr", title="İlmek — internetsiz, kendi küçük internetiniz",
        desc="İlmek, kendi ağınızda uygulama yayınlar ve açar; kendi protokolü ILMEK/1 ile doğrudan TCP üzerinden çalışır. Bulut yok, genel internet yok.",
        menu="Menü", skip="İçeriğe geç", other="English", other_href="../", gh="GitHub", on_this="Bu sayfada",
        foot="İlmek · MIT OR Apache-2.0 ·", src="GitHub'da kaynak", prefix="../", out="tr/index.html",
    ),
}

TRMAP = str.maketrans("çğıöşüÇĞİÖŞÜ", "cgiosuCGIOSU")


def slug(text):
    text = re.sub(r"<[^>]+>", "", html.unescape(text)).translate(TRMAP).lower()
    return re.sub(r"[^a-z0-9]+", "-", text).strip("-")


def T(en, tr):
    return {"en": en, "tr": tr}


# --------------------------------------------------------------------------- sequence diagrams
def M(a, b, kind, label, sub=None):
    return ("m", a, b, kind, label, sub)


def S(a, b, text):
    return ("self", a, b, text)


def N(text, cls=""):
    return ("note", text, cls)


SEQ = {}

SEQ["discover"] = dict(
    lanes=T([("Device", "new, no saved state"), ("Beacon", "UDP 47400"), ("Registry", "TCP 47401")],
            [("Cihaz", "yeni, kayıtlı durumu yok"), ("Beacon", "UDP 47400"), ("Registry", "TCP 47401")]),
    title=T("Finding the node (two ways, started together)", "Node'u bulmak (iki yol, birlikte başlar)"),
    cap=T("The first answer wins. With a known address (<code>--registry</code>) only the second path is used, to that address.",
          "İlk gelen yanıt kullanılır. Adres biliniyorsa (<code>--registry</code>) yalnızca ikinci yol, o adrese karşı kullanılır."),
    rows=[
        M(0, 1, "req", "ILMEK-DISCOVER/1", T("16-byte UDP probe, broadcast every 500 ms (≤ 4 s)", "16 byte'lık UDP probe, 500 ms'de bir yayın (≤ 4 sn)")),
        M(1, 0, "resp", "{community_id, registry_id, registry_port}", T("registry address = source IP of this answer", "registry adresi = bu yanıtın kaynak IP'si")),
        N(T("…or, in parallel, when UDP is blocked or an address is given:", "…ya da, UDP engelliyse veya adres verildiyse, paralel olarak:")),
        M(0, 2, "req", "HELLO", T("random community_id; TCP :47401 on each /24 address (64 at a time, 700 ms)", "rastgele community_id; /24'teki her adreste TCP :47401 (aynı anda 64, 700 ms)")),
        M(2, 0, "bad", "ERROR community mismatch", T("details: {community_id, server_id, server_role: registry}", "details: {community_id, server_id, server_role: registry}")),
        N(T("Device saves <code>profile.json</code> and <code>device.id</code>. It never discovers again.", "Cihaz <code>profile.json</code> ve <code>device.id</code> dosyalarını kaydeder. Bir daha keşif yapmaz."), "good"),
    ])

SEQ["handshake"] = dict(
    lanes=T([("Client", None), ("Server", "registry or host")], [("İstemci", None), ("Sunucu", "registry veya host")]),
    title=T("One connection, start to finish", "Bir bağlantı, baştan sona"),
    cap=T("Every connection starts with HELLO. Its mode (<code>control</code> or <code>bulk</code>) never changes afterwards.",
          "Her bağlantı HELLO ile başlar. Kipi (<code>control</code> veya <code>bulk</code>) sonra değişmez."),
    rows=[
        M(0, 1, "req", "TCP connect", T("deadline 5 s", "süre sınırı 5 sn")),
        M(0, 1, "req", "HELLO", T("within 5 s · client_id, mode, client_version", "5 sn içinde · client_id, mode, client_version")),
        M(1, 0, "resp", "HELLO_ACK", T("session_id, server_role, max_inflight, max_subscriptions", "session_id, server_role, max_inflight, max_subscriptions")),
        N(T("Only now are other requests accepted.", "Başka istekler ancak şimdi kabul edilir."), "good"),
        M(0, 1, "req", "any request", T("new request_id (UUIDv7)", "yeni request_id (UUIDv7)")),
        M(1, 0, "resp", "its response", T("same request_id; may arrive out of order", "aynı request_id; sırasız gelebilir")),
        M(0, 1, "req", "PING", T("after 20 s without outbound traffic", "20 sn giden trafik yoksa")),
        M(1, 0, "resp", "PONG", T("within 10 s, or the client closes", "10 sn içinde, yoksa istemci kapatır")),
        M(0, 1, "req", "GOODBYE", T("reason: user | shutdown | reconnect", "reason: user | shutdown | reconnect")),
        M(1, 0, "resp", "GOODBYE_ACK", T("a server may also send DRAIN (grace ≤ 30 s)", "sunucu DRAIN da gönderebilir (süre ≤ 30 sn)")),
    ])

SEQ["open"] = dict(
    lanes=T([("Client", None), ("Registry", None), ("Host", None), ("Backend", "loopback")],
            [("İstemci", None), ("Registry", None), ("Host", None), ("Arka uç", "loopback")]),
    title=T("Opening <code>ilmek://nuri/forum</code>", "<code>ilmek://nuri/forum</code> adresini açmak"),
    cap=T("The registry only answers “who serves this name”. Everything after that happens between the client and the host.",
          "Registry yalnızca “bu ismi kim sunuyor?” sorusunu yanıtlar. Sonrası istemci ile host arasında olur."),
    rows=[
        M(0, 1, "req", "RESOLVE", T("namespace nuri, slug forum", "namespace nuri, slug forum")),
        M(1, 0, "resp", "RESOLVED", T("version, manifest_hash, host endpoint", "version, manifest_hash, host adresi")),
        N(T("The registry is done. It never sees application traffic.", "Registry işini bitirdi. Uygulama trafiğini hiç görmez.")),
        M(0, 2, "req", "GET_MANIFEST", T("control connection", "control bağlantı")),
        M(2, 0, "resp", "MANIFEST", T("exact manifest.json bytes", "manifest.json'ın tam byte'ları")),
        N(T("SHA-256 of the manifest must equal manifest_hash from the registry.", "Manifestin SHA-256'sı registry'deki manifest_hash'e eşit olmalı.")),
        M(0, 2, "req", "GET_ASSET × N", T("separate bulk connection", "ayrı bulk bağlantı")),
        M(2, 0, "resp", "INFO · CHUNK… · END", T("raw file bytes", "ham dosya byte'ları")),
        N(T("Every file's size and SHA-256 is checked; only then is the package activated and the page started in the sandbox.", "Her dosyanın boyutu ve SHA-256'sı denetlenir; ancak sonra paket etkinleşir ve sayfa sandbox'ta başlar."), "good"),
        M(0, 2, "req", "APP_CALL", T("method + body; operation_id for writes", "metot + gövde; yazmada operation_id")),
        M(2, 3, "req", "IPC_CALL", T("+ principal_id added by the host", "+ host'un eklediği principal_id")),
        M(3, 2, "resp", "IPC_RESULT", ""),
        M(2, 0, "resp", "APP_RESULT", T("same request_id", "aynı request_id")),
    ])

SEQ["write"] = dict(
    lanes=T([("Client", None), ("Host", None), ("Backend", "SQLite")], [("İstemci", None), ("Host", None), ("Arka uç", "SQLite")]),
    title=T("A write whose reply is lost", "Yanıtı kaybolan bir yazma"),
    cap=T("The client never invents a new operation id after an uncertain timeout. Repeating the same call is the safe way to learn the result.",
          "İstemci belirsiz bir zaman aşımından sonra yeni işlem kimliği üretmez. Aynı çağrıyı tekrarlamak sonucu öğrenmenin güvenli yoludur."),
    rows=[
        M(0, 1, "req", "APP_CALL", T("operation_id X · payload_hash", "operation_id X · payload_hash")),
        M(1, 2, "req", "IPC_CALL", ""),
        S(1, 2, T("<b>ONE transaction:</b> business change + dedupe record (X, method, hash) + result", "<b>TEK işlem:</b> iş değişikliği + tekrar önleme kaydı (X, metot, özet) + sonuç")),
        M(2, 1, "resp", "IPC_RESULT", ""),
        N(T("✕ The connection drops before the reply reaches the client. The client does NOT know whether it was applied.", "✕ Yanıt istemciye ulaşmadan bağlantı kopar. İstemci uygulanıp uygulanmadığını BİLMEZ."), "bad"),
        M(0, 1, "req", "APP_CALL", T("new connection · new request_id · SAME operation_id X · SAME body bytes", "yeni bağlantı · yeni request_id · AYNI operation_id X · AYNI gövde byte'ları")),
        M(1, 2, "req", "IPC_CALL", ""),
        S(1, 2, T("X found, same method and hash → <b>stored result returned, no second write</b>", "X bulundu, metot ve özet aynı → <b>saklanan sonuç döner, ikinci yazma yok</b>")),
        M(2, 1, "resp", "IPC_RESULT", ""),
        M(1, 0, "resp", "APP_RESULT", T("the original result", "özgün sonuç")),
        N(T("Same X with a different method or body → <code>OPERATION_CONFLICT</code>. Same X while the first is still running → <code>OPERATION_PENDING</code>: repeat.", "Aynı X farklı metot veya gövdeyle → <code>OPERATION_CONFLICT</code>. Aynı X, ilki hâlâ sürerken → <code>OPERATION_PENDING</code>: tekrarlayın.")),
    ])

SEQ["events"] = dict(
    lanes=T([("Client", None), ("Host", None)], [("İstemci", None), ("Host", None)]),
    title=T("Subscribing to events", "Olaylara abone olmak"),
    cap=T("The host sends at most 16 events that are not yet acknowledged, so a slow client can never make it buffer without bound.",
          "Host onaylanmamış en fazla 16 olay gönderir; yavaş bir istemci hostun sınırsız arabellek tutmasına yol açamaz."),
    rows=[
        M(0, 1, "req", "SUBSCRIBE", T("topic · cursor: null (new) or last stored cursor", "konu · cursor: null (yeni) veya son kayıtlı imleç")),
        M(1, 0, "resp", "SUBSCRIBED", T("subscription_id, window 16", "subscription_id, pencere 16")),
        M(1, 0, "resp", "EVENT 1, 2, 3 …", T("sequence, event_id, cursor · body JSON", "sequence, event_id, cursor · gövde JSON")),
        N(T("Client processes them, stores the cursor locally, then acknowledges.", "İstemci işler, imleci yerelde saklar, sonra onaylar.")),
        M(0, 1, "req", "EVENT_ACK", T("last contiguous sequence + its cursor", "son ardışık sequence + imleci")),
        M(1, 0, "resp", "EVENT_ACKED", T("window opens; the host reads more from its durable log", "pencere açılır; host kalıcı günlüğünden daha fazla okur")),
        N(T("No ACK progress for 30 s → <code>ERROR SLOW_CONSUMER</code>, only this subscription closes.", "30 sn ACK ilerlemesi yok → <code>ERROR SLOW_CONSUMER</code>, yalnızca bu abonelik kapanır."), "bad"),
        N(T("Connection lost → subscribe again with the last cursor. Older than the last 10000 events → <code>CURSOR_EXPIRED</code>: take a fresh snapshot. Delivery is at-least-once: the same event_id can repeat.", "Bağlantı koptu → son imleçle yeniden abone olun. Son 10000 olaydan eskiyse → <code>CURSOR_EXPIRED</code>: yeni anlık görüntü alın. Teslim en az bir kezdir: aynı event_id tekrarlanabilir.")),
    ])

SEQ["resume"] = dict(
    lanes=T([("Client", None), ("Host", "bulk connection")], [("İstemci", None), ("Host", "bulk bağlantı")]),
    title=T("Downloading a file, and resuming it", "Dosya indirmek ve sürdürmek"),
    cap=T("A file only becomes active when the SHA-256 of the whole file matches, including the part received before the interruption.",
          "Dosya, kesintiden önce alınan kısım dahil tüm dosyanın SHA-256'sı eşleştiğinde etkinleşir."),
    rows=[
        M(0, 1, "req", "HELLO", T("mode: bulk (one active file per connection)", "mode: bulk (bağlantı başına tek etkin dosya)")),
        M(1, 0, "resp", "HELLO_ACK", ""),
        M(0, 1, "req", "GET_ASSET", T("app, version, manifest_hash, path, asset_hash, offset 0", "uygulama, sürüm, manifest_hash, yol, asset_hash, offset 0")),
        M(1, 0, "resp", "ASSET_INFO", T("total_size, chunk_size 65536", "total_size, chunk_size 65536")),
        M(1, 0, "resp", "ASSET_CHUNK 0, 1, …", T("consecutive sequence and offset, ≤ 65536 B each", "ardışık sequence ve offset, her biri ≤ 65536 B")),
        N(T("✕ Connection drops. The client keeps the verified bytes in a temporary area.", "✕ Bağlantı kopar. İstemci doğrulanmış byte'ları geçici alanda tutar."), "bad"),
        M(0, 1, "req", "GET_ASSET", T("new connection · new request_id · offset = bytes already held", "yeni bağlantı · yeni request_id · offset = eldeki byte sayısı")),
        M(1, 0, "resp", "ASSET_INFO", T("offset echoed", "offset yankılanır")),
        M(1, 0, "resp", "ASSET_CHUNK …", ""),
        M(1, 0, "resp", "ASSET_END", T("asset_hash, total_sent", "asset_hash, total_sent")),
        N(T("END is not proof. The client hashes the whole file itself, then activates it. <code>CANCEL</code> stops a transfer; exactly one terminal response is sent.", "END kanıt değildir. İstemci tüm dosyanın özetini kendisi hesaplar, sonra etkinleştirir. <code>CANCEL</code> aktarımı durdurur; yalnızca bir son yanıt gönderilir."), "good"),
    ])

SEQ["register"] = dict(
    lanes=T([("Extra host", "ilmek host start"), ("Beacon", "UDP"), ("Registry", "TCP")], [("Ek host", "ilmek host start"), ("Beacon", "UDP"), ("Registry", "TCP")]),
    title=T("A host joins and stays registered", "Bir host katılır ve kayıtlı kalır"),
    cap=T("A host does not need to know its own address: it sends 0.0.0.0 and the registry records where the connection came from.",
          "Host kendi adresini bilmek zorunda değildir: 0.0.0.0 gönderir, registry bağlantının geldiği adresi kaydeder."),
    rows=[
        M(0, 1, "req", "UDP probe", ""),
        M(1, 0, "resp", "community, registry port", ""),
        M(0, 2, "req", "HELLO + HOST_REGISTER", T("endpoint 0.0.0.0:47402", "endpoint 0.0.0.0:47402")),
        S(1, 2, T("records the connection's <b>source IP</b>", "bağlantının <b>kaynak IP</b>'sini kaydeder")),
        M(2, 0, "resp", "HOST_REGISTERED", T("lease 90 s, heartbeat every 30 s", "kira 90 sn, 30 sn'de bir heartbeat")),
        M(0, 2, "req", "HOST_HEARTBEAT", T("every 30 s", "30 sn'de bir")),
        M(2, 0, "resp", "HEARTBEAT_ACK", T("lease renewed", "kira yenilendi")),
        N(T("No heartbeat for 90 s → the host's applications show availability <code>stale</code>.", "90 sn heartbeat yok → host'un uygulamaları <code>stale</code> erişilebilirlik gösterir."), "bad"),
    ])

SEQ["publish"] = dict(
    lanes=T([("Host machine", "ilmek app …"), ("Registry", None)], [("Host makinesi", "ilmek app …"), ("Registry", None)]),
    title=T("Publishing an application", "Uygulama yayınlamak"),
    cap=T("Install and set-backend are local administrator actions on the host machine, not network messages. Only the final step talks to the registry.",
          "Install ve set-backend host makinesinde yerel yönetici işlemleridir, ağ mesajı değildir. Registry ile yalnızca son adım konuşur."),
    rows=[
        S(0, 1, T("<b>app pack</b> → manifest.json with size + SHA-256 per file", "<b>app pack</b> → her dosya için boyut + SHA-256 içeren manifest.json")),
        S(0, 1, T("<b>app install</b> → verify, then activate atomically (stopped)", "<b>app install</b> → doğrula, sonra atomik etkinleştir (durdurulmuş)")),
        S(0, 1, T("<b>app set-backend</b> + <b>app start</b> → backend running", "<b>app set-backend</b> + <b>app start</b> → arka uç çalışıyor")),
        M(0, 1, "req", "REGISTER_APP", T("operation_id, namespace, slug, app_id, version, manifest_hash, server_id", "operation_id, namespace, slug, app_id, version, manifest_hash, server_id")),
        M(1, 0, "resp", "APP_REGISTERED", T("the record clients will RESOLVE", "istemcilerin RESOLVE edeceği kayıt")),
        N(T("The first device to register a namespace owns it; others get <code>FORBIDDEN</code>. A later version uses <code>UPDATE_APP</code>; the version can only go forward.", "Bir ad alanını ilk kaydeden cihaz sahip olur; diğerleri <code>FORBIDDEN</code> alır. Sonraki sürüm <code>UPDATE_APP</code> kullanır; sürüm yalnızca ileri gider.")),
    ])


def render_seq(name, lang):
    d = SEQ[name]
    lanes = d["lanes"][lang]
    names = [n for n, _ in lanes]
    actors_label = T("Participants", "Katılımcılar")[lang]
    out = [f'<div class="seq-actors"><span>{actors_label}:</span>' + "".join(f"<b>{n}</b>" for n in names) + "</div>",
           f'<div class="seq" style="--n:{len(lanes)}">']
    for nm, sub in lanes:
        out.append(f'<div class="lane">{nm}' + (f"<small>{sub}</small>" if sub else "") + "</div>")
    for row in d["rows"]:
        if row[0] == "m":
            _, a, b, kind, label, sub = row
            lo, hi = min(a, b), max(a, b)
            direction = "r" if a < b else "l"
            sub_txt = sub[lang] if isinstance(sub, dict) else (sub or "")
            out.append(
                f'<div class="m {direction} {kind}" data-route="{html.escape(names[a])} → {html.escape(names[b])}" style="grid-column:{lo + 1}/{hi + 2};--span:{hi - lo + 1}">'
                f"<b>{html.escape(label)}</b>" + (f"<i>{sub_txt}</i>" if sub_txt else "") + "</div>")
        elif row[0] == "self":
            _, a, b, text = row
            out.append(f'<div class="m self" style="grid-column:{a + 1}/{b + 2}"><i style="color:inherit">{text[lang]}</i></div>')
        else:
            _, text, cls = row
            out.append(f'<div class="note {cls}">{text[lang]}</div>')
    out.append("</div>")
    legend = T('<span>request</span><span class="d">response / push</span>', '<span>istek</span><span class="d">yanıt / itme</span>')[lang]
    return (f'<figure class="fig"><p class="ft">{d["title"][lang]}</p>' + "".join(out)
            + f'<div class="legend">{legend}</div><figcaption>{d["cap"][lang]}</figcaption></figure>')


# --------------------------------------------------------------------------- other figures
def fig_arch(t):
    return f"""<figure class="fig"><p class="ft">{t('The parts and who talks to whom', 'Parçalar ve kimin kiminle konuştuğu')}</p>
<div class="arch">
 <div class="client"><b>{t('Client', 'İstemci')}</b><p style="margin:.2rem 0 .7rem;color:var(--muted);font-size:.9rem">{t('ilmek command or desktop window', 'ilmek komutu veya masaüstü penceresi')}</p>
  <ol class="steps">
   <li><span class="n">1</span><span>{t('find the node: UDP probe or TCP scan', "node'u bul: UDP probe veya TCP tarama")}</span></li>
   <li><span class="n">2</span><span>{t('<code>RESOLVE</code> the name at the registry', "ismi registry'de <code>RESOLVE</code> et")}</span></li>
   <li><span class="n">3</span><span>{t('download and call, directly on the host', 'doğrudan host üzerinde indir ve çağır')}</span></li>
  </ol></div>
 <div class="node"><h4>{t('Node — one process: <code>ilmek node</code>', 'Node — tek süreç: <code>ilmek node</code>')}</h4>
  <div class="items">
   <div class="it"><b><span class="n">1</span>Beacon</b><span>UDP 47400</span></div>
   <div class="it"><b><span class="n">2</span>{t('Registry', 'Registry')}</b><span>TCP 47401</span></div>
   <div class="it"><b><span class="n">3</span>{t('Application host', 'Uygulama sunucusu')}</b><span>TCP 47402</span></div>
   <div class="it"><b>{t('Backend processes', 'Arka uç süreçleri')}</b><span>{t('loopback IPC, one per app', 'loopback IPC, uygulama başına bir')}</span></div>
  </div></div>
 <div class="more-hosts"><b>{t('More machines:', 'Başka makineler:')}</b> {t('<code>ilmek host start</code> finds the node, registers itself and serves its own applications. Clients download from it directly (step 3).', "<code>ilmek host start</code> node'u bulur, kendini kaydeder ve kendi uygulamalarını sunar. İstemciler doğrudan ondan indirir (adım 3).")}</div>
</div>
<figcaption>{t('The registry is a directory, not a proxy: application traffic goes straight from client to host.', 'Registry bir rehberdir, vekil değil: uygulama trafiği doğrudan istemciden host’a gider.')}</figcaption></figure>"""


def fig_layers(t):
    return f"""<figure class="fig"><p class="ft">{t('Where ILMEK/1 sits', 'ILMEK/1 nerede durur?')}</p>
<div class="stacks">
 <div class="stack"><h4>ILMEK/1</h4>
  <div>{t('Applications', 'Uygulamalar')}<small>{t('HTML/JS page + optional backend', 'HTML/JS sayfa + isteğe bağlı arka uç')}</small></div>
  <div class="me">ILMEK/1<small>{t('binary frames, JSON messages', 'ikili çerçeveler, JSON mesajlar')}</small></div>
  <div>TCP</div><div>IP</div><div class="os">{t('Wi-Fi / Ethernet / hotspot', 'Wi-Fi / Ethernet / hotspot')}</div></div>
 <div class="stack"><h4>{t('A web app, for comparison', 'Karşılaştırma: bir web uygulaması')}</h4>
  <div>{t('Web application', 'Web uygulaması')}</div>
  <div class="me">HTTP<small>{t('text requests and status codes', 'metin istekleri ve durum kodları')}</small></div>
  <div>TCP</div><div>IP</div><div class="os">{t('Wi-Fi / Ethernet / mobile data', 'Wi-Fi / Ethernet / mobil veri')}</div></div>
</div>
<figcaption>{t('ILMEK/1 replaces what HTTP does, not TCP or IP. This version has no TLS layer (the secure design on the <code>security</code> branch puts TLS 1.3 between TCP and ILMEK/1).', 'ILMEK/1, HTTP’nin yaptığı işin yerini alır; TCP veya IP’nin değil. Bu sürümde TLS katmanı yoktur (<code>main</code> daki güvenli tasarım TLS 1.3’ü TCP ile ILMEK/1 arasına koyar).')}</figcaption></figure>"""


def fig_frame(t):
    return f"""<figure class="fig"><p class="ft">{t('Anatomy of one frame', 'Bir çerçevenin yapısı')}</p>
<div class="fbar">
 <div class="c-magic"><b>magic</b>4 B<small>ILM1</small></div>
 <div class="c-m"><b>M</b>4 B<small>{t('metadata length', 'metadata uzunluğu')}</small></div>
 <div class="c-b"><b>B</b>4 B<small>{t('body length', 'gövde uzunluğu')}</small></div>
 <div class="c-meta"><b>{t('metadata', 'metadata')}</b>M bytes (1…65536)<small>UTF-8 JSON</small></div>
 <div class="c-body"><b>{t('body', 'gövde')}</b>B bytes (0…262144)<small>{t('raw bytes', 'ham byte')}</small></div>
</div>
<p style="margin:.9rem 0 .2rem;font-size:.9rem"><b>{t('A real frame', 'Gerçek bir çerçeve')}</b> — {t('the <code>RESOLVE nuri/forum</code> request from the test fixtures, 179 bytes in total:', "test fixture'larındaki <code>RESOLVE nuri/forum</code> isteği, toplam 179 byte:")}</p>
<div class="hex" role="img" aria-label="49 4c 4d 31 00 00 00 a7 00 00 00 00 7b 22 74 79 70 65">
 <span class="h1">49 4c 4d 31</span><span class="h2">00 00 00 a7</span><span class="h3">00 00 00 00</span><span class="h4">7b 22 74 79 70 65 22 3a 22 52 45 53 4f 4c 56 45 22 2c</span>…</div>
<div class="hexlegend">
 <span class="h1">49 4c 4d 31 = “ILM1”</span>
 <span class="h2">00 00 00 a7 = {t('M = 167', 'M = 167')}</span>
 <span class="h3">00 00 00 00 = {t('B = 0 (no body)', 'B = 0 (gövde yok)')}</span>
 <span class="h4">7b 22 74 79 … = <code>{{"type":"RESOLVE",</code> …</span></div>
<pre><code>{{"type":"RESOLVE","request_id":"01a160e8-5066-7b4d-b5a9-5a83c695c819",
 "community_id":"01a160e8-5001-7e8d-beb8-a77c859b95da",
 "data":{{"namespace":"nuri","slug":"forum"}}}}        {t('(167 bytes, one line on the wire)', '(167 byte, telde tek satır)')}</code></pre>
<figcaption>{t('12 + 167 + 0 = 179 bytes. The reader first reads 12 bytes, checks the magic and both lengths against the limits, and only then reads M and B more bytes.', 'Okuyucu önce 12 byte okur, magic’i ve iki uzunluğu sınırlara karşı denetler, ancak sonra M ve B byte daha okur.')}</figcaption></figure>"""


def flow(items, horizontal=False):
    out = [f'<ol class="flow{" h" if horizontal else ""}">']
    for i, (cls, arrow, title, text) in enumerate(items):
        data = f' data-arrow="{arrow}"' if arrow and i < len(items) - 1 else ""
        out.append(f'<li class="{cls}"{data}><b>{title}</b><span>{text}</span></li>')
    out.append("</ol>")
    return "".join(out)


def fig_integrity(t):
    body = flow([
        ("blue", t("manifest_hash", "manifest_hash"), t("Registry record", "Registry kaydı"), t("name → version, <code>manifest_hash</code>, host", "isim → sürüm, <code>manifest_hash</code>, host")),
        ("", t("SHA-256 must equal", "SHA-256 eşit olmalı"), "manifest.json", t("the exact bytes the host stored; lists every file with path, size and sha256", "host'un sakladığı tam byte'lar; her dosyayı yol, boyut ve sha256 ile listeler")),
        ("", t("size + SHA-256 must match", "boyut + SHA-256 eşleşmeli"), t("Each file", "Her dosya"), t("downloaded to a temporary area on the client", "istemcide geçici alana indirilir")),
        ("green", "", t("Atomic activation", "Atomik etkinleştirme"), t("the new version directory is switched in; a failure leaves the old one running", "yeni sürüm klasörü devreye girer; hata eskisini çalışır bırakır")),
    ], horizontal=True)
    return f"""<figure class="fig"><p class="ft">{t('How a package is verified', 'Paket nasıl doğrulanır?')}</p>{body}
<figcaption>{t('This catches corruption and partial downloads. It does not stop a malicious host, because it can change both the manifest and the record: packages are not signed in this version.', 'Bu, bozulmayı ve yarım indirmeyi yakalar. Kötü niyetli bir host’u durdurmaz; çünkü hem manifesti hem kaydı değiştirebilir: bu sürümde paketler imzalı değildir.')}</figcaption></figure>"""


def fig_boundaries(t):
    body = flow([
        ("acc", t("postMessage only", "yalnızca postMessage"), t("Page (HTML/JS)", "Sayfa (HTML/JS)"), t("<b>Untrusted.</b> Sandboxed iframe, opaque origin, no network of its own", "<b>Güvenilmez.</b> Sandbox'lı iframe, opak origin, kendi ağı yok")),
        ("", t("ILMEK/1 over TCP", "TCP üzerinden ILMEK/1"), t("Client shell", "İstemci kabuğu"), t("Relays only its own frames' messages; the bridge has no field for a host, address or file path", "Yalnızca kendi çerçevelerinin mesajlarını iletir; köprüde host, adres veya dosya yolu alanı yoktur")),
        ("", t("loopback + per-process token", "loopback + süreç başına token"), t("Host", "Host"), t("Checks session, app, permission and body hash; adds <code>principal_id</code>", "Oturumu, uygulamayı, izni ve gövde özetini denetler; <code>principal_id</code> ekler")),
        ("green", "", t("Backend", "Arka uç"), t("<b>Trusted</b> local process chosen by the administrator; owns the SQLite data. A separate process is not a sandbox", "Yönetici tarafından seçilen <b>güvenilir</b> yerel süreç; SQLite verisinin sahibi. Ayrı süreç sandbox değildir")),
    ], horizontal=True)
    return f"""<figure class="fig"><p class="ft">{t('Trust boundaries of one call', 'Bir çağrının güven sınırları')}</p>{body}
<figcaption>{t('The network link between client and host is plain TCP in this version: anyone on the path can read or change it.', 'Bu sürümde istemci ile host arasındaki ağ bağlantısı düz TCP’dir: yoldaki herkes okuyabilir veya değiştirebilir.')}</figcaption></figure>"""


def fig_publish(t):
    body = flow([
        ("", "app pack", t("Project", "Proje"), "<code>app.toml</code> + <code>assets/</code>"),
        ("", "app install", t("Package", "Paket"), t("<code>manifest.json</code> + files, SHA-256 for each", "<code>manifest.json</code> + dosyalar, her biri için SHA-256")),
        ("", "set-backend · start", t("Installed", "Kurulu"), t("verified and activated on the host, stopped", "host'ta doğrulandı ve etkinleştirildi, durdurulmuş")),
        ("", "app publish", t("Running", "Çalışıyor"), t("backend up (if the app has one)", "arka uç ayakta (varsa)")),
        ("blue", t("clients RESOLVE", "istemciler RESOLVE eder"), t("Published", "Yayında"), t("registry record: name → version, manifest_hash, host", "registry kaydı: isim → sürüm, manifest_hash, host")),
        ("green", "", t("Opened", "Açıldı"), "<code>ilmek://namespace/slug</code>"),
    ])
    return f"""<figure class="fig"><p class="ft">{t('From source to a running application', 'Kaynaktan çalışan uygulamaya')}</p>{body}<figcaption>{t('Update: raise <code>version</code>, pack, <code>app update</code>, <code>app publish</code>. Roll back: <code>app rollback --version</code>.', 'Güncelleme: <code>version</code>’ı artırın, pack, <code>app update</code>, <code>app publish</code>. Geri alma: <code>app rollback --version</code>.')}</figcaption></figure>"""


def fig_lifecycle(t):
    return f"""<figure class="fig"><p class="ft">{t("A backend's life on the host", 'Bir arka ucun host üzerindeki yaşamı')}</p>
<div class="chips"><span class="chip">{t('installed, stopped', 'kurulu, durdurulmuş')}</span><span class="ar">app start →</span><span class="chip ok">running</span><span class="ar">{t('crash →', 'çökme →')}</span><span class="chip warn">{t('restart after a delay', 'gecikmeyle yeniden başlatma')}</span><span class="ar">{t('works →', 'çalışır →')}</span><span class="chip ok">running</span></div>
<div class="chips"><span class="chip warn">{t('restart after a delay', 'gecikmeyle yeniden başlatma')}</span><span class="ar">{t('keeps failing →', 'başarısız kalır →')}</span><span class="chip bad">{t('abandoned', 'bırakıldı')}</span><span class="ar">app restart →</span><span class="chip ok">running</span></div>
<div class="chips"><span class="chip ok">running</span><span class="ar">app stop →</span><span class="chip">{t('stopped', 'durduruldu')}</span></div>
<figcaption>{t('The delay starts at <code>restart_backoff_ms</code> and doubles up to 30 s. While one backend is down, calls to it fail with <code>APP_UNAVAILABLE</code>; every other application keeps running.', 'Gecikme <code>restart_backoff_ms</code> ile başlar ve 30 sn’ye kadar ikiye katlanır. Bir arka uç kapalıyken ona yapılan çağrılar <code>APP_UNAVAILABLE</code> ile başarısız olur; diğer tüm uygulamalar çalışmaya devam eder.')}</figcaption></figure>"""


FIGS = dict(arch=fig_arch, layers=fig_layers, frame=fig_frame, integrity=fig_integrity,
            boundaries=fig_boundaries, publish=fig_publish, lifecycle=fig_lifecycle)


# --------------------------------------------------------------------------- page assembly
def label_tables(markup):
    def table(m):
        block = m.group(0)
        heads = [re.sub(r"<[^>]+>", "", h).strip() for h in re.findall(r"<th[^>]*>(.*?)</th>", block, flags=re.S)]
        if not heads:
            return f'<div class="table-wrap">{block}</div>'

        def row(rm):
            cells = iter(range(len(heads)))

            def td(tm):
                i = next(cells, None)
                label = heads[i] if i is not None and i < len(heads) else ""
                attr = f' data-label="{html.escape(label, quote=True)}"' if label else ""
                return f"<td{attr}{tm.group(1)}>"

            return re.sub(r"<td([^>]*)>", td, rm.group(0))

        block = re.sub(r"<tr>.*?</tr>", row, block, flags=re.S)
        return f'<div class="table-wrap">{block}</div>'

    return re.sub(r"<table>.*?</table>", table, markup, flags=re.S)


def expand(markup, lang):
    t = lambda en, tr: tr if lang == "tr" else en  # noqa: E731
    markup = re.sub(r"@@seq:(\w+)@@", lambda m: render_seq(m.group(1), lang), markup)
    markup = re.sub(r"@@fig:(\w+)@@", lambda m: FIGS[m.group(1)](t), markup)
    return markup


BRAND = ('<svg viewBox="0 0 32 32" aria-hidden="true"><path d="M5 5l5.5 9L16 5M16 5l5.5 9L27 5M5 18l5.5 9L16 18M16 18l5.5 9L27 18" '
         'fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" style="color:var(--accent)"/></svg>')
FAVICON = ("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Cpath d='M5 5l5.5 9L16 5M16 5l5.5 9L27 5M5 18l5.5 9L16 18M16 18l5.5 9L27 18' "
           "fill='none' stroke='%23b23b22' stroke-width='3' stroke-linecap='round' stroke-linejoin='round'/%3E%3C/svg%3E")

JS = """<script>
(function(){
  var q=function(s,c){return Array.prototype.slice.call((c||document).querySelectorAll(s))};
  q('pre').forEach(function(p){var b=document.createElement('button');b.className='copy';b.type='button';b.textContent='copy';
    b.addEventListener('click',function(){var t=p.querySelector('code').innerText;
      (navigator.clipboard?navigator.clipboard.writeText(t):Promise.reject()).then(function(){b.textContent='ok';setTimeout(function(){b.textContent='copy'},1200)},function(){});});
    p.appendChild(b);});
  var m=document.querySelector('.menu');
  if(m){q('a',m).forEach(function(a){a.addEventListener('click',function(){m.removeAttribute('open')})});}
  if(!('IntersectionObserver' in window))return;
  var links={};q('.side a').forEach(function(a){links[a.getAttribute('href').slice(1)]=a});
  var cur=null;
  var io=new IntersectionObserver(function(es){es.forEach(function(e){if(e.isIntersecting){
      if(cur)cur.classList.remove('on');cur=links[e.target.id]||null;if(cur){cur.classList.add('on');
        var s=cur.closest('.side');if(s){var r=cur.getBoundingClientRect(),sr=s.getBoundingClientRect();
          if(r.top<sr.top+40||r.bottom>sr.bottom-40)s.scrollTop+=r.top-sr.top-sr.height/2;}}}})},
    {rootMargin:'-20% 0px -70% 0px'});
  q('section[id], h3[id]').forEach(function(e){io.observe(e)});
})();
</script>"""


def build(lang):
    c = CHROME[lang]
    src = (SITE / "src" / f"{lang}.html").read_text(encoding="utf-8")
    src = expand(src, lang)
    # ids on h3, collected for the sidebar
    nav = []  # (kind, id, label)

    def sections(markup):
        out, pos = [], 0
        for m in re.finditer(r'<section id="([\w-]+)" data-nav="([^"]+)">', markup):
            out.append((m.start(), m.group(1), m.group(2)))
        return out

    parts = sections(src)
    for i, (start, sid, label) in enumerate(parts):
        end = parts[i + 1][0] if i + 1 < len(parts) else len(src)
        nav.append(("l1", sid, label))
        for h in re.finditer(r"<h3>(.*?)</h3>", src[start:end], flags=re.S):
            nav.append(("l2", f"{sid}-{slug(h.group(1))}", re.sub(r"<[^>]+>", "", h.group(1))))

    def add_h3_ids(markup):
        cur = {"id": ""}

        def sec(m):
            cur["id"] = m.group(1)
            return m.group(0)

        out, last = [], 0
        for m in re.finditer(r'<section id="([\w-]+)" data-nav="[^"]+">|<h3>(.*?)</h3>', markup, flags=re.S):
            out.append(markup[last:m.start()])
            if m.group(1):
                cur["id"] = m.group(1)
                out.append(m.group(0))
            else:
                out.append(f'<h3 id="{cur["id"]}-{slug(m.group(2))}">{m.group(2)}</h3>')
            last = m.end()
        out.append(markup[last:])
        return "".join(out)

    src = add_h3_ids(src)
    src = re.sub(r'<section id="([\w-]+)" data-nav="[^"]+">', r'<section id="\1">', src)
    src = label_tables(src)

    side = "".join(f'<a class="{k}" href="#{i}">{html.escape(l)}</a>' for k, i, l in nav)
    panel = "".join(f'<a href="#{i}">{html.escape(l)}</a>' for k, i, l in nav if k == "l1")
    css = f'{c["prefix"]}assets/style.css'
    other = c["other_href"]
    page = f"""<!doctype html>
<html lang="{c['lang']}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>{c['title']}</title>
<meta name="description" content="{html.escape(c['desc'], quote=True)}">
<meta name="theme-color" content="#b23b22">
<meta property="og:title" content="{html.escape(c['title'], quote=True)}">
<meta property="og:description" content="{html.escape(c['desc'], quote=True)}">
<meta property="og:type" content="website">
<link rel="icon" href="{FAVICON}">
<link rel="stylesheet" href="{css}">
<link rel="alternate" hreflang="{'tr' if lang == 'en' else 'en'}" href="{other}">
</head>
<body>
<a class="skip" href="#top">{c['skip']}</a>
<header class="top">
  <a class="brand" href="./">{BRAND}İlmek</a>
  <a class="pill" href="{other}" hreflang="{'tr' if lang == 'en' else 'en'}">{c['other']}</a>
  <a class="pill gh" href="{REPO}">{c['gh']}</a>
  <details class="menu"><summary class="pill">{c['menu']}</summary><nav class="panel" aria-label="{c['on_this']}">{panel}</nav></details>
</header>
<div class="layout">
<nav class="side" aria-label="{c['on_this']}">{side}</nav>
<main id="top"><div class="wide">
{src}
</div></main>
</div>
<footer>{c['foot']} <a href="{REPO}">{c['src']}</a> · <a href="{other}">{c['other']}</a></footer>
{JS}
</body>
</html>
"""
    out = SITE / c["out"]
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(page, encoding="utf-8")
    print("wrote", out.relative_to(ROOT), len(page), "bytes;", len(nav), "nav entries")


if __name__ == "__main__":
    for lang in CHROME:
        build(lang)
    sys.exit(0)
