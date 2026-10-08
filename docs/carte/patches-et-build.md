# Carte du code — système de patches et build

GammaEngine garde le build à base de patches de Crucible. Les sources de Minecraft et de Forge ne
sont pas dans git : elles sont décompilées dans un workspace généré, les diffs de `patches/` sont
appliqués par-dessus, et le code propre au fork vit dans `src/main/java`.

## Vue d'ensemble

Du dépôt au jar serveur :

```
jar serveur Mojang 1.7.10
  └─ mergeJars, deobfuscateJar (MCP 9.08)                    → jar en noms SRG
     └─ decompile (forgeflower)                               → sources brutes
        └─ forgePatchJar : étapes « fml », « forge », « bukkit » → sources Forge + API Bukkit
           ├─ remapCleanJar ─ extractCleanSource              → eclipse/Clean/src/main/java      (référence, sans Crucible)
           └─ cauldronPatchJar : applique patches/            → sources patchées
              └─ remapCauldronJar ─ extractCauldronSources    → eclipse/cauldron/src/main/java   (le moteur réel)
generateProjects                                              → eclipse/Clean et eclipse/cauldron (projets Gradle générés)
```

Le projet généré `eclipse/cauldron` compile ensemble les sources patchées et, par référence directe,
`src/main/java`, `src/main/resources` et `src/test/java` du dépôt (`eclipse/cauldron/build.gradle:142-157`,
chemins absolus écrits par `GenDevProjectsTask`). Modifier `src/main/java` ne demande donc aucune
étape de patch.

