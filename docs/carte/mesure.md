# Mesure : existant et points de sonde

Lecture faite sur la branche `dev` le 8 octobre 2026. Les numéros de ligne de `E/` se rapportent au workspace patché ; ceux de `S/io/github/gammaengine/**` peuvent dériver avec les commits suivants.
Conventions : `E/` = `eclipse/cauldron/src/main/java/` (état réel, hors git) ; `S/` = `src/main/java/`. « GammaEngine » désigne les ajouts du fork (`io.github.gammaengine.*`), à ne pas confondre avec Crucible.
Cet audit vérifie sur le code les hypothèses du [plan de la phase 0](../phase-0.md).

## Ce qui mesure déjà

| Mécanisme | Ce qu'il mesure | `chemin:ligne` | Export | Coût | État |
|---|---|---|---|---|---|
| `tickTimeArray` (100 ticks) | durée de `tick()` du pre-tick Forge à la sauvegarde auto incluse ; exclut `onPostServerTick` et l'attente | `E/net/minecraft/server/MinecraftServer.java:157,839,889` | texte : `/forge tps` (`E/net/minecraftforge/server/command/ForgeCommand.java:100-127`), `/gamma tps` (`S/io/github/crucible/CrucibleCommand.java:100-135`), GUI (`E/net/minecraft/server/gui/StatsComponent.java:47`), snooper (`MinecraftServer.java:1372`) | 2 `nanoTime` par tick | actif |
| `worldTickTimes` (`Hashtable<Integer,long[100]>`) | durée par dimension : pre-tick, `tick()`, `updateEntities()`, post-tick, suivi ; exclut scheduler, `processQueue`, réseau, liste des joueurs, `tickables` | `MinecraftServer.java:159,952,1030` ; `E/net/minecraftforge/common/DimensionManager.java:241,248` | idem ; le « TPS » affiché est `min(1000 / moyenne_ms, 20)` (`ForgeCommand.java:113-117`) : dérivé, jamais observé | 2 `nanoTime` par monde et par tick | actif |
| `currentTps`, `recentTps[3]` | intervalle d'UN seul tick, échantillonné tous les 100 ticks, puis moyennes exponentielles 1/5/15 min | `MinecraftServer.java:205,695-700` ; `/tps` : `S/org/bukkit/craftbukkit/v1_7_R4/command/TicksPerSecondCommand.java:20` | `/tps` affiche `currentTps` (un échantillon, arrondi à 0,1) ; `/gamma tps` affiche `recentTps` | nul | actif, peu précis |
| `WatchdogThread` | silence du thread serveur : avertit après `max(timeout/3, 5 s)` (30 s), puis après `timeout-time` (90 s) écrit les dumps et appelle `RestartCommand.restart()` si `restart-on-crash` (true) ; sonde toutes les 10 s | `S/org/spigotmc/WatchdogThread.java:48-50,93-212` (alerte `:180-200`, arrêt `:97-176`) ; démarrage `S/org/spigotmc/SpigotConfig.java:180` | log : par dimension chunks chargés/actifs, entités, TileEntities (`:113-121`), `entitiesTicked`/`tilesTicked`, paquet et joueur en cours (`E/net/minecraft/network/NetworkManager.java:240,256,271,276`), dump de tous les threads ; `dumpChunksOnDeadlock`, `dumpHeapOnDeadlock`, `dumpThreadsOnWarn` (défaut false) | une écriture volatile par tick, un `AtomicReference.set` par paquet | actif |
| `World.entitiesTicked`, `tilesTicked` | entités et TileEntities réellement ticked au dernier tick, par monde | `E/net/minecraft/world/World.java:2359-2360,2565,2707` | seulement le log du watchdog | un incrément | actif |
| `TickStatistics`, `LatencyHistogram`, `GammaProfiler` (GammaEngine) | durée du tick complet (`onTickStart` après `FULL_SERVER_TICK.start`, `onTickEnd` après `onPostServerTick`) ; TPS 1/5/15 min lissé en temps ; MSPT mean/p50/p95/p99/max EXACTS sur 1200 ticks ; histogramme log-linéaire (32 sous-seaux, 1920 `AtomicLong`, erreur relative au plus ~3 %) | `S/io/github/gammaengine/profiler/TickStatistics.java:20,38-62,94-110` ; `LatencyHistogram.java` ; `S/.../autothread/AutoThreadRuntime.java:111-120` | `/autothread status`, `/autothread profile start\|stop\|report`, fichier `gammaengine/reports/profile-<date>.txt` (`GammaProfiler.java:140-165`) ; tests `src/test/java/io/github/gammaengine/profiler/*Test.java` | ~4 opérations atomiques et 3 `Math.exp` par tick, enregistré même hors session | actif ; une seule métrique de tick (`server.tick`) |
| Sessions de profilage | remise à zéro de toutes les métriques + horodatage | `GammaProfiler.java:68-92` | rapport texte : durée, TPS, MSPT, tableau « Time by subsystem » trié par coût, compteurs | nul | actif ; le tableau ne contient que `server.tick`, `server.startup` et la compression native (`NativeEngine.java:145-166`) |
| `/autothread bench` (`WorldBenchmark`) | charge synthétique (chunks gardés chargés, entités avec IA, TileEntities qui tickent), CPU du processus, nombre et durée des GC, tas | `S/io/github/gammaengine/bench/WorldBenchmark.java:37-343` (CPU `:307-309`, GC `:319-336`, tas `:340`) | console + `profile-*.txt` | sur commande | inactif par défaut ; aucun joueur simulé |
| `/autothread memory [gc]` (`MemoryReport`) | tas, hors-tas, pools de buffers, threads, chunks/entités/TileEntities par monde | `S/io/github/gammaengine/diag/MemoryReport.java:33-97` | texte | `gc` : 2 x `System.gc()` + `sleep(250)` sur le thread appelant, donc le thread serveur (`:35-42`) | sur commande |
| Profileur vanille `/debug start\|stop` | arbre de sections `theProfiler.startSection` en % | `E/net/minecraft/command/CommandDebug.java:23,77` ; `ServerCommandManager.java:76` ; activation `MinecraftServer.java:847-852,1526` | `debug/profile-results-<date>.txt` | désactivé : un test de booléen par appel ; 34 références dans `World.java`, 6 par entité et par tick (`:2455,2484-2485,2502,2728,2777`) | inactif, utilisable |
| Logs Cauldron | chargement/déchargement de chunks, apparition/disparition d'entités, collisions, vitesses | `S/net/minecraftforge/cauldron/CauldronHooks.java:71-122,193-213` | log (pile si `logWithStackTraces`) | un test de booléen | inactif (`cauldron.logging.*` = false) |
| Dumps | `/gamma chunks [all]` JSON des entités et TileEntities par monde ; `/gamma findChunks` top 20 par TileEntities ; `/gamma heap`, `/cauldron heap` hprof | `CrucibleCommand.java:217-246` (findChunks), `:266-300` (heap, chunks) ; `CauldronHooks.java:309-392,423-435` | `chunk-dumps/`, `dumps/` | arrêt du thread serveur pendant le dump | sur commande |
| Contention de threads | temps bloqué par thread (JMX) | `CauldronHooks.java:438-445` | alimente les dumps du watchdog | JMX | inactif (`cauldron.debug.enableThreadContentionMonitoring: false`) |
| Snooper vanille | `addMemoryStatsToSnooper` toutes les 6000 ticks, `avg_tick_ms` | `MinecraftServer.java:893-901,1372` | envoi à Mojang, pas une mesure locale | négligeable | `snooper-enabled=true` dans `run/server.properties:21` |

