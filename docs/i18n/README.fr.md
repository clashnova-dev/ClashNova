# ClashNova Desktop

<!-- i18n-nav -->
[English](../../README.md) · [简体中文](README.zh-CN.md) · [Deutsch](README.de.md) · **Français** · [日本語](README.ja.md) · [한국어](README.ko.md) · [Русский](README.ru.md)

> Les traductions peuvent être en retard sur le document principal en anglais.

Un client proxy de bureau fondé sur [sing-box](https://sing-box.sagernet.org/), écrit avec Vue 3 et Tauri 2 pour macOS, Windows et Linux. Importez un abonnement et vous êtes connecté — le cœur est compilé depuis les sources et embarqué dans l'application, si bien que l'analyse, le routage et la connexion sortante se font entièrement sur votre machine.

Comme le cœur sing-box embarqué est compilé depuis les sources (GPL-3.0), le projet dans son ensemble est publié sous **GPL-3.0**.

<p align="center">
  <img src="../screenshots/overview-light.jpg" width="760" alt="Vue d'ensemble">
</p>

## Fonctionnalités

- **Vue d'ensemble** — connexion ou déconnexion en un clic, avec l'état de la connexion en direct, le nœud actuel, le débit montant/descendant en temps réel et la consommation de la session. La consommation de l'abonnement et l'IP de sortie avec sa géolocalisation s'affichent sur place (masquables, actualisées manuellement ou par minuterie), à côté du basculement rapide entre vos nœuds habituels.
- **Nœuds** — recherche par nom, tri par latence, ou *Tout tester* pour mesurer chaque nœud de façon asynchrone ; les résultats arrivent au fil de l'eau et se retrient par latence la plus basse. Choisissez un nœud à la main, ou laissez la sélection automatique (le groupe `urltest` du cœur retient le plus rapide).
- **Abonnements** — importez des abonnements Clash, sing-box ou universels (liens de partage) depuis une URL ou un fichier local ; la conversion se fait localement plutôt que via un service en ligne. Chacun s'actualise, se copie ou se supprime, et sa consommation et son échéance s'affichent lorsque le fournisseur les communique.
- **Connexion et routage** — **proxy système**, **proxy manuel** ou **TUN** (une interface réseau virtuelle, qui nécessite une autorisation système). Basculez entre routage **par règles** et **global**, et configurez le port local, l'accès au réseau local, etc. Une liste des connexions actives, le journal en direct du cœur et des statistiques de trafic sont disponibles.
- **Intégration système** — barre d'état système, lancement au démarrage, et thèmes clair / sombre / selon le système. L'interface est disponible en chinois simplifié, anglais, 日本語, 한국어, Deutsch, Français et Русский.

## Captures d'écran

| Nœuds | Abonnements | Réseau | Paramètres |
| :-: | :-: | :-: | :-: |
| <img src="../screenshots/nodes.jpg" width="200"> | <img src="../screenshots/subscriptions.jpg" width="200"> | <img src="../screenshots/network.jpg" width="200"> | <img src="../screenshots/settings.jpg" width="200"> |

<p align="center">
  <img src="../screenshots/overview-dark.jpg" width="540" alt="Vue d'ensemble (sombre)">
</p>

## Compilation

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

## Configuration

Les options de compilation sont lues par Vite depuis un fichier `.env` (ignoré par `.gitignore`) :

```
VITE_DEFAULT_SERVER=     # URL de backend optionnelle ; vide = client d'abonnement autonome
VITE_ENABLE_LOGIN=false  # à false, l'écran de connexion au compte est masqué
```

Laissée à ses valeurs par défaut, l'application est un client d'abonnement autonome qui n'a besoin d'aucun serveur. Les deux options sont le point d'extension pour la relier à votre propre backend.

## Structure du projet

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

## Licence

GPL-3.0, voir [LICENSE](../../LICENSE). Le projet lie le cœur sing-box sous GPL-3.0 ; les distributions doivent donc suivre la même licence et fournir le code source correspondant.
