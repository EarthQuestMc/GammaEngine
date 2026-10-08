![GammaEngine](logo.png)

# GammaEngine

GammaEngine est un serveur Minecraft **1.7.10** qui fait tourner ensemble les mods Forge et les
plugins Bukkit. C'est un fork de [Crucible](https://github.com/CrucibleMC/Crucible), lui-même fork de
Thermos, développé pour le serveur EarthQuest et distribué sous licence GPL-3.0.

Objectif : tenir un maximum de joueurs à 20 TPS et démarrer vite, en optimisant puis en
parallélisant le moteur, sans rien demander aux mods ni aux plugins.

## Principes

* Les mods et plugins se déposent dans `mods/` et `plugins/` tels quels : aucun jar tiers n'est
  modifié, toute correction se fait par transformation de bytecode au chargement.
* Le moteur ne connaît pas les mods à l'avance et n'attend aucune liste ni aucun réglage par mod.
* Le code des mods et plugins s'exécute sur le thread propriétaire de son monde, sauf s'il a été
  classé sûr pour le parallèle.
* Le serveur fonctionne sans aucune configuration ; chaque optimisation a un interrupteur dans
  `gammaengine.yml`.
* Tout ce que Crucible supporte continue de fonctionner : mêmes mods, mêmes plugins, mêmes mondes,
  même protocole 1.7.10.

## État du projet

Développement précoce. La couche de mesure est en place ; le banc de test (phase 0) est la
prochaine étape. Voir la [feuille de route](docs/roadmap.md) pour le plan et l'avancement.

## Prérequis

* Java 8 à 21 aujourd'hui ; la phase 1 vise Java 21 avec ZGC.
* Forge 1.7.10-10.13.4.1614, API Bukkit 1.7.10-R0.1-SNAPSHOT.

## Lancer le serveur

```bash
java -Xms4G -Xmx8G -jar GammaEngine-1.7.10-<version>-server.jar nogui
```

Le premier lancement installe les bibliothèques du serveur puis demande un redémarrage. Sur Java 9
ou plus récent, ajouter les arguments de `java9args.txt`.

Fichiers de configuration, tous facultatifs :

| Fichier | Contenu |
| --- | --- |
| `gammaengine.yml` | Interrupteurs du moteur GammaEngine |
| `Gamma.yml` | Réglages hérités de Crucible, migrés depuis `Crucible.yml` |

Commandes :

| Commande | Ce qu'elle affiche | Permission |
| --- | --- | --- |
| `/tps` | TPS sur 5 s à 15 min, durée des ticks, CPU, mémoire, GC, charge et coût par joueur | `bukkit.command.tps` |
| `/mods` | Les mods Forge et les plugins Bukkit chargés : actifs en vert, désactivés en rouge, plugins fournis par un mod en bleu clair | `gamma.mods` |
| `/chunks [dump [all]]` | Chunks, entités et TileEntities par monde ; `dump` écrit le détail dans `chunk-dumps/` | `gamma.chunks` |
| `/findchunks [dimension]` | Les 20 chunks chargés qui portent le plus de TileEntities | `gamma.findChunks` |
| `/heapdump` | Dump du tas dans `dumps/` (le serveur se fige pendant l'écriture) | `gamma.heap` |
| `/autothread` | Profilage, banc synthétique, enregistrement des ticks (`record`), mémoire | `gammaengine.autothread` |

Les anciennes sous-commandes `/gamma` et `/crucible` sont remplacées par ces commandes ;
`/version`, `/plugins` et `/restart` restent celles de Bukkit et de Spigot.

## Compiler

Voir [docs/build.md](docs/build.md). En résumé :

```bash
./gradlew setupCrucible     # une fois, crée le workspace patché
./gradlew buildPackages     # jar serveur dans build/distributions/
```

## Documentation

| Document | Contenu |
| --- | --- |
| [docs/roadmap.md](docs/roadmap.md) | Les phases 0 à 12, les cibles et l'avancement |
| [docs/carte/](docs/carte/README.md) | Carte du code : tick, chunks, réseau, chargement, patches |
| [docs/existant.md](docs/existant.md) | Ce que Crucible fait déjà parmi la feuille de route |
| [docs/phase-0.md](docs/phase-0.md) | Plan détaillé du banc de test |
| [docs/scaling.md](docs/scaling.md) | Où part la consommation à 400 joueurs, chiffres à l'appui |
| [docs/threading-model.md](docs/threading-model.md) | Les règles de threading que tout code du fork respecte |
| [docs/native-engine.md](docs/native-engine.md) | La bibliothèque Rust et son périmètre |
| [docs/build.md](docs/build.md) | Build reproductible |

## Crédits

* [Crucible](https://github.com/CrucibleMC/Crucible) — projet amont
* [Thermos](https://github.com/CyberdyneCC/Thermos) — l'amont de Crucible
* [Spigot](https://hub.spigotmc.org/stash/projects/SPIGOT/repos/spigot/browse) et
  [Paper](https://github.com/PaperMC/Paper) — de nombreuses améliorations sur Bukkit
* [lwjgl3ify](https://github.com/GTNewHorizons/lwjgl3ify) — support Java 9+
