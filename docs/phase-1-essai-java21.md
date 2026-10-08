# Phase 1 : premier essai réel sur Java 21 (ZGC générationnel)

Date de l'essai : 8 octobre 2026, branche `dev`, HEAD `a72db610`.

En bref : le serveur démarre et tient le banc sur Java 21 avec ZGC générationnel, sans exception ni
avertissement du système de modules. ZGC tient « pauses sous 10 ms » (au plus 4,15 ms, dont
0,25 ms de travail réel). Le MSPT ne départage pas Java 8, Java 21 + ZGC et Java 21 + G1 sur ce
monde vide. Les corrections déjà faites sont listées à la fin, dans « Suites données ».

## Avertissement sur la portée des chiffres

Le monde est vide (graine `gammaengine`, aucun joueur) et sans mod : `mods/` est vide, les cinq « mods » listés par FML sont `mcp`, `Crucible`, `FML`, `Forge` et `kimagine` (interne). La charge vient uniquement du banc synthétique (`/autothread bench`, 289 chunks, 800 entités, 800 TileEntities, 1200 ticks). **Les chiffres servent à comparer les trois configurations entre elles, pas de référence** : la phase 1 demande le modpack d'EarthQuest, qui n'a pas été essayé ici.

Le jar testé est `GammaEngine-1.7.10-dev-f5f1010f-server.jar`, plus ancien que HEAD : la commande `/tps` détaillée (`a72db610`) n'y figure pas, `/tps` répond encore la ligne Bukkit à trois valeurs.

## Conditions

* Tas de 2 Go (`-Xms2G -Xmx2G`), un seul serveur à la fois, port 25567, dossiers `test-server-j8/` et `test-server-j21/` (supprimés à la fin).
* Scénario de chaque essai : attendre « Done », 5 s, `autothread record start <nom>`, `autothread bench start chunks=8 entities=800 tiles=800 ticks=1200 name=<nom>`, attendre le rapport, `autothread record stop`, `tps`, `stop`.
* Configurations :
  * **Java 8** : 1.8.0_491, arguments par défaut. Attention, le GC par défaut de Java 8 sur cette machine est **Parallel** (`PS Scavenge`), pas G1.
  * **Java 21 ZGC** : 21.0.12.1, `@java9args.txt -XX:+UseZGC -XX:+ZGenerational`.
  * **Java 21 G1** : 21.0.12.1, `@java9args.txt`, GC par défaut (G1).
  * **Java 8 G1** (ajout) : 1.8.0_491 avec `-XX:+UseG1GC`, pour séparer l'effet du GC de celui de la JVM.
* Répétitions : 4 essais Java 8, 6 Java 21 ZGC, 5 Java 21 G1, 2 Java 8 G1, entrelacés dans le temps. Sur Java 21, `-Xlog:gc` est actif dès l'essai `j21g1` (G1) et `z2` (ZGC), `-Xlog:safepoint` à partir de `z4` et `g4`, pour mesurer les pauses au dixième de milliseconde : JMX ne donne que des millisecondes entières. Dans le tableau, la « pause max, journal JVM » de ZGC vient de `-Xlog:safepoint` (pause totale, temps d'accès au safepoint compris) ; celle de G1 vient des lignes `Pause` de `-Xlog:gc` sur toute la vie du processus, démarrage compris.
* **La machine est chargée** (jeux, Discord, CurseForge, autre serveur de test sur 25565). Le bruit d'un essai à l'autre est du même ordre que l'écart entre configurations. Deux essais ont visiblement été perturbés (démarrage à 26 s pour `z4` et `g4`, `g4` a un tick à 141 ms) ; ils sont gardés dans les plages, la médiane les absorbe.
* Les percentiles du tableau sont recalculés sur les 1200 ticks mesurés de `ticks.csv` (rang le plus proche), après les 100 ticks de chauffe du banc. Ils concordent avec le rapport du banc (qui passe par un histogramme) à quelques centièmes près. Le `summary.json` couvre tout l'enregistrement (≈ 71 s, dont l'installation de la charge et la chauffe) : son MSPT max est plus haut, il n'est pas repris ici.

## Tableau comparatif

Valeurs : médiane [minimum – maximum] sur les essais.

