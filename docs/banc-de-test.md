# Banc de test : mode d'emploi

Le banc de la [phase 0](phase-0.md) joue un scénario de charge de bout en bout et compare deux
exécutions. Tout passe par un seul exécutable, `gamma-bots` ([`tools/bench/`](../tools/bench/README.md)) :

* `gamma-bots run <scénario.toml>` prépare un serveur neuf, le démarre, fait venir les bots,
  chauffe, enregistre chaque tick pendant la mesure, arrête tout et range les fichiers dans
  `bench/results/` ;
* `gamma-bots compare <avant> <après>` écrit le tableau avant et après en Markdown, prêt pour le
  compte rendu d'une tâche ;
* `gamma-bots [options]`, sans sous-commande, lance seulement des bots contre un serveur déjà
  démarré (voir le README des bots).

Les scénarios sont versionnés dans `bench/scenarios/` ; les résultats ne le sont pas.

## Préparer

```sh
./gradlew buildPackages          # build/distributions/*-server.jar et libraries.zip
cd tools/bench
cargo build --release            # target/release/gamma-bots(.exe)
```

Il faut aussi Java (le `java` du PATH, ou celui du scénario), et pour le script de préparation
PowerShell sous Windows ou `bash`, `unzip` et `md5sum` sous Linux.

## Jouer un scénario

Depuis la racine du dépôt :

```sh
tools/bench/target/release/gamma-bots run bench/scenarios/spawn-groupe.toml --accept-eula
```

| Option | Effet |
| --- | --- |
| `--set <table.clé=valeur>` | Remplace une valeur du scénario, répétable : `--set bots.count=100`, `--set server.heap=2G`, `--set server.properties.view-distance=10`, `--set repetitions=3`. La valeur est lue comme en TOML ; si elle n'en est pas (`2G`), elle est prise comme texte |
| `--java <chemin>` | Exécutable Java, à la place de `[server] java` |
| `--accept-eula` | Écrit `eula=true` dans le dossier du serveur. Ne le passer que si vous acceptez le [CLUF de Minecraft](https://aka.ms/MinecraftEULA) ; sans lui, l'exécution s'arrête avant de démarrer le serveur |
| `--results <dossier>` | Dossier parent des résultats, `bench/results/` par défaut |
| `--dry-run` | Affiche le scénario lu et la ligne de commande Java, sans rien toucher |

Déroulement :

1. Lecture du scénario et des `--set` ; une clé inconnue ou mal typée arrête tout, avec son
   numéro de ligne.
2. `java -version`, empreinte SHA-256 du monde de départ s'il y en a un (refus si elle ne vaut pas
   `world_sha256`), commit courant (`git rev-parse --short HEAD`) et présence de modifications non
   commitées.
3. Vérification que le port est libre sur `127.0.0.1`.
4. Dossier du serveur vidé puis préparé par `tools/test-server/setup.ps1` (`setup.sh` sous Linux) :
   le jar le plus récent de `build/distributions/`, les bibliothèques et les réglages de test.
   L'orchestrateur ne vide qu'un dossier qu'il a créé lui-même (fichier `.gamma-orchestrator`) ;
   tout autre dossier non vide est refusé. `[server] jar` remplace ensuite le jar copié.
5. Pour chaque répétition :
   1. le dossier est ramené à l'état d'après la préparation (seuls `libraries/`, `server.jar` et
      `eula.txt` restent : monde, journaux, configurations générées et fichiers des joueurs
      disparaissent), les réglages de `tools/test-server/config/` sont réécrits, puis
      `server-ip=127.0.0.1`, `online-mode=false`, `server-port` et la table
      `[server.properties]` du scénario ; le monde de départ est copié en `world/` ;
   2. le serveur démarre avec ses entrées et sorties sur des tubes : `java <arguments> -jar
      server.jar nogui`, `nogui` en dernier, sans `--noconsole` puisque l'orchestrateur écrit sur
      la console ; toute la sortie va dans `console.log` ;
   3. attente de la ligne `Done (…)! For help` (`startup_timeout`) ;
   4. commandes `[console] at_start` ;
   5. arrivée des bots, dans le processus de l'orchestrateur, au rythme `rate`, puis attente que
      chacun soit en jeu ou ait abandonné (`join_timeout` et `attempts`) ;
   6. commandes `[console] after_arrival` ;
   7. chauffe (`warmup`) ;
   8. `autothread record start <scénario>-rep<n>` (avec `attribution` si demandée) ;
   9. mesure (`duration`) ;
   10. `autothread record stop` ;
   11. arrêt des bots, écriture de `bots.csv` et `bots.txt` ;
   12. `stop`, fermeture de la console, attente de la fin du processus (`stop_timeout`), sinon il
       est tué ;
   13. copie de l'enregistrement, de `logs/latest.log`, des journaux GC et des rapports de plantage.
