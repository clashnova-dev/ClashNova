<div align="center">

<img src="../../app-icon.png" width="116" alt="ClashNova" />

<h1>ClashNova Desktop</h1>

<p>
  <b>Un client proxy local-first fondé sur <a href="https://sing-box.sagernet.org/">sing-box</a>.</b><br/>
  Importez un abonnement et vous êtes connecté — analyse, routage et connexion sortante se font sur votre machine.
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
  <a href="README.de.md">Deutsch</a> ·
  <b>Français</b> ·
  <a href="README.ja.md">日本語</a> ·
  <a href="README.ko.md">한국어</a> ·
  <a href="README.ru.md">Русский</a>
</p>

<img src="../screenshots/overview-light.jpg" width="840" alt="ClashNova Desktop" />

</div>

> Les traductions peuvent être en retard sur le document principal en anglais.
## ✨ Fonctionnalités

- **Vue d'ensemble** — connexion ou déconnexion en un clic, avec l'état de la connexion en direct, le nœud actuel, le débit montant/descendant en temps réel et la consommation de la session. La consommation de l'abonnement et l'IP de sortie avec sa géolocalisation s'affichent sur place (masquables, actualisées manuellement ou par minuterie), à côté du basculement rapide entre vos nœuds habituels.
- **Nœuds** — recherche par nom, tri par latence, ou *Tout tester* pour mesurer chaque nœud de façon asynchrone ; les résultats arrivent au fil de l'eau et se retrient par latence la plus basse. Choisissez un nœud à la main, ou laissez la sélection automatique (le groupe `urltest` du cœur retient le plus rapide).
- **Abonnements** — importez des abonnements Clash, sing-box ou universels (liens de partage) depuis une URL ou un fichier local ; la conversion se fait localement plutôt que via un service en ligne. Chacun s'actualise, se copie ou se supprime, et sa consommation et son échéance s'affichent lorsque le fournisseur les communique.
- **Connexion et routage** — **proxy système**, **proxy manuel** ou **TUN** (une interface réseau virtuelle, qui nécessite une autorisation système). Basculez entre routage **par règles** et **global**, et configurez le port local, l'accès au réseau local, etc. Une liste des connexions actives, le journal en direct du cœur et des statistiques de trafic sont disponibles.
- **Intégration système** — barre d'état système, lancement au démarrage, et thèmes clair / sombre / selon le système. L'interface est disponible en chinois simplifié, anglais, 日本語, 한국어, Deutsch, Français et Русский.

## 🌟 Points forts

Quelques possibilités qui vont au-delà d'un simple client d'abonnement.

### Chaîne de proxys

Faites transiter le trafic par plusieurs nœuds dans l'ordre — une entrée, autant de relais que voulu et une sortie. Construisez une chaîne à la main, mesurez sa latence de bout en bout et enregistrez celles que vous utilisez comme préréglages nommés. La latence s'additionne à chaque saut et la chaîne ne vaut que son maillon le plus faible ; l'éditeur garde donc le chemin complet sous les yeux.

<p align="center"><img src="../screenshots/special-chain.jpg" width="760" alt="Éditeur de chaîne de proxys"></p>

### Protection réseau et tests

La page **Réseau** rassemble tout ce qui concerne la façon dont le trafic quitte la machine. Choisissez comment le proxy s'applique — proxy système, TUN ou manuel — avec une note en clair sur ce que chaque mode couvre ou non. Le **kill switch** (mode TUN) bloque tout le trafic dès que le tunnel tombe, pour que rien ne retombe sur votre vraie connexion. À la demande, lancez un bilan de santé (réseau local, accessibilité du proxy, DNS, IP de sortie), des tests de fuite DNS / IPv6 / WebRTC et la détection du déblocage des services de streaming et d'IA.

<p align="center"><img src="../screenshots/special-network.jpg" width="760" alt="Protection réseau et tests"></p>

### Transfert LAN

Envoyez du texte et des fichiers à vos propres appareils sur le même réseau, sans serveur ni cloud entre les deux. Cela fonctionne comme une discussion avec un assistant de fichiers personnel : appairez un appareil une fois, puis échangez messages et fichiers dans un fil. L'appairage établit la confiance, si bien qu'un appareil appairé peut recevoir automatiquement.