| | Java 8 (Parallel), 4 essais | Java 21 ZGC générationnel, 6 essais | Java 21 G1, 5 essais | Java 8 G1, 2 essais |
| --- | --- | --- | --- | --- |
| Démarrage à chaud, « Startup complete in » (s) | 15,3 [14,2 – 22,4] | 16,5 [14,7 – 26,1] | 15,8 [15,3 – 26,9] | 15,3 [14,3 – 16,2] |
| Premier démarrage, avec génération du monde (s) | 27,5 | 28,1 | non mesuré | non mesuré |
| TPS du banc (1200 ticks) | 19,96 [19,93 – 19,98] | 19,88 [19,87 – 19,91] | 19,90 [19,82 – 19,90] | 19,95 [19,93 – 19,97] |
| MSPT moyen (ms) | 4,17 [3,40 – 4,59] | 3,85 [3,29 – 4,38] | 3,71 [3,20 – 5,15] | 4,60 [4,21 – 4,99] |
| MSPT p95 (ms) | 7,7 [6,0 – 10,6] | 7,5 [6,3 – 8,3] | 6,9 [5,6 – 12,2] | 8,7 [7,4 – 10,0] |
| MSPT p99 (ms) | 12,7 [7,2 – 19,1] | 12,2 [7,6 – 14,7] | 8,6 [6,6 – 30,6] | 11,8 [10,1 – 13,4] |
| MSPT max (ms) | 43,1 [12,8 – 47,7] | 39,4 [20,5 – 49,8] | 22,9 [9,0 – 141,2] | 36,0 [27,5 – 44,6] |
| Nombre de pauses GC (enregistrement ≈ 71 s) | 3 | 13 à 18 (pauses seules) | 1 à 3 | 2 |
| Pause GC max, JMX (ms entiers) | 7 [4 – 11] | 0 à 1 | 7 [3 – 18] | 17 à 20 |
| Pause GC max, journal JVM (ms) | non mesuré | 0,30 / 2,89 / 4,15 (3 essais) | 6,6 – 18,1 (5 essais) | non mesuré |
| Cycles GC concurrents | sans objet | 3 à 4 par essai, 103 à 298 ms chacun | 0 dans la fenêtre | sans objet |
| CPU du processus (ms-cœur par tick) | 5,2 [5,0 – 7,0] | 5,7 [5,0 – 6,7] | 4,7 [4,0 – 6,5] | 6,3 [5,6 – 7,0] |

### Lecture

1. **Le MSPT ne distingue pas les configurations.** Les plages se recouvrent presque entièrement ; les médianes (3,7 à 4,2 ms de moyenne) sont dans le bruit de la machine. ZGC ne coûte rien de mesurable sur le MSPT moyen. Sur le p99, G1 a une médiane plus basse (8,6 contre 12,2 ms pour ZGC) mais avec un essai à 30,6 ms : avec ces échantillons on ne peut pas conclure. Le CPU par tick suggère un surcoût de ZGC d'environ 10 à 20 % sur G1 (processus entier, JIT et threads GC compris), indicatif seulement.
2. **Les ticks lents ne viennent presque jamais du GC.** Sur les 71 ticks de plus de 20 ms relevés pendant les mesures (Java 8 Parallel 20, ZGC 20, G1 31), un seul contient une pause GC de 3 ms ou plus (`g5`, tick de 21,7 ms avec la pause G1 de 18 ms), à ±60 ms près. Java 8 avec G1 fait moins bien : 3 ticks lents sur 8 contiennent une pause G1 de 17 à 20 ms. Le MSPT max est donc surtout du bruit de machine ou de chauffe du JIT.
3. **Les pauses : ZGC tient le critère « sous 10 ms », G1 ne le tient pas toujours.** Pour ZGC, le travail réellement fait pendant les pauses (`At safepoint`) est de 0,02 à 0,25 ms ; la pause totale la plus longue est 0,30 ms (`z5`), 2,89 ms (`z4`) et 4,15 ms (`z6`). Les deux dernières sont dues au temps pour atteindre le safepoint (`Reaching safepoint` 2,85 et 4,10 ms), c'est-à-dire à la machine chargée, pas au GC. G1 monte à 17,7 – 18,1 ms dans deux essais, Parallel (Java 8) à 11 ms ; les pauses de G1 et de Parallel grandiront avec un vrai tas de plusieurs dizaines de Go, celles de ZGC non (propriété de conception de ZGC, non mesurée ici).
4. **Le démarrage est équivalent** (à 1 ou 2 s près, dans le bruit). Premier démarrage : 27,5 s contre 28,1 s.
5. **TPS du banc** : 19,88 à 19,90 en Java 21 contre 19,96 en Java 8, soit 0,4 % d'écart, reproductible, quel que soit le GC (Java 8 + G1 : 19,95). L'intervalle moyen entre deux ticks est de 50,25 à 50,36 ms sur Java 21 contre 50,06 à 50,18 ms sur Java 8. La boucle de tick Spigot dort avec `Thread.sleep(wait / 1000000)`; l'hypothèse est une granularité de `Thread.sleep` différente sous Windows, non vérifiée. Sans conséquence sous charge (le tick est alors borné par son travail), mais l'horloge du monde avance 0,4 % plus lentement quand le serveur est au repos.