6. `run.json` est réécrit après chaque répétition, puis une ligne de résultat par répétition est
   affichée.

Le serveur n'écoute que sur `127.0.0.1`, en mode hors ligne, piloté par sa console : ni RCON ni
port de requête. Il tourne dans son propre groupe de processus, donc un Ctrl-C dans la console de
l'orchestrateur ne l'atteint pas : l'orchestrateur arrête les bots, envoie `stop`, attend la fin
du serveur et range ce qui existe. Sous Windows, le serveur appartient en plus à un objet job qui
le tue si l'orchestrateur meurt sans avoir pu l'arrêter.

Code de sortie : 0 si chaque répétition a produit son enregistrement, 1 si l'une a échoué ou si
l'exécution a été interrompue, 2 pour une erreur avant le premier démarrage (scénario, Java, port,
préparation). Des bots expulsés ne sont pas une erreur de l'orchestrateur : c'est un résultat,
dans `bots.csv` et dans la comparaison.

## Les scénarios

| Fichier | Charge | Défauts |
| --- | --- | --- |
| `spawn-groupe.toml` | Bots immobiles au point d'apparition : coût fixe par joueur, tracking, envoi des chunks | 20 bots, 1 Go, distance de vue 8 |
| `disperses.toml` | Bots répartis par `spreadplayers 0 0 64 384 false {bots}` une fois en jeu, puis en errance autour de leur point d'arrivée : chunks chargés, tick par chunk | 20 bots, 1 Go, distance de vue 6, chauffe 60 s |
| `exploration.toml` | Bots en vol en ligne droite au-dessus de terrain neuf : chargement, génération et envoi de chunks | 10 bots, 2 Go |

Les défauts tiennent sur un poste de développement ; les points de référence du plan (0, 50, 100,
200 et 400 bots, trois répétitions) se jouent avec `--set bots.count=… --set repetitions=3`. Pour
`disperses`, la génération des chunks aux nouveaux emplacements a lieu pendant la chauffe : elle
doit être assez longue pour que la mesure ne la contienne pas.

### Format

Un sous-ensemble de TOML : tables `[a]` et `[a.b]`, clés nues ou entre guillemets, chaînes
`"…"` (avec échappements) et `'…'`, entiers, flottants, booléens, tableaux sur une ou plusieurs
lignes, commentaires `#`. Les tables en ligne, tableaux de tables, dates, chaînes sur plusieurs
lignes et clés pointées sont refusés. Toutes les clés sont facultatives ; une clé ou une table
inconnue est une erreur.

