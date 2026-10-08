# Serveur de test

Un serveur GammaEngine local, reproductible, qui sert aux essais à la main et au banc de la
[phase 0](../../docs/phase-0.md). Il se prépare à partir du dernier build et vit dans
`test-server/` à la racine du dépôt, dossier ignoré par git.

## Préparer

```bash
./gradlew buildPackages
```

Puis, sous Windows :

```powershell
powershell -ExecutionPolicy Bypass -File tools\test-server\setup.ps1 -AcceptEula
```

ou sous Linux :

```bash
tools/test-server/setup.sh --accept-eula
```

`-AcceptEula` (`--accept-eula`) écrit `eula=true` : ne le passer que si vous acceptez le
[CLUF de Minecraft](https://aka.ms/MinecraftEULA). Le script :

* copie le jar le plus récent de `build/distributions/` en `test-server/server.jar` ;
* dépose les bibliothèques de `libraries.zip` avec leurs empreintes MD5, pour que le premier
  démarrage ne télécharge rien ;
* écrit les réglages de test de `config/` là où aucun fichier n'existe encore (`-ResetConfig` les
  réécrit, `-ResetWorld` efface le monde).

Relancer `setup` après un nouveau build met à jour le jar et garde le monde.

## Démarrer

```powershell
powershell -ExecutionPolicy Bypass -File tools\test-server\start.ps1
```

```bash
tools/test-server/start.sh
```

Par défaut : `java` du PATH, 4 Go de tas. Sur Java 9 ou plus récent, le script ajoute les arguments
de `java9args.txt`. Exemple sur Java 21 avec ZGC générationnel et le journal des pauses :

```powershell
powershell -ExecutionPolicy Bypass -File tools\test-server\start.ps1 -Java "$HOME\.jdks\ms-21.0.12.1\bin\java.exe" -Gc zgc -GcLog
```

| Option (`start.ps1` / `start.sh`) | Effet |
| --- | --- |
| `-Gc zgc` / `--gc zgc` | ZGC, Java 15 ou plus ; générationnel sur Java 21 et 22, il l'est par défaut à partir de 23 |
| `-Gc g1` / `--gc g1` | G1, utile pour comparer sur Java 8, dont le GC par défaut est Parallel |
| `-GcLog` / `--gc-log` | Journal GC et safepoints dans `logs/gc-<pid>.log` : JMX arrondit les pauses à la milliseconde |
| `-JvmArgs` / `-- …` | Arguments JVM supplémentaires, ajoutés après les autres |
| `-NoConsole` / `--no-console` | Pas de lecture de la console, pour un serveur en arrière-plan |

## Réglages de test

Seules les clés qui diffèrent des valeurs par défaut sont fournies ; le serveur complète le reste au
premier démarrage.

| Fichier | Réglage | Pourquoi |
| --- | --- | --- |
| `server.properties` | `server-ip=127.0.0.1` | Le serveur n'écoute que sur ce poste : il tourne en mode hors ligne |
| | `online-mode=false` | Les bots n'ont pas de compte Mojang |
| | `allow-flight=true`, `max-players=500`, `player-idle-timeout=0` | Aucune expulsion des bots pour vol, place ou inactivité |
| | `level-seed=gammaengine` | Même monde à chaque génération |
| | `snooper-enabled=false`, `spawn-protection=0` | Pas d'envoi de statistiques, construction libre au spawn |
| `bukkit.yml` | `settings.connection-throttle: -1` | Les bots se connectent tous depuis la même adresse |
| `Gamma.yml` | `thermos.logging.clientModList: false` | Pas une ligne de journal par client avec toute sa liste de mods |
| `ops.json`, `whitelist.json`, `banned-players.json`, `banned-ips.json` | listes vides | Pas quatre piles d'exception `FileNotFoundException` au premier démarrage |

Pour tester un modpack, déposer ses jars dans `test-server/mods/` et `test-server/plugins/`.
Pour ouvrir le serveur à d'autres machines, vider `server-ip` dans `test-server/server.properties`
en gardant à l'esprit que le mode hors ligne laisse entrer n'importe quel pseudo.