### Détail des pauses ZGC : cycles et pauses séparés

Beans JMX observés : `ZGC Minor Cycles`, `ZGC Minor Pauses`, `ZGC Major Cycles`, `ZGC Major Pauses`. Un essai type (`z5`) donne 3 pauses Minor + 15 pauses Major, soit 18 pauses d'au plus 1 ms chacune, et 4 cycles (1 Minor de 108 ms, 3 Major dont le plus long fait 269 ms), concurrents au serveur, qui ne le bloquent pas. Le `summary.json` de ZGC annonce pourtant `"gc": {"count": 16 à 22, "total_ms": 628 à 814, "max_ms": 248 à 298}` : c'est le **cycle concurrent** qui est compté comme une « pause ». Voir la correction 4.

Déclencheurs vus dans le journal : `Warmup` (3 collections majeures au démarrage), `Allocation Rate`, `Proactive`, `Metadata GC Threshold` (collection majeure de 0,175 s à 14 s de JVM, pendant le démarrage). Aucun `Allocation Stall`. Avec un tas de 2 Go, le premier cycle Minor se déclenche à 82 % d'occupation (1,7 Go avant, 0,28 Go après) : le « Heap used » du rapport est donc inutilisable comme mesure de consommation sous ZGC (déchets flottants).

## Erreurs et avertissements au démarrage

Aucun des motifs `IllegalAccess`, `InaccessibleObject`, `module`, `NoSuchMethod`, `NoClassDef` ni aucune pile d'exception n'apparaît sur Java 21 en dehors des lignes ci-dessous. Tous les essais se terminent proprement (`stop`, code de sortie 0). Les mods internes, le chargement des 71 bibliothèques, lwjgl3ify (« Dynamicized enum », « Unfinalized 171 Holder fields ») et le moteur AutoThread fonctionnent comme sur Java 8.

| # | Ligne de log | Java 8 ? | Gravité | À faire |
| --- | --- | --- | --- | --- |
| 1 | `OpenJDK 64-Bit Server VM warning: Ignoring option --illegal-access=warn; support was removed in 17.0` (ligne 1 de la console) | non | bénin, bruit | correction 1 |
| 2 | `WARNING: A terminally deprecated method in java.lang.System has been called` suivi de `WARNING: System::setSecurityManager has been called by cpw.mods.fml.common.launcher.FMLTweaker (file:/…/server.jar)`, `WARNING: Please consider reporting this to the maintainers of cpw.mods.fml.common.launcher.FMLTweaker`, `WARNING: System::setSecurityManager will be removed in a future release` | non | bénin sur 21, **bloquant sur Java 24 et plus** | correction 2 |
| 3 | `[Server thread/WARN] [STDERR]: 13 [Server thread] INFO io.netty.util.internal.PlatformDependent - Your platform does not provide complete low-level API for accessing direct buffers reliably. Unless explicitly requested, heap buffer will always be preferred to avoid potential system unstability.` | non | **dégradation possible du réseau**, à mesurer | correction 3 |
| 4 | Bannière affichée avec des `?` à la place des caractères de bloc (`[96m ???????  ?????? ????   ????????` …) quand la sortie est redirigée vers un fichier | non (bannière ASCII) | cosmétique | correction 5 |
| 5 | `2026-10-08 13:51:59,143 WARN Unable to instantiate org.fusesource.jansi.WindowsAnsiOutputStream` (5 occurrences) | oui, identique | bénin, sortie redirigée | aucune |
| 6 | `[Server thread/WARN] [net.minecraft.server.dedicated.DedicatedPlayerList]: Failed to load user banlist:` puis `java.io.FileNotFoundException: banned-players.json` (idem `banned-ips.json`, `ops.json`, `whitelist.json`) | oui, identique | premier démarrage seulement | correction 11 (outillage) |
| 7 | `**** SERVER IS RUNNING IN OFFLINE/INSECURE MODE!` | oui | voulu (`online-mode=false` du serveur de test) | aucune |