Seul rapport réel dans le dépôt : `run/gammaengine/reports/profile-20260912-033228.txt`, benchmark `baseline` (289 chunks, 800 entités, 800 TileEntities, 600 ticks).
* Résultat : 19,97 TPS, MSPT moyen 2,086 ms (p95 3,768, p99 5,177, max 13,720), 1 collection GC de 9 ms, tas 1210,9 -> 578,5 MiB, 0,09 cœur moyen.
* Le tableau « Time by subsystem » n'y contient que `server.tick`. La ligne « MSPT (last minute) » (mean 1,57 ms, max 75,14 ms) vient du tampon de 1200 ticks et diffère de la session (600 ticks) : deux fenêtres, deux moyennes.
* Le serveur de test `run/` n'a aucun mod (`run/mods/` vide) et deux dossiers de plugins (`PluginMetrics`, `bStats`) : aucun profil réaliste n'existe dans le dépôt.

Les quatre « TPS » du serveur ne mesurent pas la même chose :

| Commande | Définition | Fenêtre | `chemin:ligne` |
|---|---|---|---|
| `/tps` | `1E9 / (début du tick - début du tick précédent)` d'un seul tick, pris tous les 100 ticks, plafonné à 20 | 1 tick, tous les 5 s | `MinecraftServer.java:695-697` ; `TicksPerSecondCommand.java:20` |
| `/gamma tps` (alias `/crucible`) | `recentTps[0..2]` (moyennes exponentielles de l'échantillon ci-dessus, pas de 5 s) + durée moyenne de `tickTimeArray` et par dimension | 1/5/15 min ; 100 ticks pour les durées | `MinecraftServer.java:698-700` ; `CrucibleCommand.java:100-135` |
| `/forge tps [dim]` | `min(1000 / durée moyenne en ms, 20)` : capacité théorique, pas le rythme réel | 100 ticks | `ForgeCommand.java:100-127` |
| `/autothread status` | taux instantané entre deux fins de tick, lissé en temps (1/5/15 min), plus MSPT exact sur 1200 ticks | 1/5/15 min ; 1 min | `TickStatistics.java:38-62,94-110` |

## Timings v1 et v2

* Actif : v2, paquet `co.aikar.timings` (`S/co/aikar/timings/`, 19 fichiers, ~2950 lignes). Désactivé au démarrage : `timings.enabledSinceServerStartup: false` (`run/Gamma.yml`), appliqué par `CrucibleModContainer.configureTimings()` (`S/io/github/crucible/CrucibleModContainer.java:276-283`) ; champ `Timings.timingsEnabled = false` (`S/co/aikar/timings/Timings.java:43`).
* Activation : `/timings on` (`S/co/aikar/timings/TimingsCommand.java:110-115`), sous-commandes `report reset on off paste verbon verboff timed timedverbose` (`:42`). Les timers dont le nom commence par `##` (classe d'entité ou de TileEntity, blocs, paquets, IA) ne tournent qu'en mode verbose (`TimingHandler.java:63,72`) : `timings.verbose: false`, `ultraverbose.enabled: false`.
* v1 : `org.spigotmc.CustomTimingsHandler` n'est plus qu'un adaptateur déprécié qui délègue à v2 (`S/org/spigotmc/CustomTimingsHandler.java`) ; `SpigotTimings` n'existe plus. `SimplePluginManager.useTimings` (`E/org/bukkit/plugin/SimplePluginManager.java:52`, alimenté par `settings.plugin-profiling`, `S/.../CraftServer.java:236`) ne mène plus à rien : branches `TimedRegisteredListener` mortes (`if (false)`, `E/org/bukkit/plugin/java/JavaPluginLoader.java:338`, `SimplePluginManager.java:566`). La commande v1 reste dans `E/org/bukkit/command/defaults/TimingsCommand.java` mais `SimpleCommandMap.java:38` enregistre celle de v2.
* Couverture quand v2 est actif : tick complet (`MinecraftServer.java:836,911`), pre/post tick Forge (`CrucibleTimings.forgePreTick/PostTick`, `:841-843,906-908`), scheduler Bukkit (`:918-920`), `processQueue`, `ChunkIOExecutor.tick`, heure, connexions, liste de joueurs, `tickables` (`:923-1050`), ~50 timers par monde (`S/co/aikar/timings/WorldTimingsHandler.java:12-64` : spawn, déchargement, ticks de blocs, tracker, villages, chunk GC, entités, TileEntities, chargement, génération, sauvegarde), entités et TileEntities par classe (verbose), événements Forge et handlers, exécuteurs d'événements Bukkit, tâches synchrones Bukkit, commandes de joueur (`NetHandlerPlayServer.java:1336-1395`).
* Déclaré mais jamais démarré : `Block.getTiming()` (`E/net/minecraft/block/Block.java:129-135`, aucun appelant), `MinecraftTimings.getPacketTiming` et `packetProcessTimer`, `serverOversleep`, `savePlayers`, `getTileEntityPersonalTimings` (`CrucibleTimings.getPersonalTimingFromTileEntity` renvoie `null`, TODO), plusieurs timers de `WorldTimingsHandler` (`doChunkMapUpdate`, `tracker2`, `chunkProviderTick`).
* Coût désactivé : `TimingHandler.startTiming()` sort sur `!enabled` avant tout `nanoTime` ou `Bukkit.isPrimaryThread()` (`TimingHandler.java:101-104`). Reste deux appels d'interface par entité et par TileEntity ticked (`World.java:2462-2464,2564,2567,2706,2791`) et trois tests dans `TimedEventExecutor.execute` (`:74-77`).
* Coût caché même désactivé : chaque `new Entity` et `new TileEntity` construit un nom (concaténation de `getClass().getName()`), un `TimingIdentifier` et fait une recherche dans une `ConcurrentHashMap` (`E/net/minecraft/entity/Entity.java:179`, `E/net/minecraft/tileentity/TileEntity.java:42`, `MinecraftTimings.java:112-125`, `TimingsManager.java:134-135`, `TimingIdentifier.java:54-59`). C'est un coût d'apparition d'objets, à mesurer sous charge d'items et de flèches.
* Coût activé : deux `nanoTime`, `Bukkit.isPrimaryThread()`, empilement sur `TIMING_STACK`, `addDiff` avec parent (`Int2ObjectOpenHashMap`) et groupe (`TimingHandler.java:101-166`).
* Coût activé côté Forge : chaque `EventBus.post` construit une chaîne et un `TimingIdentifier` pour l'événement, même sans handler, puis pour chaque entrée du tableau de listeners (`E/cpw/mods/fml/common/eventhandler/EventBus.java:153-175` ; `S/io/github/crucible/CrucibleTimings.java`). Le tableau contient aussi un marqueur `EventPriority` par niveau utilisé (`ListenerList.java:203-210`), chronométré lui aussi. Sites chauds : `LivingUpdateEvent` par entité vivante (`E/net/minecraftforge/common/ForgeHooks.java:297`), `PlayerTickEvent` par joueur (`E/cpw/mods/fml/common/FMLCommonHandler.java:349,354`).
* Le rapport mesure son coût moyen par paire start/stop (`timingcost`, `TimingsExport.java:125,252-284`). L'estimation « 25 à 50 ns par objet » de `docs/phase-0.md` n'est mesurée nulle part dans le dépôt.
* Export : JSON posté en HTTP à `http://timings.aikar.co/post` (`TimingsExport.java:321-361`), un rapport par minute au plus (`:88-94`), 12 fenêtres de 5 min conservées (`TimingsManager.java:56`, `historyInterval: 300`, `historyLength: 3600`). Pas de fichier local. À chaque fenêtre, `TimingHistory` parcourt TOUS les chunks chargés et leurs entités sur le thread principal pour compter les types par région (`TimingHistory.java:104-142`). Le rapport inclut GC (compte, durée), mémoire max, nombre de CPU, arguments JVM (`TimingsExport.java:124-135`).

## Attribution par mod et par plugin

Méthode.
* Aucun résolveur « classe -> mod » n'existe (`getCodeSource`/`getProtectionDomain` : seulement `FMLModContainer`, `ForgeModContainer`, `PluginClassLoaderFactory` et un commentaire de `PluginClassLoader.java:475`).
* Plugin : `PluginClassLoader.getPlugin()` (`E/org/bukkit/plugin/java/PluginClassLoader.java:36`), déjà employé par `TimingsManager.getPluginByClassloader` (`TimingsManager.java:180-190`). `CrucibleModContainer.isModPlugin` (`:271-274`) reconnaît un mod qui sert aussi de plugin.
* Mod : comparer le `CodeSource` de la classe à `ModContainer.getSource()` (`E/cpw/mods/fml/common/ModContainer.java:64`, `FMLModContainer.java:139`), table classe -> propriétaire remplie à la première rencontre, clés faibles (comme `MinecraftTimings.taskNameCache`, `:56`).
* Aides déjà dans Forge : `EntityRegistry.lookupModSpawn(clazz, true)` donne le `ModContainer` d'une entité enregistrée (`E/cpw/mods/fml/common/registry/EntityRegistry.java:365-381`, `getContainer` `:75`) ; le nom de registre `modid:nom` d'un bloc ; `GameRegistry.worldGenMap` (classe de générateur -> modId, `GameRegistry.java:86-95`) ; `NetworkRegistry.registry` (`:54`).
* Pièges : les wrappers de `ASMEventHandler` sont définis par un `ASMClassLoader` à part (`ASMEventHandler.java:141-150`) : leur propriétaire ne se déduit pas de leur classe ; une entité qui étend `EntityItem` est une classe de mod pour une entité vanille ; classes internes et anonymes à rattacher à la classe englobante ; le `CodeSource` des classes passées par `LaunchClassLoader` est à vérifier sur un vrai modpack.

| Site | `chemin:ligne` | Propriétaire obtenu comment | Existant |
|---|---|---|---|
| Boucle d'entités | `E/net/minecraft/world/World.java:2440-2503` (appel `:2463`) ; tick réel `:2665-2793` | `entity.getClass()` -> jar ; `lookupModSpawn` | timer global (`:2462-2464`) + timer par classe `entity.tickTimer` (`Entity.java:179`, `## tickEntity - <classe>`, groupe « Minecraft », verbose) : la classe est visible, pas le mod |
| Boucle de TileEntities | `World.java:2536-2587` (appel `:2566`) | `tile.getClass()` -> jar ; préfixe du nom de registre de `getBlockType()` | timer par classe `tileentity.tickTimer` (`TileEntity.java:42`), verbose ; timer par position prévu mais inactif |
| Ticks de blocs planifiés | `E/net/minecraft/world/WorldServer.java:753-798` (`block.updateTick` `:772`) | nom de registre du bloc, `block.getClass()` | un seul timer global (`:278-280`) ; aucun par bloc |
| Ticks aléatoires | `WorldServer.java:540-568` (`:564`) ; chunk `:449-451` | idem | un seul timer global (`:282-284`) ; aucun par bloc ni par chunk |
| Événements Forge | `EventBus.java:151-191` (appels `:164,183`) | `ASMEventHandler.owner` : champ PRIVÉ sans accesseur (`ASMEventHandler.java:28,33,42`) ; `EventBus.listenerOwners` privé (`EventBus.java:32,62`), rempli par `Loader.activeModContainer()` à l'enregistrement, repli sur le conteneur Minecraft (`:56-61`) | timer par événement + par « classe du listener », donc `ASMEventHandler` pour tous (`CrucibleTimings.java`) : les handlers d'un événement se confondent, aucun mod |
| Événements Bukkit | `S/co/aikar/timings/TimedEventExecutor.java:74-82` ; `fireEvent` `E/org/bukkit/plugin/SimplePluginManager.java:498-508` | `plugin` passé au constructeur (`JavaPluginLoader.java:324-337`, `SimplePluginManager.java:565`) ; `RegisteredListener.getPlugin()` | exact : groupe = nom du plugin, nom `Event: <classe> (<Événement>)`, actif avec Timings seulement ; appel par `method.invoke` (`JavaPluginLoader.java:330`) |
| Tâches Bukkit synchrones | `S/.../CraftScheduler.java:356-358` ; `CraftTask.java:73-77` | `CraftTask.getOwner()` (`CraftTask.java:65`), sinon classloader (`MinecraftTimings.java:66-82`) | exact par plugin et classe de tâche ; asynchrones : `NullTimingHandler` (`MinecraftTimings.java:67-69`) |
| Handlers de mod par tick | `FMLCommonHandler.java:249-273`, `:349,354` ; `MinecraftServer.java:842,907,971,1020` | via `EventBus.post` | un timer par type d'événement seulement |
| `processQueue`, `tickables` | `MinecraftServer.java:924-927,1046-1049` | classe du `Runnable` / de l'`IUpdatePlayerListBox` | un timer global chacun |
| Génération de monde | `E/cpw/mods/fml/common/registry/GameRegistry.java:123-139` (`generate` `:137`) ; `ChunkProviderServer.java:428,459` | `worldGenMap.get(classe)` : modId capturé à l'enregistrement (`:86-95`) | table présente ; interrupteur par générateur `worldgen-<modid>-<Classe>` (`:131`) ; aucun chronomètre par générateur |
| Paquets entrants | `E/net/minecraft/network/NetworkManager.java:258` ; mod : `E/cpw/mods/fml/common/network/internal/FMLProxyPacket.java:68-104` (canal `:106`) | classe du paquet ; paquet de mod : nom du canal -> mod via `NetworkRegistry` ; joueur : `WatchdogThread.currentPlayer` | `currentPacket`/`currentPlayer` posés pour le watchdog seulement ; `getPacketTiming` jamais appelé |
| Chargement de chunk | `E/net/minecraftforge/common/chunkio/ChunkIOProvider.java` (`callStage2`) | handlers de `ChunkDataEvent.Load`, `ChunkLoadEvent` | `syncChunkLoad*Timer` globaux |
| Commandes | `TimingsManager.java:149-170` ; `NetHandlerPlayServer.java:1336-1395` | plugin par classloader | `playerCommandTimer` global + timer par commande de plugin |

## Attribution par chunk

Aucun temps par chunk n'existe. Comptages existants :
* `TimingHistory.RegionData` : types d'entités et de TileEntities par région, une fois par fenêtre (`TimingHistory.java:104-142,177-236`).
* `/gamma findChunks` : 20 chunks les plus chargés en TileEntities (`CrucibleCommand.java:217-246`).
* `CauldronHooks.writeChunks` : JSON par monde avec la position de chaque entité et TileEntity (`:309-392`).
* `Chunk.entityCount` par type de créature (`E/net/minecraft/world/chunk/Chunk.java:72,881,921`) ; avertissements de collisions par chunk (`CauldronHooks.java:41,445-465`).

Sites où le chunk est déjà connu sans recherche : entité (`entity.chunkCoordX/Z`, `World.java:2418,2489`), TileEntity (`xCoord >> 4`, `:2553`), ticks aléatoires (`chunkX/chunkZ`, `WorldServer.java:449-451`), ticks planifiés (`:764`), apparition (clé `long`, `E/net/minecraft/world/SpawnerAnimals.java:130-134`), chargement (`ChunkProviderServer.java:174`).
Deux voies possibles : un champ de cumul dans `Chunk` (patch de `Chunk.java`, sans table) ou une table `long -> cumul` hors de `Chunk`. Aucune n'existe.

## Mémoire et GC

* Observable sans outil externe : tas utilisé/engagé/max, non-tas, pools de buffers directs et mappés, threads vivants, JVM libre/allouée, contenu par monde (`MemoryReport.java:46-97`, `/autothread memory`) ; compteurs GC cumulés (`TimingsExport.java:134`) ; delta de GC et de tas pendant un benchmark (`WorldBenchmark.java:319-340`) ; arguments JVM (`TimingsExport.java:133`) ; dump de tas (`/gamma heap`, `/cauldron heap`, `HotSpotDiagnosticMXBean`, `CauldronHooks.java:423-435`).
* Absent : écoute des pauses GC (aucun `GarbageCollectionNotificationInfo` ni `NotificationEmitter` dans `S/`), octets alloués par tick ou par thread (`com.sun.management.ThreadMXBean.getThreadAllocatedBytes`), historique du tas, journal GC. `java9args.txt` ne contient que `--add-opens` et des propriétés d'accès, rien sur le GC ni JFR ; aucun `-XX:` dans le dépôt.
* Le serveur de dev tourne sur HotSpot 8u491 (`run/start_seq.log`).

## Export et profileurs externes

* Formats existants : texte (`gammaengine/reports/profile-*.txt`, `debug/profile-results-*.txt`), JSON de dump (`chunk-dumps/`), hprof (`dumps/`), logs (`crash-reports/watchdog-*`), JSON Timings v2 envoyé en HTTP à `timings.aikar.co`. Ni CSV, ni série temporelle locale, ni MBean personnalisé (aucun `registerMBean` dans `S/`), ni serveur HTTP, ni Prometheus.
* Prévu mais absent : `docs/phase-0.md` annonce `ticks.csv`, `gc.csv`, `mods.csv`, `chunks.csv`, `summary.json` dans `gammaengine/bench/<exécution>/` et les interrupteurs `bench.export`, `bench.attribution`. `GammaConfig` n'a que trois clés.
* JFR, Spark, async-profiler : aucun fichier, dépendance, script ni option JVM dans le dépôt. Mentions seulement dans `docs/roadmap.md` (profil avant/après) et `docs/phase-0.md` (JFR à environ 1 %, indisponible sur Oracle 8u202). L'attribution après coup par table classe -> jar y est prévue, non écrite.
* `jetty-servlet` figure dans les bibliothèques (`build.gradle:102`) sans lien avec un export.

## Coût des sondes actuelles sur les chemins chauds

Aucune mesure de ces coûts n'existe dans le dépôt ; ce qui suit liste ce que le code exécute, Timings coupé (état par défaut).

| Chemin chaud | Sondes présentes | Détail | `chemin:ligne` |
|---|---|---|---|
| Par entité et par tick | 6 appels `theProfiler` (test de booléen), 4 appels de timer (`tickEntityTimer`, `tickTimer`), 3 compteurs | `TimingHistory.entityTicks` statique incrémenté 2 fois, `entitiesTicked++` | `World.java:2437,2455,2461-2464,2484-2485,2502,2706,2707,2728,2777,2791` |
| Par TileEntity et par tick | 2 appels de timer, 1 compteur | `tilesTicked++` | `World.java:2564-2567` |
| Par apparition d'entité ou de TileEntity | concaténation + `TimingIdentifier` + recherche `ConcurrentHashMap` | exécuté même Timings coupé | `Entity.java:179` ; `TileEntity.java:42` |
| Par paquet entrant | `System.currentTimeMillis()` + 2 `AtomicReference.set` ; second `currentTimeMillis()` seulement si `packetTimeout` | `currentPlayer.set` deux fois par connexion et par tick | `NetworkManager.java:240,255-256,260,271,276` |
| Par événement Forge | test `serverStarted && isTimingsEnabled()` | chemin non chronométré ensuite | `EventBus.java:153,176-191` |
| Par événement Bukkit | 3 tests dans `TimedEventExecutor`, puis `method.invoke` ; `synchronized` sur le gestionnaire de plugins pour tout événement synchrone | le moniteur sérialise les événements | `TimedEventExecutor.java:74-77` ; `JavaPluginLoader.java:330` ; `SimplePluginManager.java:491-494` |
| Par tâche Bukkit synchrone | `try (Timing)` autour de `task.run()` | `NullTimingHandler` pour les asynchrones | `CraftTask.java:73-77` |
| Par tick (serveur) | `FULL_SERVER_TICK.startTiming/stopTiming` (2 tests de drapeaux), `WatchdogThread.tick()` (`currentTimeMillis` + écriture volatile), `tickTimeArray`, `worldTickTimes`, `GammaProfiler` (4 atomiques, 3 `Math.exp`) | de l'ordre de la microseconde au total (estimation sur lecture, non mesurée) | `MinecraftServer.java:836-838,889,911,1030` ; `FullServerTickHandler.java:20-27` ; `AutoThreadRuntime.java:111-120` |

## Ce qui manque pour la phase 0

1. Décomposition du tick par phase : seule `server.tick` est enregistrée, par `AutoThreadRuntime.onTickEnd`. Les bornes existent (`MinecraftServer.java:841-843,873-886,919,931,971-1024,1034-1050,907`) ; `worldTickTimes` couvre les mondes mais pas scheduler, réseau, sauvegarde.
2. Compteurs par tick joints à la durée : joueurs, chunks chargés et actifs, entités et TileEntities chargées et réellement ticked (`entitiesTicked`/`tilesTicked` existent par monde, `World.java:2359-2360`).
3. Un résolveur classe -> mod/plugin et un accesseur sur le propriétaire du handler Forge (`ASMEventHandler.owner` privé) ; nom de timing des listeners Forge corrigé (tous « ASMEventHandler »).
4. Sondes par mod dans les boucles d'entités, de TileEntities, de ticks de blocs (aucun timer par bloc), de handlers Forge, de générateurs de monde et de paquets entrants, avec leur coût mesuré (hypothèse de `docs/phase-0.md`) et un interrupteur. Les timers par classe actuels exigent Timings v2 en verbose et ne donnent pas le mod.
5. Cumul par chunk et classement des chunks les plus coûteux (voir section).
6. Export lisible par une machine, local et sans réseau (CSV/JSON) ; l'export Timings v2 passe par un hôte externe.
7. Écoute des pauses GC (MXBeans), octets alloués par tick, tas par tick.
8. Mesure de la marge et de la gigue : `Thread.sleep(wait / 1000000)` tronque au ms (`MinecraftServer.java:686`) et, d'après la lecture du code, la boucle tourne sur `sleep(0)` pendant la dernière milliseconde, donc consomme du CPU en attente ; `serverOversleep` existe sans appelant.
9. Mesure des I/O bloquantes sur le thread serveur : aucune sonde autour de `syncChunkLoad` (`ChunkProviderServer.java:198`), de `saveAllWorlds` (`MinecraftServer.java:873-886`), de la sérialisation de chunk (`AnvilChunkLoader.java:201-222`) ni de `ChunkIOExecutor.tick` (`:931`).
10. Joueurs simulés, scénarios rejouables et comparaison de deux exécutions : rien dans le dépôt (le benchmark n'a pas de joueurs).
11. Précision du TPS : `/tps` et le watchdog lisent un échantillon d'UN tick toutes les 100 ticks (`MinecraftServer.java:697`) ; la référence du banc doit être `TickStatistics.tps1m()` et le MSPT.
12. Profileurs externes (JFR, async-profiler, Spark) : aucun script ni option JVM ne les active ; vérifier la JVM de référence (JFR absent d'Oracle 8u202).