| Clé | Défaut | Rôle |
| --- | --- | --- |
| `name` | nom du fichier | Nom du scénario, dans les noms de dossiers : lettres, chiffres, `-` et `_` |
| `description` | vide | Texte libre, recopié dans `run.json` |
| `repetitions` | 1 | Nombre de répétitions, de 1 à 100 ; chacune repart d'un serveur et d'un monde neufs |
| **`[server]`** | | |
| `dir` | `test-server-orch` | Dossier du serveur, relatif à la racine du dépôt ; vidé à chaque exécution (déjà ignoré par git via `/test-server*/`) |
| `jar` | vide | Jar à utiliser ; vide : le plus récent de `build/distributions/` |
| `java` | `java` | Exécutable Java ; `~/` est développé, un chemin relatif part de la racine du dépôt |
| `heap` | `1G` | `-Xms` et `-Xmx` |
| `gc` | vide | `""` (choix de la JVM : Parallel sur Java 8, G1 ensuite), `"g1"` ou `"zgc"` (Java 15 ou plus, générationnel ajouté sur 21 et 22) |
| `gc_log` | `false` | Journal GC et safepoints dans `logs/gc-<pid>.log`, recopié dans les résultats |
| `jvm_args` | `[]` | Arguments JVM ajoutés après les autres ; `-Xms` et `-Xmx` y sont refusés. Sur Java 9 ou plus, `@java9args.txt` est ajouté comme dans `start.ps1` |
| `port` | 25570 | Port du serveur, sur `127.0.0.1` |
| `world` | vide | Monde de départ : un dossier contenant `level.dat`, copié en `world/` avant chaque démarrage ; vide : monde généré à partir de la graine des réglages de test |
| `world_sha256` | vide | Empreinte attendue du monde (affichée et écrite dans `run.json` à chaque exécution) ; une autre empreinte arrête l'exécution |
| `startup_timeout` | 300 | Secondes d'attente de la ligne `Done` |
| `stop_timeout` | 120 | Secondes laissées au serveur pour s'arrêter avant d'être tué |
| **`[server.properties]`** | | Clés de `server.properties` écrites par-dessus les réglages de test, par exemple `view-distance = 6`. `server-ip`, `online-mode` et `server-port` sont refusées : l'orchestrateur les fixe |
| **`[bots]`** | | |
| `count` | 20 | Nombre de bots ; 0 pour mesurer le serveur sans joueur |
| `rate` | 5 | Connexions par seconde ; 0 : toutes d'un coup |
| `behaviour` | `idle` | `idle`, `wander` ou `explore` (voir le README des bots) |
| `prefix`, `first` | `bot`, 1 | Noms `<prefix><n>` |
| `seed` | 1 | Graine des trajets et des directions |
| `wander_radius`, `wander_height` | 16, 0 | Errance, en blocs |
| `fly_height` | 100 | Hauteur de vol des explorateurs au-dessus du point d'apparition du monde |
| `join_timeout`, `attempts` | 30, 3 | Abandon d'une connexion qui n'entre pas en jeu, connexions par bot |
| **`[measure]`** | | |
| `warmup` | 30 | Secondes entre l'entrée en jeu des bots (commandes `after_arrival` comprises) et le début de l'enregistrement |
| `duration` | 60 | Secondes d'enregistrement |
| `attribution` | `false` | Niveau 2 : temps par classe, mod et chunk (`mods.csv`, `chunks.csv`). Son coût fausse le MSPT : comparer niveau 2 contre niveau 2 seulement |
| **`[console]`** | | |
| `at_start` | `[]` | Commandes envoyées à la console après `Done`, avant les bots. Les scénarios fournis figent l'heure et la météo |
| `after_arrival` | `[]` | Commandes envoyées une fois les bots en jeu, avant la chauffe. `{bots}` y devient la liste des noms des bots en jeu, séparés par des espaces ; une commande qui le contient n'est pas envoyée s'il n'y a aucun bot. `@a` ne convient pas : `spreadplayers … @a` envoyé depuis la console de ce serveur n'a déplacé personne, sans aucun message |

L'empreinte d'un monde couvre, dans l'ordre des chemins, le chemin relatif, la taille et le
contenu de chaque fichier, sauf `session.lock` que le serveur réécrit à chaque démarrage. Pour la
connaître, lancer une fois `run --dry-run` avec `world` renseigné : elle est affichée.

## Les résultats

`bench/results/<aaaaMMjj-HHmmss>_<scénario>_<commit>/`, date et heure UTC du lancement, commit
court de `HEAD` (le jar mesuré est nommé dans `run.json`) :

| Fichier | Contenu |
| --- | --- |
| `run.json` | Ce qui a été joué : scénario et `--set`, dates, commit et modifications non commitées, jar et son SHA-256, Java, ligne de commande complète, tas, GC, port, monde et son empreinte, réglages, bots, chauffe, mesure, attribution, commandes ; puis par répétition : dates, durée du démarrage, dossier de l'enregistrement côté serveur, bots en jeu au début et à la fin de la mesure, fin du serveur (code de sortie ou « killed »), erreur |
| `scenario.toml` | Le fichier du scénario, précédé des `--set` en commentaire |
| `command.txt` | Dossier et ligne de commande exacte du serveur |
| `java-version.txt` | Sortie de `java -version` |
| `rep<n>/summary.json`, `ticks.csv`, `gc.csv` | L'enregistrement du serveur (format dans le [plan](phase-0.md), section « La sonde serveur et l'export ») |
| `rep<n>/mods.csv`, `chunks.csv` | Avec l'attribution seulement |
| `rep<n>/bots.csv`, `bots.txt` | Une ligne par bot (colonnes dans le README des bots) et le résumé affiché à la fin |
| `rep<n>/console.log` | Toute la sortie du serveur, avec les commandes envoyées (`> commande`) |
| `rep<n>/latest.log`, `gc-<pid>.log`, `crash-reports/` | Journal du serveur, journal GC si demandé, rapports de plantage s'il y en a |