Non exercés mais chargés sans message : Nashorn embarqué (`nashorn-core-15.4.jar`, `--add-modules jdk.dynalink`), `java.sql.rowset`, `koloboke-*-jdk8`, `jaxb`/`jakarta`. Le patch de `JarDiscoverer` ignore déjà `org/openjdk/nashorn` et `META-INF/versions`.

## Corrections à faire pour la phase 1

Classées par importance. Chaque ligne donne le fichier probablement concerné.

1. **Retirer `--illegal-access=warn`** des arguments pour les JVM 17 et plus : l'option est ignorée depuis 17 et produit l'avertissement n° 1. Fichier : `java9args.txt` (ligne 1), ou séparer en `java9args.txt` (9 à 16) et un second fichier pour 17 et plus, choisi par `tools/test-server/start.ps1` et `start.sh`. La cible du projet étant Java 8 puis 21 (`Lwjgl3ifyGlue.checkJava` annonce « Java 8-21 »), supprimer la ligne est le plus simple ; Java 9 à 16 n'ont pas été essayés.
2. **Préparer la sortie du `SecurityManager` de FML.** Sur Java 21, `-Djava.security.manager=allow` (déjà dans `java9args.txt`) est indispensable et ne coûte qu'un avertissement. Le JEP 486 (Java 24) supprime définitivement le `SecurityManager` : `System.setSecurityManager` lève `UnsupportedOperationException`, que `FMLTweaker` n'attrape pas (il n'attrape que `SecurityException`), et l'option `allow` n'y est plus acceptée (d'après le JEP 486, de mémoire). Cela n'a **pas été essayé ici** (pas de JDK 24 ou 25 disponible). Il faudra remplacer `FMLSecurityManager`, qui sert à intercepter `System.exit` des mods, par autre chose (transformer d'octets qui neutralise `System.exit`/`Runtime.exit` hors FML, par exemple). Fichiers : `patches/cpw/mods/fml/common/launcher/FMLTweaker.java.patch` (appel `System.setSecurityManager`), `forge/fml/src/main/java/cpw/mods/fml/relauncher/FMLSecurityManager.java`. À planifier avant « Java 25 à évaluer plus tard », pas pour la 21.
3. **Mesurer puis traiter Netty 4.0.10 sur Java 9 et plus.** Le Netty 4.0.10 de Minecraft 1.7.10 est embarqué dans `libraries/net/minecraft/server/1.7.10/server-1.7.10.jar` ; sur Java 9+ il ne trouve plus l'API de bas niveau qu'il attend (probablement `sun.misc.Cleaner`, cause non vérifiée) et préfère des tampons sur le tas (avertissement n° 3, absent sur Java 8). Pour 400 joueurs c'est une copie de plus par envoi et plus d'allocations. Un essai avec les bots de `tools/bench/` sur Java 8 puis Java 21, avec un nombre de joueurs significatif, dira si le coût est réel. Si oui : remplacer ou corriger `PlatformDependent`/`PlatformDependent0` (transformer au chargement dans `src/main/java/io/github/crucible/…`), ou embarquer un Netty plus récent compatible avec l'API 4.0 que Forge utilise (déclaration actuelle : `forge/fml/jsons/1.7.10.json`, `build.gradle` pour la liste `libraries`). Risque élevé de compatibilité avec les mods : ne le faire qu'après mesure. Dans tous les cas, filtrer cette ligne du flux STDERR.
4. **Séparer cycles et pauses dans les métriques GC.** Sous ZGC, le `summary.json` compte les cycles concurrents comme des pauses (16 à 22 « pauses », 628 à 814 ms, max 298 ms, alors que les pauses réelles font moins de 1 ms). Le rapport du banc (`GC: 18 collection(s), 636 ms`) et `ServerHealth` ont le même défaut. Règle : un bean dont le nom se termine par `Cycles` est concurrent, `Pauses` est un arrêt du monde (pour G1, `G1 Concurrent GC` est concurrent). Fichiers : `src/main/java/io/github/gammaengine/metrics/Recording.java` (`onGarbageCollection`, vers les lignes 190 à 215, plus l'écriture JSON `"gc"` vers la ligne 379), `src/main/java/io/github/gammaengine/bench/WorldBenchmark.java` (`gcCount()` et `gcMillis()`, lignes 317 à 337), `src/main/java/io/github/gammaengine/diag/ServerHealth.java` (lignes 90 à 92). Comme `GcInfo.getDuration()` est en millisecondes entières, les pauses ZGC s'écrasent à 0 ou 1 ms : pour vérifier « pauses sous 10 ms » au dixième de milliseconde, le banc doit ajouter `-Xlog:safepoint:file=…` aux arguments de la JVM et lire le journal (lignes `Safepoint "ZMarkEndYoung" … Total: … ns`).
5. **Encodage de la console sur Java 19 et plus.** `ConsoleBanner` choisit la bannière Unicode d'après `file.encoding` (ligne 140), que `java9args.txt` force à UTF-8, alors que `System.out` suit `stdout.encoding` (page de code native tant que rien ne le fixe) : d'où les `?`. Ajouter `-Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8` à `java9args.txt` : vérifié, la bannière s'affiche correctement dans le fichier de log. Dans un terminal Windows interactif il faut en plus un `chcp 65001` dans le lanceur, non vérifié. Fichiers : `java9args.txt`, `src/main/java/io/github/gammaengine/util/ConsoleBanner.java` (tester `stdout.encoding` en priorité).
6. **`-XX:+ZGenerational` à n'ajouter qu'aux JVM 21 et 22.** L'option est dépréciée en 23 et retirée en 24 (ZGC est alors toujours générationnel). Le lanceur doit tester la version majeure. Fichiers : `tools/test-server/start.ps1`, `start.sh`, et les scripts de lancement de production de la phase 1 (point 1 de la feuille de route, absents du dépôt pour l'instant).
7. **Scripts de lancement de production (point 1 de la feuille de route).** À créer. Attention au `.gitignore` : `*.sh` et `*.bat` sont ignorés ; l'exception `!tools/**/*.sh` couvre les scripts shell sous `tools/`, pas les `.bat` ni un `start.sh` à la racine. Arguments mesurés ici et à y mettre : `-XX:+UseZGC -XX:+ZGenerational` (21 et 22), `@java9args.txt`, les options d'encodage ci-dessus, et `-Xlog:safepoint,gc:file=logs/gc-%t.log` pour les mesures.
8. **`-XX:MetaspaceSize` plus grand.** Une collection est déclenchée par `Metadata GC Threshold` pendant le démarrage, majeure en ZGC (0,175 s), `Concurrent Start` en G1. `-XX:MetaspaceSize=256m` l'évite probablement. Non essayé. Fichier : lanceurs, avec les arguments du point 7.
9. **Valider avec le modpack d'EarthQuest** (critère de sortie de la phase 1) : `mods/` était vide ici. Risques propres à Java 21 que ce monde vide ne couvre pas : mods qui utilisent `sun.misc.Cleaner`, `sun.reflect`, des `Unsafe` tardifs, `ScriptEngineManager` (Nashorn est embarqué mais n'a pas été appelé), les `--add-opens` manquants pour des mods précis, le coût de lwjgl3ify sur un grand nombre de classes.
10. **Tick légèrement plus long sur Java 21** (+0,15 à 0,2 ms par tick au repos). Voir la lecture n° 5. À vérifier d'abord en mesurant l'intervalle de tick avec une attente plus fine (`LockSupport.parkNanos` à la place de `Thread.sleep`) avant d'y toucher. Fichier : `patches/net/minecraft/server/MinecraftServer.java.patch` (boucle de tick, `Thread.sleep(wait / 1000000)`, vers la ligne 551). Priorité basse.
11. **Outillage de test** : `setup.ps1` et `setup.sh` pourraient écrire `banned-players.json`, `banned-ips.json`, `ops.json` et `whitelist.json` contenant `[]` pour supprimer les quatre piles du premier démarrage (même chose en Java 8). Un `tools/bench/` non suivi existe déjà dans l'arbre de travail et n'a pas été touché.