<p align="center"><img src="../screenshots/special-lan.jpg" width="760" alt="Transfert LAN"></p>

### Règles de routage

Inspectez la table de routage réellement en vigueur — le type de chaque règle, ce qu'elle met en correspondance, où elle envoie le trafic (direct / proxy / blocage) et si elle vient de votre abonnement ou des valeurs par défaut intégrées. Cherchez par contenu, type ou destination pour comprendre pourquoi un domaine donné prend tel chemin.

<p align="center"><img src="../screenshots/special-rules.jpg" width="760" alt="Règles de routage"></p>

## 📸 Captures d'écran

| Vue d'ensemble | Nœuds | Abonnements |
| :-: | :-: | :-: |
| <img src="../screenshots/overview-dark.jpg" width="240"> | <img src="../screenshots/nodes.jpg" width="240"> | <img src="../screenshots/subscriptions.jpg" width="240"> |

| Connexions | Journaux | Paramètres |
| :-: | :-: | :-: |
| <img src="../screenshots/connections.jpg" width="240"> | <img src="../screenshots/logs.jpg" width="240"> | <img src="../screenshots/settings.jpg" width="240"> |

## 🛠️ Compilation

Il vous faut Node.js avec pnpm, Rust, Go, Python 3, ainsi que les dépendances système de Tauri pour votre plateforme.

```sh
cp .env.example .env
pnpm install --frozen-lockfile
python3 scripts/prepare-sidecars.py   # compiler le sidecar sing-box depuis les sources
pnpm tauri dev                         # développement
pnpm tauri build                       # produire une version de distribution
```

`prepare-sidecars` compile le cœur et le convertisseur d'abonnements depuis les sources au lieu de télécharger des binaires préconstruits. Les hooks dev et build de Tauri l'exécutent automatiquement, vous n'avez donc pas à l'appeler à la main après avoir modifié les sources du cœur.

Pour prévisualiser l'interface sans installer Rust ni Go, lancez le frontend seul avec `pnpm dev` et ouvrez-le dans un navigateur. `browserBridge()` remplace la partie native par des bouchons, donc tout ce qui dépend du cœur — la connexion, l'IP de sortie, le débit en direct — doit être vérifié dans la vraie application de bureau.

## 🍎 Exécution sur macOS

Les versions de distribution ne sont ni signées ni notarisées (il n'y a pas de certificat de développeur Apple), donc macOS peut refuser d'ouvrir une version téléchargée (« ClashNova est endommagé… » ou un avertissement de développeur non identifié). Supprimez l'indicateur de quarantaine, puis ouvrez l'application normalement — adaptez le chemin si vous l'avez installée ailleurs :

```sh
sudo xattr -rd com.apple.quarantine "/Applications/ClashNova.app"
```

## ⚙️ Configuration

Les options de compilation sont lues par Vite depuis un fichier `.env` (ignoré par `.gitignore`) :

```
VITE_DEFAULT_SERVER=     # URL de backend optionnelle ; vide = client d'abonnement autonome
VITE_ENABLE_LOGIN=false  # à false, l'écran de connexion au compte est masqué
```

Laissée à ses valeurs par défaut, l'application est un client d'abonnement autonome qui n'a besoin d'aucun serveur. Les deux options sont le point d'extension pour la relier à votre propre backend.

## 🗂️ Structure du projet

```
src/
  App.vue                 enveloppe : navigation latérale et les différentes pages
  composables/useApp.ts   centre d'état (session / abonnements / nœuds / connexion / paramètres)
  pages/                  Vue d'ensemble / Nœuds / Abonnements / Connexions / Journaux / Paramètres / Aide
  i18n/                   catalogues de messages
  lib/                    machine à états de connexion, analyse des abonnements, préférences, service TUN
  styles/app.css          jetons de design clairs et sombres
src-tauri/                hôte Tauri (Rust) : fenêtres, barre d'état et cycle de vie du cœur et du helper TUN
```

Le trafic sortant réel est acheminé par le cœur sing-box embarqué.

## 📄 Licence

GPL-3.0, voir [LICENSE](../../LICENSE). Le projet lie le cœur sing-box sous GPL-3.0 ; les distributions doivent donc suivre la même licence et fournir le code source correspondant.
