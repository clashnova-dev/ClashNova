<div align="center">

<img src="../../app-icon.png" width="116" alt="ClashNova" />

<h1>ClashNova Desktop</h1>

<p>
  <b>Ein lokal-first Proxy-Client auf Basis von <a href="https://sing-box.sagernet.org/">sing-box</a>.</b><br/>
  Abonnement importieren und verbunden sein – Parsen, Routing und ausgehende Verbindung laufen auf deinem Rechner.
</p>

<p>
  <a href="../../LICENSE"><img alt="License" src="https://img.shields.io/badge/License-GPL--3.0-2f6fed?style=flat-square"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/macOS%20·%20Windows%20·%20Linux-5b6472?style=flat-square">
  <img alt="Vue 3" src="https://img.shields.io/badge/Vue_3-42b883?style=flat-square&logo=vuedotjs&logoColor=white">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri_2-24c8db?style=flat-square&logo=tauri&logoColor=white">
  <img alt="sing-box core" src="https://img.shields.io/badge/core-sing--box-7d56f4?style=flat-square">
  <a href="https://x.com/ClashNova2048"><img alt="Follow on X" src="https://img.shields.io/badge/Follow-%40ClashNova2048-000000?style=flat-square&logo=x&logoColor=white"></a>
</p>

<p>
  <a href="../../README.md">English</a> ·
  <a href="README.zh-CN.md">简体中文</a> ·
  <b>Deutsch</b> ·
  <a href="README.fr.md">Français</a> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a>
</p>

<img src="../screenshots/overview-light.jpg" width="840" alt="ClashNova Desktop" />

</div>

> Übersetzungen können dem englischen Hauptdokument hinterherhinken.
## ✨ Funktionen

- **Übersicht** – mit einem Klick verbinden oder trennen, mit Live-Verbindungsstatus, aktuellem Knoten, Up-/Download-Geschwindigkeit in Echtzeit und Sitzungsverbrauch. Abonnementverbrauch und die Ausgangs-IP mit Geo-Zuordnung werden direkt angezeigt (ausblendbar, manuell oder per Timer aktualisiert), daneben der schnelle Wechsel zwischen häufig genutzten Knoten.
- **Knoten** – nach Name suchen, nach Latenz sortieren oder mit *Alle testen* jeden Knoten asynchron messen; die Ergebnisse treffen nach und nach ein und lassen sich nach niedrigster Latenz neu sortieren. Knoten von Hand wählen oder der automatischen Auswahl überlassen (die `urltest`-Gruppe des Kernels wählt fortlaufend den schnellsten).
- **Abonnements** – Clash-, sing-box- oder universelle (Share-Link-)Abonnements aus einer URL oder einer lokalen Datei importieren; die Umwandlung läuft lokal statt über einen Onlinedienst. Jedes lässt sich aktualisieren, kopieren oder entfernen, und Verbrauch und Ablauf werden angezeigt, sofern der Anbieter sie meldet.
- **Verbindung und Routing** – **Systemproxy**, **manueller Proxy** oder **TUN** (eine virtuelle Netzwerkschnittstelle, die eine Systemautorisierung erfordert). Zwischen **Regel-** und **Global-**Routing wechseln und lokalen Port, LAN-Zugriff und mehr konfigurieren. Eine Liste aktiver Verbindungen, das Live-Log des Kernels und Verkehrsstatistiken stehen bereit.
- **Systemintegration** – Infobereich (Tray), Autostart sowie helles / dunkles / systemabhängiges Design. Die Oberfläche ist auf Vereinfachtes Chinesisch, Englisch, 日本語, 한국어, Deutsch, Français und Русский verfügbar.

## 🌟 Highlights

Ein paar Dinge, die über einen reinen Abo-Client hinausgehen.

### Proxy-Kette

Leite den Verkehr der Reihe nach über mehrere Knoten — einen Eingang, beliebig viele Relais und einen Ausgang. Baue eine Kette von Hand, miss ihre Ende-zu-Ende-Latenz und speichere häufig genutzte Ketten als benannte Vorlagen. Die Latenz summiert sich über die Sprünge, und die Kette ist nur so zuverlässig wie ihr schwächstes Glied — der Editor hält daher den gesamten Pfad im Blick.

<p align="center"><img src="../screenshots/special-chain.jpg" width="760" alt="Proxy-Ketten-Editor"></p>

### Netzwerkschutz & Prüfungen

Die Seite **Netzwerk** bündelt alles dazu, wie der Verkehr das Gerät verlässt. Wähle, wie das Proxying angewendet wird — Systemproxy, TUN oder manuell — mit einem verständlichen Hinweis, was jeder Modus abdeckt und was nicht. Der **Kill Switch** (TUN-Modus) blockiert allen Verkehr in dem Moment, in dem der Tunnel abbricht, sodass nichts auf die echte Verbindung zurückfällt. Auf Abruf laufen ein Zustandscheck (lokales Netzwerk, Erreichbarkeit des Proxys, DNS, Ausgangs-IP), DNS-/IPv6-/WebRTC-Leaktests sowie die Erkennung der Freischaltung von Streaming- und KI-Diensten.