## Ce qui n'a pas été essayé

* Le modpack d'EarthQuest, des joueurs ou des bots (`tools/bench/` n'a pas été lancé).
* Java 17 (disponible mais hors de la demande), Java 24 et 25 (non installés).
* Un tas de plus de 2 Go : le critère « pauses sous 10 ms » sera à rejouer avec la taille réelle de production et un monde chargé.
* Un échantillon assez grand pour départager G1 et ZGC sur le MSPT : il faudrait une machine au repos et au moins une vingtaine d'essais.

## Tous les essais

Les données brutes (`summary.json`, `ticks.csv`, `gc.csv`, journaux de la JVM) sont restées sur la
machine d'essai et ne sont pas versionnées : un essai se rejoue avec le scénario des « Conditions ».

| Essai | JVM / GC | Démarrage (s) | MSPT moyen | p95 | p99 | max | TPS | Pauses GC (JMX) |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| j8 | Java 8 Parallel | 22,43 | 4,516 | 8,46 | 19,14 | 46,09 | 19,93 | 3, max 8 ms |
| k2 | Java 8 Parallel | 14,18 | 3,397 | 6,04 | 7,20 | 12,85 | 19,98 | 3, max 4 ms |
| k3 | Java 8 Parallel | 14,76 | 3,815 | 6,95 | 8,86 | 40,03 | 19,97 | 3, max 6 ms |
| k5 | Java 8 Parallel | 15,93 | 4,591 | 10,63 | 16,53 | 47,71 | 19,96 | 3, max 11 ms |
| j21z | Java 21 ZGC | 18,72 | 3,982 | 7,45 | 13,02 | 46,30 | 19,87 | 13 pauses ≤ 1 ms, 3 cycles (max 292 ms) |
| z2 | Java 21 ZGC | 15,94 | 3,580 | 6,25 | 7,63 | 20,55 | 19,90 | 18 pauses ≤ 0 ms, 4 cycles (max 259 ms) |
| z3 | Java 21 ZGC | 16,52 | 4,156 | 7,29 | 8,69 | 39,66 | 19,89 | 18 pauses ≤ 0 ms, 4 cycles (max 248 ms) |
| z4 | Java 21 ZGC | 26,14 | 4,381 | 8,29 | 13,69 | 35,63 | 19,87 | 13 pauses ≤ 1 ms, 3 cycles (max 298 ms) |
| z5 | Java 21 ZGC | 14,73 | 3,715 | 8,28 | 14,66 | 39,08 | 19,87 | 18 pauses ≤ 1 ms, 4 cycles (max 269 ms) |
| z6 | Java 21 ZGC | 16,38 | 3,287 | 7,56 | 11,38 | 49,80 | 19,91 | 18 pauses ≤ 1 ms, 4 cycles (max 285 ms) |
| j21g1 | Java 21 G1 | 15,78 | 3,203 | 5,62 | 6,60 | 9,03 | 19,90 | 2, max 3 ms |
| g2 | Java 21 G1 | 16,05 | 3,312 | 5,91 | 7,13 | 15,77 | 19,90 | 1, max 6 ms |
| g3 | Java 21 G1 | 15,79 | 3,709 | 6,93 | 8,60 | 27,45 | 19,90 | 2, max 7 ms |
| g4 | Java 21 G1 | 26,86 | 5,152 | 12,16 | 30,56 | 141,20 | 19,82 | 3, max 18 ms |
| g5 | Java 21 G1 | 15,26 | 4,021 | 9,31 | 14,42 | 22,86 | 19,89 | 2, max 18 ms |
| h1 | Java 8 G1 | 16,23 | 4,994 | 9,99 | 13,38 | 44,57 | 19,97 | 2, max 20 ms |
| h2 | Java 8 G1 | 14,27 | 4,208 | 7,39 | 10,14 | 27,49 | 19,93 | 2, max 17 ms |

## Suites données

| Correction | État |
| --- | --- |
| 1. `--illegal-access=warn` | Fait : retiré de `java9args.txt` (ignoré depuis Java 17 ; Java 9 à 16 se comportent désormais comme 17) |
| 4. Cycles et pauses GC | Fait : `metrics/GcBeans` classe les beans ; `/tps`, le rapport du banc et `summary.json` comptent les pauses à part (`gc.concurrent_cycles`), `gc.csv` a une colonne `kind` |
| 5. Encodage de la console | Fait : `-Dstdout.encoding=UTF-8 -Dstderr.encoding=UTF-8` dans `java9args.txt`, et `ConsoleBanner` lit l'encodage réel de `System.out` selon la version de Java |
| 6. `-XX:+ZGenerational` | Fait pour le serveur de test : `start.ps1 -Gc zgc` / `start.sh --gc zgc` ne l'ajoutent que sur Java 21 et 22 ; `-GcLog` / `--gc-log` écrit le journal des safepoints |
| 11. Listes vides au premier démarrage | Fait : modèles `ops.json`, `whitelist.json`, `banned-players.json`, `banned-ips.json` dans `tools/test-server/config/` |
| 2, 3, 7 à 10 | À faire, dans la phase 1 |