Empaquetage : `buildPackages` → `packageServer` (patchs binaires `genBinPatches` calculés depuis le
jar réobfusqué par `obfuscateJar`, plus les ressources FML, Forge et Bukkit) et `packageApi` (jar
d'API dont `uncodeCrucible` a vidé le code des classes Minecraft).

## Tâches Gradle

Toutes sont définies dans `buildSrc/src/main/java/io/github/cruciblemc/forgegradle/CrucibleDevPlugin.java`
sauf mention contraire.

| Tâche | Ligne | Entrées | Sorties | Rôle |
| --- | --- | --- | --- | --- |
| `setupCrucible` | 55 | jar Mojang, `patches/`, `forge/`, `bukkit/` | `eclipse/` | Crée ou répare le workspace |
| `deobfuscateJar` | 80 | jar fusionné | jar SRG | Désobfuscation MCP |
| `decompile` | 93 | jar SRG | zip de sources | Décompilation déterministe |
| `forgePatchJar` | 104 | sources brutes | sources Forge | Patches FML, Forge, ajout de Bukkit |
| `cauldronPatchJar` | 128 | sources Forge remappées | sources patchées | Applique `patches/` |
| `extractCleanSource` / `extractCauldronSources` | 163 / 190 | zips | `eclipse/*/src/main/java` | Extraction du workspace |
| `generateProjects` | 261 | — | `eclipse/Clean`, `eclipse/cauldron` | Projets Gradle générés |
| `genPatches` | 331 | `eclipse/Clean` contre `eclipse/cauldron` | `patches/` | Régénère tous les diffs |
| `obfuscateJar` | 350 | jar de `:eclipse:cauldron` | jar réobfusqué | Retour aux noms Notch |
| `genBinPatches` | 363 | jars propres et réobfusqué | patchs binaires | Ce qui est livré dans le jar |
| `uncodeCrucible` | 378 | `cauldron.jar` | jar sans code Minecraft | Base du jar d'API |
| `packageServer` | 389 | patchs binaires, ressources | `build/distributions/*-server.jar` | Jar serveur |
| `packageApi` | 418 | jar « uncodé » | jar d'API | API pour les plugins |
| `buildPackages` | 68 | — | — | `cleanPackages` + `packageServer` + `packageApi` |
| `packageLibraries` | `build.gradle:181` | configuration `libraries` | `libraries.zip` | Bibliothèques pour les machines hors ligne |
| `:eclipse:cauldron:compileJava` | généré | workspace + `src/main/java` | classes | Vérification rapide (environ 20 s) |
| `:eclipse:cauldron:test` | généré | `src/test/java` | rapports JUnit | Tests unitaires |

## Cycle de travail

* **Modifier une classe Minecraft, Forge ou Bukkit** : éditer `eclipse/cauldron/src/main/java`, puis
  `./gradlew genPatches`, puis ne commiter que les fichiers de `patches/` voulus.
* **Ajouter une classe au moteur** : `src/main/java/io/github/gammaengine/...`, aucune étape de patch.
  Le patch d'une classe vanilla se réduit à un appel vers ce paquet.
* **Compiler et tester** : `./gradlew :eclipse:cauldron:compileJava :eclipse:cauldron:test`.
* **Serveur complet** : `./gradlew buildPackages`, puis copier le jar de `build/distributions/` dans
  `run/` (dossier ignoré par git) et lancer `java -jar server.jar nogui`.

**Un patch par sujet.** `genPatches` produit un fichier par classe et régénère tous les fichiers à
chaque passage. La règle « un patch par sujet » se tient donc au niveau du commit : un commit par
sujet, qui ne contient que les morceaux de `.patch` de ce sujet, chaque ajout marqué
`// GammaEngine` avec sa raison sur la même ligne ou juste au-dessus.

État vérifié le 8 octobre 2026 : le workspace ne contient aucune modification non exportée. Les
cinq fichiers modifiés après l'extraction ont chacun leur patch, et le dernier commit de `patches/`
(12 septembre, 03:40) est postérieur à la dernière modification du workspace (03:36).

## Conventions de marquage

Nombre de fichiers de `patches/` (411 au total) qui contiennent chaque marqueur :

| Marqueur | Fichiers | Origine |
| --- | --- | --- |
| `// CraftBukkit start` … `end` | 189 | CraftBukkit |
| `// Cauldron start` … `end` | 106 | Cauldron (MCPC+) |
| `// Spigot` | 85 | Spigot |
| `Crucible` | 48 | Crucible |
| `Thermos` | 44 | Thermos |
| `KCauldron` | 9 | KCauldron |
| `// GammaEngine` | 5 | ce fork |

Les cinq patches GammaEngine : `MinecraftServer` (points d'accroche du cycle de vie et du tick,
préchargement du spawn), `S21PacketChunkData` et `S26PacketMapChunkBulk` (tampons de compression et
niveau deflate), `org.bukkit.Bukkit` et `VersionCommand` (marque).

Exemple réel, `patches/net/minecraft/server/MinecraftServer.java.patch` :

```java
+        io.github.gammaengine.autothread.AutoThreadRuntime.get().onTickStart(); // GammaEngine
```

## Tests

Cinq classes JUnit 4 dans `src/test/java/io/github/gammaengine/` : `LatencyHistogramTest`,
`TickStatisticsTest`, `ScratchBuffersTest`, `CpuTopologyTest`, `NativeEngineTest` (banc
Java contre Rust, saute la partie native si la bibliothèque est absente). Lancées par
`:eclipse:cauldron:test`. Rien ne teste le serveur démarré, et la CI ne lance pas les tests.

## Bibliothèque native

`native/` est une crate Rust (`cdylib`) : compression et décompression zlib, XXH64, arithmétique des
secteurs de fichiers region. Elle se compile à part (`cargo build --release`), hors de Gradle et de
la CI. Le serveur la cherche dans cet ordre (`NativeEngine.java:52-93`) : propriété
`gammaengine.nativeLibrary`, `java.library.path`, puis `gammaengine/native/` à côté du jar. Le
serveur la charge au démarrage mais ne l'appelle pas encore : seuls les tests s'en servent.

## Intégration continue

| Workflow | Déclencheur | Ce qu'il fait |
| --- | --- | --- |
| `.github/workflows/prerelease-build.yml` | push sur `main` | JDK 8, `setupCrucible`, `buildPackages`, renomme le jar `GammaEngine-1.7.10-main-<sha>-dev-<n>-server.jar`, publie une pré-version taguée `main-<sha>` |
| `.github/workflows/verify-build.yml` | pull request vers `main`, ou à la main | JDK 8, `setupCrucible`, `buildPackages` |

Le premier build sur `main` a réussi et publié le tag `main-1d2a51b`. Manquent : tests unitaires,
build Rust, démarrage du serveur, banc de fumée.

`build.gradle:72` calcule la version : toute branche autre que `master` donne
`1.7.10-<branche>-<hash>`. Il n'y a plus de branche `master`, donc chaque build porte sa branche et
son hash, ce qui convient tant qu'aucune version n'est publiée.

## Pièges pour la suite

* `genPatches` réécrit les 411 fichiers. Avant de commiter, vérifier `git diff --stat patches` : un
  changement de fins de ligne (git signale des conversions LF/CRLF sur ce poste) peut toucher des
  fichiers sans rapport.
* `eclipse/` est hors git et contient des chemins absolus propres à la machine : il se recrée par
  `setupCrucible`, il ne se copie pas.
* La compilation incrémentale est désactivée pour `:eclipse:cauldron` (`build.gradle`, bloc
  `subprojects`) : recompilée seule, une classe Minecraft patchée trouvait le `MinecraftServer`
  obfusqué du jar vanilla avant le bon. L'encodage des sources y est fixé à UTF-8.
* Dans un script, ne pas juger un build sur la dernière ligne d'un `| tail` : c'est le code de
  sortie de Gradle qui compte.
* `.gitignore` ignore `*.sh` et `*.bat` : le script de lancement de la phase 1 et les scripts du banc
  de la phase 0 demandent une exception explicite.
* La CI ne lance ni les tests ni la crate Rust : une régression du moteur passe le build sans bruit.
* Le build tourne sur JDK 8. Écrire du code moteur en Java 21 (phase 1) demande de séparer ce code ou
  de changer la cible de compilation, et rend le serveur inutilisable sur Java 8.
