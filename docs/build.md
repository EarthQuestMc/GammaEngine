# Compiler GammaEngine

GammaEngine est un fork de Crucible pour Minecraft 1.7.10. Il conserve le build à base de patches de
Crucible : les sources Minecraft et Forge sont décompilées dans un workspace généré, les patches de
`patches/` sont appliqués par-dessus, et le code propre au fork vit dans `src/main/java`.

## Versions visées par ce build

| Composant | Version |
| --- | --- |
| Minecraft | 1.7.10 |
| Mappings MCP | 9.08 (canal stable, pas `snapshot`) |
| Forge | 10.13.4.1614 |
| API Bukkit | 1.7.10-R0.1-SNAPSHOT |
| Gradle | 8.0 (wrapper) |
| JDK de build | Java 8 (`JAVA_HOME` doit pointer sur un JDK 8) |
| Toolchain buildSrc | Java 17, résolue par les toolchains Gradle |
| JDK d'exécution | Java 8 à 21 (lwjgl3ify est intégré) |

Gradle lui-même tourne sur le JDK 8 de `JAVA_HOME` ; seul le plugin `buildSrc` compile avec une
toolchain Java 17, que Gradle localise automatiquement (par exemple `~/.jdks/ms-17.x`). Une machine
qui n'a que le JDK 8 ne peut pas compiler `buildSrc` de zéro.

## Premier build

```bash
export JAVA_HOME=/chemin/vers/jdk8     # Windows : définir JAVA_HOME sur le dossier du JDK 8
./gradlew setupCrucible                # ~2 min : téléchargement, désobfuscation, décompilation, patches
./gradlew buildPackages                # ~1 min : compilation, réobfuscation, empaquetage
```

Les artefacts arrivent dans `build/distributions/` :

* `GammaEngine-<version mc>-<branche>-<hash>-server.jar` — le serveur,
* `libraries.zip` — toutes les dépendances d'exécution, pour les machines qui ne peuvent pas les
  télécharger au démarrage.

Le jar du serveur télécharge ce dont il a besoin au premier lancement puis demande un redémarrage :
c'est le comportement de Crucible amont, il est normal.

## Ce qui se trouve où

| Chemin | Contenu |
| --- | --- |
| `src/main/java` | Code du fork : CraftBukkit, Crucible, et tout `io/github/gammaengine` |
| `src/test/java` | Tests unitaires (JUnit 4), lancés par `./gradlew :eclipse:cauldron:test` |
| `patches/` | Diffs appliqués aux sources décompilées de Minecraft et Forge |
| `eclipse/cauldron/src/main/java` | Workspace généré : sources décompilées puis patchées. Hors git |
| `native/` | Bibliothèque Rust optionnelle |
| `buildSrc/` | Le plugin Gradle `crucible` qui pilote setup, patches et réobfuscation |

Modifier une classe Minecraft ou Forge se fait dans `eclipse/cauldron/src/main/java`, puis on
régénère les diffs :

```bash
./gradlew genPatches
```

Modifier du code appartenant au fork se fait directement dans `src/main/java` ; aucune étape de
patch n'est impliquée. Un nouveau sous-système GammaEngine va donc toujours dans
`src/main/java/io/github/gammaengine`, et le patch appliqué à une classe vanilla doit se réduire à
un appel vers lui.

## Tâches utiles

| Tâche | Rôle |
| --- | --- |
| `./gradlew setupCrucible` | Créer ou réparer le workspace |
| `./gradlew :eclipse:cauldron:compileJava` | Vérification de compilation rapide (~20 s) |
| `./gradlew :eclipse:cauldron:test` | Lancer les tests unitaires |
| `./gradlew buildPackages` | Jar serveur complet et archive des bibliothèques |
| `./gradlew genPatches` | Régénérer `patches/` depuis le workspace |
| `./gradlew clean setupCrucible` | Reconstruire le workspace après un pull amont |

## Bibliothèque native

```bash
cd native
cargo test
cargo build --release
```

Elle est optionnelle : sans elle le serveur fonctionne à l'identique, seulement plus lentement sur
les chemins qu'elle accélère. Voir [native-engine.md](native-engine.md).

## Lancer un serveur de développement

```bash
mkdir -p run && cd run
cp ../build/distributions/GammaEngine-*-server.jar server.jar
echo "eula=true" > eula.txt
java -Xms4G -Xmx8G -jar server.jar nogui     # le premier lancement installe les bibliothèques et s'arrête
java -Xms4G -Xmx8G -jar server.jar nogui
```

Sur Java 9 ou plus récent, ajouter les arguments de `java9args.txt`.
