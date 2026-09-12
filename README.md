![GammaEngine](logo.png)

# GammaEngine

GammaEngine est un serveur Minecraft **1.7.10** qui fait tourner ensemble les mods Forge et les
plugins Bukkit, et qui exploite réellement plusieurs cœurs physiques pour simuler le monde.

C'est un fork de [Crucible](https://github.com/CrucibleMC/Crucible), lui-même fork de Thermos. Tout
ce que Crucible supporte continue de fonctionner : les mêmes mods, les mêmes plugins, les mêmes
mondes, le même format de sauvegarde, le même protocole 1.7.10.

Ce que GammaEngine ajoute, c'est le **runtime AutoThread**.

## AutoThread en un paragraphe

Le serveur décide seul de ce qui peut s'exécuter en parallèle. Il n'y a aucun mode de threading à
choisir, pas de `legacy` / `hybrid` / `regionized`, et rien à déclarer pour un auteur de mod ou de
plugin. Le runtime observe ce que le code touche réellement à l'exécution, découpe le monde chargé
en régions qui n'interagissent pas, ticke simultanément les régions indépendantes, et sérialise
tout ce qu'il n'a pas prouvé sûr. Quand un composant pose problème, il perd son parallélisme à la
granularité la plus fine possible, une classe ou un objet, jamais un mod entier.

L'ordre de priorité n'est pas négociable : intégrité du monde, aucune corruption silencieuse,
compatibilité, stabilité, puis performances.

## État du projet

Développement précoce. Le runtime, ses métriques et la bibliothèque native sont en place ; la
simulation parallèle par régions se construit dessus. Voir [docs/roadmap.md](docs/roadmap.md) pour
le plan phase par phase et l'avancement réel.

## Prérequis

* Java 8 à 21 (lwjgl3ify est intégré, comme dans Crucible)
* Forge 1.7.10-10.13.4.1614, API Bukkit 1.7.10-R0.1-SNAPSHOT

## Lancer le serveur

```bash
java -Xms4G -Xmx8G -jar GammaEngine-1.7.10-<version>-server.jar nogui
```

Le premier lancement installe les bibliothèques du serveur puis demande un redémarrage. Sur Java 9
ou plus récent, ajouter les arguments de `java9args.txt`.

Fichiers de configuration, tous optionnels :

| Fichier | Contenu |
| --- | --- |
| `Gamma.yml` | Réglages généraux du serveur, migrés automatiquement depuis `Crucible.yml` |
| `GammaAutoThread.yml` | Limites de ressources et diagnostics AutoThread. Aucun réglage par mod, volontairement |

Commandes : `/gamma` (alias `/crucible`) pour les informations serveur, `/autothread` pour l'état du
runtime, l'occupation des workers, les conflits et le profilage.

## Compiler

Voir [docs/build.md](docs/build.md). En résumé :

```bash
./gradlew setupCrucible     # une fois, crée le workspace patché
./gradlew buildPackages     # jar serveur dans build/distributions/
cd native && cargo build --release   # accélération native, optionnelle
```

## Documentation

| Document | Contenu |
| --- | --- |
| [docs/roadmap.md](docs/roadmap.md) | Les phases, ce que fait chacune, et l'avancement |
| [docs/scaling.md](docs/scaling.md) | Où part la consommation à 400 joueurs, chiffres à l'appui |
| [docs/folia.md](docs/folia.md) | Ce qu'on reprend de Folia et où on doit diverger |
| [docs/architecture.md](docs/architecture.md) | Les sous-systèmes et les chemins de tick repris |
| [docs/threading-model.md](docs/threading-model.md) | Propriété, règles d'accès, verrouillage, le contrat |
| [docs/native-engine.md](docs/native-engine.md) | La bibliothèque Rust, ce qu'elle accélère, les mesures |
| [docs/build.md](docs/build.md) | Build reproductible |

## Crédits

* [Crucible](https://github.com/CrucibleMC/Crucible) — projet amont
* [Thermos](https://github.com/CyberdyneCC/Thermos) — l'amont de Crucible
* [Spigot](https://hub.spigotmc.org/stash/projects/SPIGOT/repos/spigot/browse) et
  [Paper](https://github.com/PaperMC/Paper) — de nombreuses améliorations sur Bukkit
* [lwjgl3ify](https://github.com/GTNewHorizons/lwjgl3ify) — support Java 9+