<p align="center"><img src="../screenshots/special-network.jpg" width="760" alt="Netzwerkschutz und Prüfungen"></p>

### LAN-Übertragung

Sende Text und Dateien an deine eigenen Geräte im selben Netzwerk, ohne Server oder Cloud dazwischen. Es funktioniert wie ein persönlicher Datei-Helfer-Chat: Gerät einmal koppeln, danach Nachrichten und Dateien in einem Verlauf austauschen. Das Koppeln schafft Vertrauen, sodass ein gekoppeltes Gerät automatisch empfangen kann.

<p align="center"><img src="../screenshots/special-lan.jpg" width="760" alt="LAN-Übertragung"></p>

### Routing-Regeln

Sieh dir die tatsächlich wirksame Routing-Tabelle an — Typ jeder Regel, was sie trifft, wohin sie den Verkehr leitet (direkt / Proxy / blockiert) und ob sie aus deinem Abo oder den eingebauten Standardwerten stammt. Suche nach Inhalt, Typ oder Ziel, um zu sehen, warum eine bestimmte Domain dorthin geht, wo sie hingeht.

<p align="center"><img src="../screenshots/special-rules.jpg" width="760" alt="Routing-Regeln"></p>

## 📸 Screenshots

| Übersicht | Knoten | Abonnements |
| :-: | :-: | :-: |
| <img src="../screenshots/overview-dark.jpg" width="240"> | <img src="../screenshots/nodes.jpg" width="240"> | <img src="../screenshots/subscriptions.jpg" width="240"> |

| Verbindungen | Logs | Einstellungen |
| :-: | :-: | :-: |
| <img src="../screenshots/connections.jpg" width="240"> | <img src="../screenshots/logs.jpg" width="240"> | <img src="../screenshots/settings.jpg" width="240"> |

## 🛠️ Erstellen

Du brauchst Node.js mit pnpm, Rust, Go, Python 3 sowie die Tauri-Systemabhängigkeiten für deine Plattform.

```sh
cp .env.example .env
pnpm install --frozen-lockfile
python3 scripts/prepare-sidecars.py   # den sing-box-Sidecar aus dem Quellcode kompilieren
pnpm tauri dev                         # Entwicklung
pnpm tauri build                       # Release-Build paketieren
```

`prepare-sidecars` baut den Kernel und den Abonnement-Konverter aus dem Quellcode, statt vorkompilierte Binärdateien herunterzuladen. Die dev- und build-Hooks von Tauri führen es automatisch aus, sodass du es nach Änderungen am Kernel-Quellcode nicht von Hand aufrufen musst.

Um die Oberfläche ohne Rust oder Go anzusehen, läuft das Frontend auch allein mit `pnpm dev` im Browser. `browserBridge()` ersetzt die native Seite durch Stubs, daher muss alles, was vom Kernel abhängt – Verbindung, Ausgangs-IP, Live-Geschwindigkeit – im echten Desktop-Build geprüft werden.

## 🍎 Unter macOS ausführen

Release-Builds sind nicht signiert oder notariell beglaubigt (es gibt kein Apple-Entwicklerzertifikat), daher kann macOS die heruntergeladene App ablehnen („ClashNova ist beschädigt …" oder eine Entwicklerwarnung). Entferne das Quarantäne-Flag und öffne die App dann normal – passe den Pfad an, falls du sie woanders installiert hast:

```sh
sudo xattr -rd com.apple.quarantine "/Applications/ClashNova.app"
```

## ⚙️ Konfiguration

Optionen zur Build-Zeit liest Vite aus einer `.env`-Datei (von `.gitignore` ignoriert):

```
VITE_DEFAULT_SERVER=     # optionale Backend-Adresse; leer = eigenständiger Abonnement-Client
VITE_ENABLE_LOGIN=false  # bei false wird die Kontoanmeldung ausgeblendet
```

Bei den Standardwerten ist die App ein eigenständiger Abonnement-Client, der keinen Server braucht. Die beiden Schalter sind der Erweiterungspunkt, um sie an ein eigenes Backend anzubinden.

## 🗂️ Projektstruktur

```
src/
  App.vue                 Hülle: Seitennavigation und die einzelnen Seiten
  composables/useApp.ts   Status-Hub (Sitzung / Abonnements / Knoten / Verbindung / Einstellungen)
  pages/                  Übersicht / Knoten / Abonnements / Verbindungen / Logs / Einstellungen / Hilfe
  i18n/                   Nachrichtenkataloge
  lib/                    Verbindungs-Zustandsmaschine, Abonnement-Parsing, Einstellungen, TUN-Dienst
  styles/app.css          helle und dunkle Design-Tokens
src-tauri/                Tauri-(Rust-)Host: Fenster, Tray und der Lebenszyklus von Kernel und TUN-Helfer
```

Der tatsächliche ausgehende Verkehr wird vom mitgelieferten sing-box-Kernel weitergeleitet.

## 📄 Lizenz

GPL-3.0, siehe [LICENSE](../../LICENSE). Das Projekt bindet den GPL-3.0-Kernel sing-box ein, daher müssen Distributionen derselben Lizenz folgen und den entsprechenden Quellcode bereitstellen.
