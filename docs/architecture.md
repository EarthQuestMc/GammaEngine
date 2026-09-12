# Architecture de GammaEngine

GammaEngine ajoute un sous-système à Crucible : le **runtime AutoThread**, qui possède toutes les
décisions de threading du serveur. Rien d'autre dans le serveur ne décide de ce qui s'exécute en
parallèle, et aucun mod, plugin ou administrateur ne déclare quoi que ce soit sur la sûreté des
threads. Le runtime observe ce que le code touche réellement à l'exécution et choisit l'ordonnancement
le plus sûr qui utilise quand même la machine.

Le code vit sous `src/main/java/io/github/gammaengine`. Les patches sur les classes Minecraft, Forge
et Bukkit restent des appels d'une ligne vers ce package, pour que le fork puisse encore être rebasé
sur Crucible amont.

## Où passe un tick aujourd'hui

Ce sont les chemins que le runtime doit reprendre, vérifiés dans le workspace généré
(`eclipse/cauldron/src/main/java`, numéros de ligne du workspace 1.7.10 / Forge 1614) :

| Étape | Emplacement |
| --- | --- |
| Boucle serveur, sommeil et rattrapage | `net/minecraft/server/MinecraftServer.java:642` (`run`) |
| Tick serveur | `MinecraftServer.java:830` (`tick`) |
| Tick Forge avant/après serveur | dans `tick`, via `FMLCommonHandler.onPre/onPostServerTick` |
| Battement du scheduler Bukkit | `MinecraftServer.java:910` (`updateTimeLightAndEntities`) |
| Vidage des IO de chunks Forge | même méthode, `ChunkIOExecutor.tick()` |
| Tick par monde | `MinecraftServer.updateTimeLightAndEntities` → `WorldServer.tick()` en `world/WorldServer.java:224` |
| Spawn de mobs, déchargement de chunks, ticks de blocs en attente, ticks aléatoires, carte des chunks, villages | dans `WorldServer.tick` |
| Tick des entités et TileEntities par monde | `WorldServer.updateEntities()` en `world/WorldServer.java:655` → `World.updateEntities()` en `world/World.java:2350` |
| Tick d'entité | `World.updateEntity(Entity)` en `world/World.java:2660` |
| Tick de TileEntity | boucle dans `World.updateEntities` appelant `TileEntity.updateEntity()` |
| Distribution des événements Forge | `cpw/mods/fml/common/eventhandler/EventBus.java:150` (`post`) |
| Scheduler Bukkit | `src/main/java/org/bukkit/craftbukkit/v1_7_R4/scheduler/CraftScheduler.java` |
| Fournisseur de chunks | `world/gen/ChunkProviderServer.java` (`loadChunk`, `provideChunk`, `unloadQueuedChunks`) |
| Sérialisation des chunks | `world/chunk/storage/AnvilChunkLoader.java` (`loadChunk__Async`, `saveChunk`, `writeChunkToNBT`, `readChunkFromNBT`) |
| Entrées-sorties des fichiers region | `world/chunk/storage/RegionFile.java` (lecture et écriture synchronisées) |
| Chargement asynchrone de chunks existant | `net/minecraftforge/common/chunkio/ChunkIOExecutor.java` |

Tout ce qui précède s'exécute aujourd'hui sur un seul thread : le thread serveur de Minecraft. Le
seul parallélisme préexistant est l'exécuteur d'IO de chunks de Forge, Netty, et les tâches
asynchrones du scheduler Bukkit.

## Sous-systèmes

Implémentés à ce jour (phases 0 et 1) :

| Classe | Responsabilité |
| --- | --- |
| `GammaEngine` | Identité du fork et logger partagé |
| `autothread.AutoThreadRuntime` | Cycle de vie, identité du thread principal, instrumentation du tick, état |
| `config.GammaConfig` | `GammaAutoThread.yml` : limites de ressources et diagnostics uniquement |
| `platform.CpuTopology` | Cœurs physiques contre jumeaux SMT, nombre de sockets pour NUMA |
| `concurrent.ThreadPools` | Les pools et les règles qui les dimensionnent |
| `concurrent.ManagedPool` | Pool instrumenté et redimensionnable : attente en file, temps d'exécution, profondeur |
| `profiler.GammaProfiler` | Registre de métriques, sessions de profilage, rapports |
| `profiler.LatencyHistogram` | p50/p95/p99 en mémoire bornée, enregistrement sans verrou |
| `profiler.TickStatistics` | TPS glissant et percentiles MSPT exacts sur la dernière minute |
| `nativeengine.NativeEngine` | Façade JNI unique, toujours avec un repli Java |
| `command.AutoThreadCommand` | `/autothread status|workers|regions|conflicts|native|profile` |

Prévus, dans l'ordre de construction (voir [threading-model.md](threading-model.md) pour les règles
que chacun doit respecter) :

`RegionManager`, `Region`, `RegionOwnership`, `RegionContext`, `RegionScheduler`,
`EntityScheduler`, `AccessTracker`, `ConflictDetector`, `DependencyGraph`,
`CrossRegionCoordinator`, `MultiRegionTransaction`, `ChunkSaveSnapshot`, `AsyncChunkLoader`,
`AsyncChunkSaver`, `AutoQuarantineManager`, la couche d'apprentissage du `RuntimeProfiler`.

## Pools de threads

Il n'y a pas d'exécuteur unique. Des travaux de formes différentes ne doivent pas partager une file,
parce qu'une compression de chunk de deux secondes placée devant un tick de région ajoute deux
secondes au temps de tick de cette région. Les tailles dérivent des cœurs **physiques**.

| Pool | Taille par défaut (machine de référence 8c/16t) | Travail |
| --- | --- | --- |
| `RegionTick` | cœurs − 1 = 7 | Simulation du monde. Avec le thread serveur, sature les 8 cœurs |
| `ChunkIO` | cœurs / 2 = 4 | Lectures et écritures disque, bloquantes |
| `ChunkWorker` | cœurs / 2 = 4 | Encodage et décodage NBT, compression, aides à la génération |
| `Async` | cœurs / 2 = 4 | Tâches asynchrones des plugins et travail de fond du moteur |
| `Native` | cœurs / 4 = 2 | Travail JNI par lots |
| `Background` | 1 | Agrégation des métriques, écriture des rapports |

Les workers tournent une priorité en dessous du thread serveur, pour que le coordinateur gagne
toujours un arbitrage d'ordonnancement, et tous les threads des pools sont des démons pour qu'un
démarrage raté ne puisse jamais laisser le processus vivant.

## Séquence de démarrage

1. `MinecraftServer.run()` appelle `AutoThreadRuntime.boot()` avant qu'aucun monde n'existe :
   configuration, topologie CPU, pools, bibliothèque native optionnelle.
2. `startServer()` s'exécute sans changement : Forge charge les mods, Bukkit charge les plugins, les
   mondes se chargent.
3. `AutoThreadRuntime.onServerStarted()` s'exécute après `handleServerStarted`.
4. Chaque tick appelle `onTickStart()` et `onTickEnd()` pour les métriques.
5. `MinecraftServer.stopServer()` appelle `shutdown()` après la sauvegarde des mondes, jamais avant :
   les workers doivent terminer leurs écritures en attente pendant que les données du monde sont
   encore cohérentes.