## Comparer

```sh
tools/bench/target/release/gamma-bots compare bench/results/<avant> bench/results/<après> --output comparaison.md
```

Chaque côté est un dossier de résultats (toutes ses répétitions), un dossier `rep<n>`, un
enregistrement brut du serveur (`gammaengine/bench/<nom>-<date>/`), ou plusieurs d'entre eux
séparés par des virgules. Une répétition sans `summary.json` (échouée) est ignorée. Avec
plusieurs répétitions, une case donne la médiane puis, entre parenthèses, la plus petite et la plus
grande valeur ; l'écart et le pourcentage portent sur les médianes. Le tableau est écrit sur la
sortie standard, et dans le fichier de `--output`.

Une première table rappelle ce qui a été joué de chaque côté (scénario, commit, jar, Java,
arguments JVM, bots, comportement, durée de mesure, attribution) et marque `≠` les lignes qui
diffèrent. La seconde donne : MSPT moyen, p50, p95, p99 et maximum, TPS, coût par joueur (MSPT
moyen divisé par le nombre moyen de joueurs), joueurs moyens et maximum, pauses GC (nombre, total,
maximum), cycles GC concurrents (nombre, total), ticks mesurés, et côté bots : prévus, en jeu à la
fin, expulsés, en échec (erreur réseau ou jamais entrés en jeu). Une mesure absente des deux côtés
est omise ; `n/d` marque une valeur absente d'un côté. Si l'attribution n'est pas la même des deux
côtés, un avertissement suit le tableau.

Extrait, 10 bots (une répétition) contre 20 (deux répétitions), immobiles, 30 s de mesure, sur le
poste de développement le 8 octobre 2026 :

| Mesure | Avant | Après | Écart | Écart (%) |
| --- | ---: | ---: | ---: | ---: |
| MSPT moyen (ms) | 3,63 | 6,28 (5,46 – 7,09) | +2,65 | +72,9 % |
| MSPT p95 (ms) | 5,39 | 10,72 (9,38 – 12,06) | +5,33 | +98,7 % |
| Coût par joueur (ms) | 0,363 | 0,314 (0,273 – 0,355) | -0,049 | -13,6 % |
| Pauses GC | 2 | 2,5 (2 – 3) | +0,5 | +25,0 % |
| Bots en jeu à la fin | 10 | 20 (20 – 20) | +10 | +100,0 % |

L'étendue des deux répétitions de 20 bots (MSPT moyen de 5,46 à 7,09 ms) rappelle qu'une seule
exécution ne suffit pas à conclure : jouer trois répétitions au moins.

## Limites

* Les bots tournent dans le processus de l'orchestrateur, sur la même machine que le serveur : ils
  lui prennent du CPU (deux threads par bot). Les chiffres absolus ne valent que pour une même
  machine ; le plan prévoit à terme des bots sur une autre machine.
* La chauffe part du moment où chaque bot est en jeu ou a abandonné ; `run.json` donne le nombre
  de bots en jeu au début et à la fin de la mesure.
* La préparation prend le jar le plus récent de `build/distributions/` au moment du lancement :
  un build lancé en parallèle peut changer le jar d'une exécution à l'autre. Le nom et le SHA-256
  du jar sont dans `run.json`, et `[server] jar` fige le jar.
* Le monde de départ n'est que l'Overworld (`world/`) ; le Nether et l'End sont regénérés. Les
  mods et plugins d'un modpack ne sont pas encore copiés par le scénario : il faudra une clé pour
  le scénario EarthQuest.
* Le niveau 3 (JFR) et les scénarios `base-industrielle`, `mobs`, `tile-entities`,
  `edition-en-masse` et `connexions` ne sont pas encore écrits.
* Sous Linux, rien ne tue le serveur si l'orchestrateur est tué brutalement (`kill -9`) : le
  Ctrl-C et `SIGTERM` passent par l'arrêt normal, mais pas un arrêt forcé.
* Le dossier de l'enregistrement est lu dans la réponse de la console à `autothread record` ; si
  la réponse change de texte côté serveur, l'orchestrateur ne la reconnaît plus.
* Les dates des noms de dossiers sont en UTC, celles des dossiers d'enregistrement du serveur à
  l'heure locale.
