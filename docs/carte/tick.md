# Carte du code : boucle de tick

Lecture faite sur la branche `dev` le 8 octobre 2026. Les numéros de ligne de `E/` se rapportent au workspace patché ; ceux de `S/io/github/gammaengine/**` peuvent dériver avec les commits suivants.
Conventions : `E/` = `eclipse/cauldron/src/main/java/` (sources décompilées et patchées, état réel du moteur) ; `S/` = `src/main/java/`. Sauf mention contraire tout s'exécute sur le thread « Server thread » (`MinecraftServer.primaryThread`). Les classes concernées ont toutes un `.patch` dans `patches/` (ex. `patches/net/minecraft/server/MinecraftServer.java.patch`) ; la colonne « patch » dit ce que le fork change par rapport à `eclipse/Clean`.

## Vue d'ensemble

Flux d'appels d'un tick, dans l'ordre.

1. `MinecraftServer.run()` (`E/net/minecraft/server/MinecraftServer.java:646`), lancée par `startServerThread()` (`:1060-1071`). `AutoThreadRuntime.boot()` (`:650`), `startServer()` (`:652`), `onServerStarted()` (`:655`).
   - Boucle `while (serverRunning)` (`:679`) : `wait = CrucibleConfigs.configs.getTickTime() - (curTime - lastTick) - catchupTime` (`:683`). `getTickTime()` = `serverTickTime / serverTickRate` (`S/io/github/crucible/CrucibleConfigs.java:257-259`), donc 50 ms (`run/Gamma.yml` : `tickHandler.serverTickRate: 20`).
   - `wait > 0` : `Thread.sleep(wait / 1000000)`, `catchupTime = 0`, `continue` (`:684-689`). Sinon `catchupTime = min(1e9, |wait|)` (`:692`) et le tick part.
   - Rattrapage : `catchupTime` ne sert qu'une fois, remis à 0 à la première attente. Inférence de `:683-693` : après un tick long le suivant démarre aussitôt, puis le rythme de 50 ms reprend depuis `lastTick` ; pas de salve de ticks, le retard devient du TPS perdu.
   - TPS : tous les 100 ticks (`currentTick++ % 100 == 0`, `:695`), `currentTps = 1E9 / (curTime - lastTick)` (`:697`) = l'intervalle d'UN SEUL tick, pas une moyenne ; puis moyennes exponentielles `recentTps[0..2]` (`:698-700`, `calcTps` `:640`). `currentTick` : `static int` initialisé à `currentTimeMillis()/50` (`:186`), lu par `ActivationRange`.
   - Verrou d'affinité CPU optionnel (`:666-672`), défaut `cauldron.optimization.affinityLocking: false`.
2. `MinecraftServer.tick()` (`:834`) :
   1. `TimingsManager.FULL_SERVER_TICK.startTiming()` (`:836`), `AutoThreadRuntime.onTickStart()` (`:837`, accroche GammaEngine), `WatchdogThread.tick()` (`:838`, `S/org/spigotmc/WatchdogThread.java:48-50`).
   2. `i = System.nanoTime()` (`:839`), puis `FMLCommonHandler.onPreServerTick()` (`:842`) : `ServerTickEvent(START)` sur le bus Forge (`E/cpw/mods/fml/common/FMLCommonHandler.java:262-265`).
   3. `++tickCounter` (`:845`) ; `updateTimeLightAndEntities()` (`:855`, corps `:914-1053`).
   4. Statut serveur toutes les 5 s (`:857-871`). Sauvegarde auto : `tickCounter % autosavePeriod == 0` (`:873`) -> `saveAllPlayerData()` + `saveAllWorlds(true)` (`:876-879`) en un bloc sur le thread principal ; `autosavePeriod` = `bukkit.yml ticks-per.autosave` (6000).
   5. `tickTimeArray[tickCounter % 100] = nanoTime() - i` (`:889`) : inclut pre-tick Forge, exclut `onPostServerTick` et `onTickEnd`.
   6. `onPostServerTick()` (`:907`), `AutoThreadRuntime.onTickEnd()` (`:910`, enregistre `server.tick` dans `GammaProfiler`), `FULL_SERVER_TICK.stopTiming()` (`:911`).
3. Dans `updateTimeLightAndEntities()` :
   - `getScheduler().mainThreadHeartbeat(tickCounter)` (`:919`) : tâches Bukkit synchrones dans l'ordre de file (`S/org/bukkit/craftbukkit/v1_7_R4/scheduler/CraftScheduler.java:343-388`), les asynchrones partent dans `executor.execute(task)` (`:371`).
   - `processQueue` (`:924-927`), `ChunkIOExecutor.tick()` (`:931`), heure aux joueurs toutes les 20 ticks (`:936-943`).
   - `DimensionManager.getIDs(tickCounter % 200 == 0)` (`:948`) : ordre de `Hashtable.keySet()` (`E/net/minecraftforge/common/DimensionManager.java:52,222`), non garanti. Boucle SÉQUENTIELLE sur les mondes (`:949-1031`) : `onPreWorldTick` (`:971`), `worldserver.tick()` (`:977`), `worldserver.updateEntities()` (`:1000`), `onPostWorldTick` (`:1020`), `getEntityTracker().updateTrackedEntities()` (`:1024`), `worldTickTimes[id][tickCounter % 100]` (`:1030`). Une exception remonte en `ReportedException` et arrête le serveur (`:980-995`, `:1003-1018`).
   - `DimensionManager.unloadWorlds` (`:1034`), `networkTick()` (`:1037`), `sendPlayerInfoToAllPlayers()` (`:1041`), `tickables` (`:1046-1049`).
4. `NetworkSystem.networkTick()` (`E/net/minecraft/network/NetworkSystem.java:144`), sous `synchronized (networkManagers)` (`:148`) : par connexion `processReceivedPackets()` (`:181`), qui dépile jusqu'à 1001 paquets (`NetworkManager.java:243`) et exécute `packet.processPacket(netHandler)` (`:258`) sur le thread principal. Les threads Netty ne font que remplir `receivedPacketsQueue` (ConcurrentLinkedQueue, `:62`, `:145`).
5. `WorldServer.tick()` (`E/net/minecraft/world/WorldServer.java:224-310`) : `super.tick()` (`:226`), `cleanupCache` (`:233`), sommeil (`:235`), apparition `animalSpawner.findChunksForSpawning` si joueurs et `doMobSpawning` (`:250-257`), `unloadQueuedChunks()` (`:261`), horloge (`:269-274`), `tickUpdates(false)` ticks planifiés (`:279`), `func_147456_g()` chunks actifs + ticks aléatoires (`:283`), `updatePlayerInstances()` (`:287`), villages (`:291`), portails (`:296-300`), sons (`:304`), `processChunkGC()` (`:308`).
6. `WorldServer.updateEntities()` (`:655-671`) : un monde sans joueur ni chunk forcé cesse d'appeler `World.updateEntities()` après 1200 ticks (`:657-663`). Sinon `World.updateEntities()` (`E/net/minecraft/world/World.java:2350-2644`) : entités météo (`:2363-2408`) ; `loadedEntityList.removeAll(unloadedEntityList)` (`:2411`) ; `ActivationRange.activateEntities(this)` (`:2434`) ; boucle entités (`:2440-2503`) bornée par `entityLimiter` ; purge des TileEntities à décharger (`:2510-2532`) ; boucle TileEntities (`:2536-2587`) bornée par `tileLimiter` ; ajout des TileEntities en attente (`:2609-2637`).

Aucun parallélisme dans cette boucle : mondes, entités, TileEntities, ticks de blocs, suivi et réseau s'exécutent l'un après l'autre sur le thread serveur. Travail déjà hors thread principal : lecture et décodage de chunks (`ChunkIOExecutor`, `E/net/minecraftforge/common/chunkio/ChunkIOExecutor.java:5-31`, pool = 1 + joueurs/50 : `:6-7,24-26` ; étape 1 `ChunkIOProvider.callStage1` lit et décode, étapes 2 et 3 sur le thread serveur), écriture des chunks (`ThreadedFileIOBase`, mais la sérialisation NBT `writeChunkToNBT` reste sur le thread principal : `AnvilChunkLoader.java:201-222,250`), préchargement du spawn au démarrage (`S/io/github/gammaengine/world/ChunkPrefetcher.java`), éclairage asynchrone optionnel (`World.java:4918,4922-4963`), tâches Bukkit asynchrones, entrée réseau Netty, compression des paquets de chunk (`S21PacketChunkData`, `S26PacketMapChunkBulk` patchés).

## Classes et méthodes clés

| Élément | `chemin:ligne` | Rôle | Patch concerné |
|---|---|---|---|
| `MinecraftServer.run/tick/updateTimeLightAndEntities` | `E/net/minecraft/server/MinecraftServer.java:646/834/914` | boucle, cadence, ordre des mondes | cadence Crucible (`getTickTime`), affinité Thermos, Timings Paper, 7 accroches GammaEngine |
| `World.updateEntities` | `E/net/minecraft/world/World.java:2350` | boucles entités et TileEntities, curseurs `tickPosition`/`tileTickPosition` (`:166,285`) | Spigot (limiteurs), Cauldron (`CauldronHooks`), Paper (`TimingHistory`) |
| `World.updateEntityWithOptionalForce` | `World.java:2665-2793` | tick d'une entité, filtre d'activation, mise à jour du chunk d'appartenance | Cauldron/Thermos (`canSushchestvoTick`, `isForced`, `EntityEvent.CanUpdate`) |
| `World.setActivePlayerChunksAndCheckLight` | `World.java:3468-3559` | construit les chunks actifs : chunk du joueur + `chunksPerPlayer` chunks aléatoires | Spigot `chunks-per-tick`, Cauldron (chunks forcés) |
| `WorldServer.func_147456_g` | `WorldServer.java:438-579` | boucle sur `activeChunkSet_CB`, ticks aléatoires (3 par sous-chunk, `:550`), météo | Spigot (trove `TLongShortHashMap`) |
| `WorldServer.tickUpdates` | `WorldServer.java:707-803` | ticks planifiés (TreeSet + LinkedHashSet synchronisé + liste du tick), traités au plus 1000 par tick, ou 1/20 de la file au-delà de 20 000 (`:720-732`) | CraftBukkit |
| `WorldServer.getPendingBlockUpdates` | `WorldServer.java:805-859` | parcourt TOUS les ticks en attente pour un chunk ; appelé à chaque sauvegarde de chunk (`AnvilChunkLoader.java:405`) | Thermos |
| `ActivationRange.activateEntities/checkIfActive` | `S/org/spigotmc/ActivationRange.java:129/249` | marque les entités proches des joueurs ; seule consommatrice : `World.java:2691` | Spigot + Cauldron |
| `CauldronHooks.canSushchestvoTick/canTileEntityTick/canUpdate` | `S/net/minecraftforge/cauldron/CauldronHooks.java:220/268/305` | intervalles par classe (`entities.yml`, `tileentities.yml`), liste noire | Cauldron/Thermos |
| `TickLimiter` | `S/org/spigotmc/TickLimiter.java:3-26` | coupe la boucle après `max-tick-time` ms (contrôle toutes les 300 itérations) | Spigot |
| `HashedArrayList` | `S/io/github/crucible/util/HashedArrayList.java:5` | type concret de `loadedEntityList` et `loadedTileEntityList` | Crucible/Thermos |
| `EntityTracker.updateTrackedEntities` | `E/net/minecraft/entity/EntityTracker.java:315-346` | pour chaque entrée suivie : `sendLocationToAllClients(playerEntities)` ; puis pour chaque joueur « mis à jour », boucle sur TOUTES les entrées (`:332-345`) | Thermos (verrou `trackerLock`, `waitForLock` `:59-78`) |
| `PlayerManager.updatePlayerInstances` | `E/net/minecraft/server/management/PlayerManager.java:58-106` | envoie les changements de blocs par chunk ; balayage complet toutes les 8000 ticks (`:64`) | CraftBukkit (files concurrentes) |
| `ChunkProviderServer.unloadQueuedChunks` | `E/net/minecraft/world/gen/ChunkProviderServer.java:508-587` | décharge 100 chunks/tick max (`:524`), `loadedChunks.remove(chunk)` O(n) (`:568`) | Cauldron/Thermos |
| `ChunkProviderServer.loadChunk` | `ChunkProviderServer.java:174-213` | chargement async (`queueChunkLoad`, `:193`) ou bloquant (`syncChunkLoad`, `:198`) | Forge/CraftBukkit |
| `SpawnerAnimals.findChunksForSpawning` | `E/net/minecraft/world/SpawnerAnimals.java:42` | reconstruit la carte des chunks éligibles à chaque appel (`:50-86`), `countEntities` par type (`:122`) | CraftBukkit (`LongObjectHashMap<Boolean>`) |
| `CraftingManager.findMatchingRecipe` | `E/net/minecraft/item/crafting/CraftingManager.java:301-380` | parcours linéaire de `recipes` (ArrayList publique, `:23`) | CraftBukkit (événement PreCraft) |
| `FurnaceRecipes.getSmeltingResult` | `E/net/minecraft/item/crafting/FurnaceRecipes.java:88-113` | parcours linéaire de `customRecipes` puis `smeltingList` (LinkedHashMap, `:18`) | CraftBukkit |
| `World.getCollidingBoundingBoxes` | `World.java:1946-2021` | `new ArrayList(30)` par appel, boucle blocs x chunks, aucune mémoïsation | Spigot (boucle par chunk), Cauldron (`checkBoundingBoxSize`) |
| `BlockRedstoneWire.func_150177_e/func_150175_a` | `E/net/minecraft/block/BlockRedstoneWire.java:74/87` | algorithme vanille ; seul ajout : `BlockRedstoneEvent` (`:164-172`) | CraftBukkit |
| `BlockRedstoneDiode/Torch.updateTick` | `E/net/minecraft/block/BlockRedstoneDiode.java:51`, `BlockRedstoneTorch.java:125` | limiteur global à l'horloge murale (`currentTimeMillis`) | Cauldron |
| `TileEntityHopper.updateEntity` | `E/net/minecraft/tileentity/TileEntityHopper.java:224-236` | tick chaque tick, compte à rebours `hopperTransfer`/`hopperCheck` (`:258,267`) | Spigot |
| `EventBus.post` (Forge) | `E/cpw/mods/fml/common/eventhandler/EventBus.java:151-191` | deux branches : avec Timings Crucible (`:153-175`), sinon boucle simple (`:176-191`) | Crucible |
| `ASMEventHandler.createWrapper` | `E/cpw/mods/fml/common/eventhandler/ASMEventHandler.java:73-` | classe ASM générée par `@SubscribeEvent` : l'appel est direct, pas de réflexion | 6 lignes (`ASMEventHandler.java.patch`) |
| `JavaPluginLoader.createRegisteredListeners` | `E/org/bukkit/plugin/java/JavaPluginLoader.java:263,324-337` | exécuteur Bukkit = classe anonyme + `method.invoke` (`:330`), toujours enveloppé par `TimedEventExecutor` | Paper/Crucible |
| `SimplePluginManager.callEvent/fireEvent` | `E/org/bukkit/plugin/SimplePluginManager.java:482/498` | `synchronized (this)` pour tout événement synchrone (`:491-494`) | CraftBukkit |
| `ChunkIOExecutor` | `E/net/minecraftforge/common/chunkio/ChunkIOExecutor.java:5-31` | pool d'E/S de chunks | `ChunkIOExecutor.java.patch` (2 lignes) |

## Structures visibles par les mods

| Champ | Type déclaré | Type concret réel | Modifié par rapport à vanille | Preuve |
|---|---|---|---|---|
| `World.loadedEntityList` | `List` | `HashedArrayList` (étend `ArrayList` + `LinkedHashSet` synchronisé) | oui (vanille : `ArrayList`) | `World.java:128` ; Clean `World.java:85` |
| `World.unloadedEntityList` | `List` | `HashedArrayList` | ajout Cauldron | `World.java:129` |
| `World.loadedTileEntityList` | `List` | `HashedArrayList` à la création, REMPLACÉE par un `ArrayList` simple dès qu'une TileEntity est à décharger | oui | `World.java:130`, remplacement `:2518-2525` |
| `World.field_147483_b` (à décharger) | `List` | `ArrayList` | non | `World.java:132` |
| `World.playerEntities`, `weatherEffects` | `List` | `ArrayList` | non | `World.java:133-134` |
| `World.activeChunkSet` | `Set` | `HashSet<ChunkCoordIntPair>`, reconstruit chaque tick (alloue des `ChunkCoordIntPair`) | `protected` -> `public` ; double structure `activeChunkSet_CB` (trove) | `World.java:168,209,3470-3533` |
| `World.activity` | `ConcurrentMap<ChunkCoordIntPair,Ticket>` | `ConcurrentHashMap`, alimenté par `ForgeChunkManager` | absent de vanille (0 occurrence dans Clean) | `World.java:4847` ; `ForgeChunkManager.java:355,367` |
| `Chunk.entityLists` | `List[]` | 16 `UnsafeList` (CraftBukkit) | oui | `Chunk.java:61,116,124` |
| `Chunk.chunkTileEntityMap` | `Map` | `HashMap(512, 0.90F)` | taille initiale | `Chunk.java:60,114` |
| `Chunk.entityCount` | `TObjectIntHashMap<Class>` | idem | ajout Spigot | `Chunk.java:72` |
| `ChunkProviderServer.loadedChunks` | `List` | `ArrayList` (« vanilla compatibility », reste alimentée) | oui | `ChunkProviderServer.java:82,262,568` |
| `ChunkProviderServer.loadedChunkHashMap` | `LongHashMap` | `VanillaChunkHashMap` (sous-classe de `LongHashMap`) adossée à un `ConcurrentHashMap<Long,Chunk>` ET à `ChunkBlockHashMap` (koloboke) | oui, double écriture à chaque ajout/retrait | `ChunkProviderServer.java:85-86` ; `S/thermos/wrapper/VanillaChunkHashMap.java:10-12,52-80` ; `ChunkBlockHashMap.java:9-12` |
| `ChunkProviderServer.chunksToUnload` | `LongHashSet` (CraftBukkit) | idem | type vanille remplacé (vanille : `Set`) | `ChunkProviderServer.java:76` |

Les moteurs lisent surtout `loadedChunkHashMap_KC.rawThermos()` (`ChunkProviderServer.java:100,166`) ; `loadedChunkHashMap` n'est qu'une façade pour les mods.

## Configuration existante

| Clé | Fichier | Défaut (dans `run/`) | Effet |
|---|---|---|---|
| `crucible.tickHandler.serverTickRate` / `serverTickTime` | `Gamma.yml` | 20 / 1000000000 | durée du tick = `serverTickTime / serverTickRate` ns (`CrucibleConfigs.java:257`) |
| `cauldron.optimization.affinityLocking` | `Gamma.yml` | false | verrouille le thread serveur sur un cœur (`MinecraftServer.java:666`) |
| `cauldron.optimization.redstoneRepeaterUpdateSpeed` / `redstoneTorchUpdateSpeed` | `Gamma.yml` | -1 / -1 | délai mini en ms entre deux `updateTick` d'un type de bloc (`BlockRedstoneDiode.java:53`) |
| `settings.skip-tileentity-ticks` | `tileentities.yml` | true | active `canTileEntityTick` (`CauldronHooks.java:270`) |
| `<classe>.tick-no-players` / `tick-interval` | `tileentities.yml` (par monde) | false / 1 | TileEntity de cette classe tick hors chunks actifs ; ne tick que si `tempsMonde % intervalle == 0` (`CauldronHooks.java:275-287`) |
| `settings.prevent-invalid-tileentity-updates` | `tileentities.yml` | true | classes dont `canUpdate()` est vrai sans surcharge de `updateEntity` bannies de la liste (`AnvilChunkLoader.java:533-545`) |
| `settings.skip-entity-ticks` | `entities.yml` | true | si FAUX, `canSushchestvoTick` renvoie 1 pour toutes les entités (`CauldronHooks.java:224-226`) |
| `<classe>.tick-no-players` / `never-ever-tick` / `tick-interval` | `entities.yml` | false / false / 1 | filtre par classe d'entité (`CauldronHooks.java:234-250`) |
| `entity-activation-range.animals/monsters/misc` | `spigot.yml` | 32 / 32 / 16 | rayons de `ActivationRange` (plafonnés à `viewDistance*16-8`, `ActivationRange.java:139`) |
| `entity-tracking-range.players/animals/monsters/misc/other` | `spigot.yml` | 48 / 48 / 48 / 32 / 64 | portée de suivi (`TrackingRange`), plafonnée par la distance d'entité de `ServerConfigurationManager` (`EntityTracker.java:225-228`) |
| `max-tick-time.tile` / `.entity` | `spigot.yml` | 1000 / 1000 ms | `TickLimiter` : abandon de la boucle au-delà (`SpigotWorldConfig.java:250-251` ; `World.java:4903-4904`) |
| `chunks-per-tick`, `clear-tick-list` | `spigot.yml` | 650, false | nombre cible de chunks à ticker (`SpigotWorldConfig.java:107,110`) ; fixe `chunksPerPlayer` (`World.java:3495`) |
| `view-distance` | `spigot.yml` + `server.properties` | 8 | sert aussi de distance de simulation (`WorldServer.java:1344-1347`) |
| `mob-spawn-range` | `spigot.yml` | 4 (plafonné à 8 et à `view-distance`, `SpawnerAnimals.java:61-63`) | demi-côté de la zone d'apparition |
| `ticks-per.hopper-transfer` / `hopper-check` | `spigot.yml` | 8 / 8 | cooldown des entonnoirs |
| `merge-radius.item` / `.exp` | `spigot.yml` | 2.5 / 3.0 | fusion des objets (`EntityItem.java:206`) et de l'XP (`World.java:1813`) |
| `max-entity-collisions` | `spigot.yml` | 8 | bornes de poussée entre entités (`EntityLivingBase.java:2185-2189`) |
| `max-tnt-per-tick`, `random-light-updates`, `use-async-lighting` | `spigot.yml` | 100, false, false | cf. `World.java:3548`, `4930` |
| `settings.timeout-time` / `restart-on-crash` | `spigot.yml` | 90 s / true | watchdog (`SpigotConfig.java:180`) |
| `settings.player-shuffle` | `spigot.yml` | 0 | mélange des connexions (`NetworkSystem.java:152`) |
| `spawn-limits.*`, `ticks-per.*` | `bukkit.yml` | 70/15/5/15 ; animal 400, monstre 1, autosave 6000 | plafonds d'apparition par monde (`SpawnerAnimals.java:97-113`), cadence d'apparition et de sauvegarde |
| `chunk-gc.enabled/period-in-ticks/load-threshold` | `bukkit.yml` | false / 600 / 0 | `CraftWorld.processChunkGC()` (`S/.../CraftWorld.java:1454`) |
| `settings.loadChunkOnRequest` | `Gamma.yml` | true | un mod peut forcer le chargement synchrone d'un chunk (`ChunkProviderServer.java:80`) |
| `removeErroringEntities` / `removeErroringTileEntities` | `config/forge.cfg` | false / false | si faux une exception d'entité ou de TileEntity arrête le serveur (`World.java:2479,2583`) |
| `maximumChunksPerTicket`, `maximumTicketCount` | `config/forgeChunkLoading.cfg` | 25 / 200 | seul plafond existant sur les chunkloaders |

## Pièges pour la suite

1. L'activation range de Spigot est quasi inopérante dans ce code. `ActivationRange.checkIfActive` n'a qu'un appelant (`World.java:2691`) et la condition exige `!canUpdate`, or `canUpdate` est vrai dès que le chunk et ses voisins à 2 chunks sont chargés (`World.java:2675`). Les entités du cœur de la zone chargée tickent donc toutes, `activateEntities` (`:2434`) tourne pour rien à ce stade ; seuls `never-ever-tick` et les intervalles par classe agissent (`:2697-2702`). À confirmer par mesure avant d'y toucher.
2. `loadedTileEntityList` change de classe en cours de partie (`HashedArrayList` -> `ArrayList`, `World.java:2518-2525`) : `contains` (`:2617`) devient O(n) ; toute référence gardée par un mod pointe l'ancienne liste.
3. `HashedArrayList` : `listIterator()` s'appelle lui-même (récursion infinie, `HashedArrayList.java:97-99`) ; `set()` retire le nouvel élément du hash au lieu de l'ancien (`:147-155`) ; `addAll(Collection)` ajoute les doublons à la liste sans les ajouter au hash (`:32-40`) ; `removeAll` reconstruit toute la liste depuis le hash (`:125-134`, O(n) à chaque tick avec une entité à décharger) ; `remove(int)` décale le tableau (`:115-123`). Le hash est un `synchronizedSet(LinkedHashSet)` (`:7`) : verrou et boxing sur chaque ajout/lecture.
4. Suppression dans la boucle : `loadedEntityList.remove(tickPosition--)` (`World.java:2497`) et les TileEntities invalides (`:2545,2552`) sont O(n) ; le curseur `tickPosition` est un champ partagé corrigé par `removePlayerEntityDangerously` (`:1930-1933`) : tout remplacement de structure doit conserver cette sémantique.
5. Politique de crash : une exception dans le tick d'un monde lève un `ReportedException` qui arrête tout le serveur (`MinecraftServer.java:994,1017`) et `removeErroringEntities`/`removeErroringTileEntities` valent faux (`forge.cfg`) : une entité ou une TileEntity de mod qui lève une exception fait aujourd'hui tomber le serveur. Toute phase déplacée sur un autre thread doit préserver ce résultat (ou le changer explicitement).
6. Le curseur d'itération avec `TickLimiter` (`World.java:2440,2536`) reprend où il s'est arrêté au tick suivant : un ordre de tick qui change (index, regroupement) modifie quelle entité est sautée sous charge.
7. `CauldronHooks.tileEntityCache` et `sushchestvoCache` sont des `HashMap` statiques non synchronisées, indexées par classe seule (`CauldronHooks.java:38-39`) : la config du PREMIER monde rencontré est réutilisée pour tous (`:275-276`).
8. `EntityTracker.waitForLock()` boucle `sleep(100)` tant que le verrou est pris, même par le thread courant (`isLocked()`), jusqu'à 100 fois, puis force `unlock()` (`EntityTracker.java:59-78`) : un appel réentrant depuis `updateTrackedEntities` bloquerait 10 s. Ne pas appeler ces méthodes depuis des workers.
9. Coûts super-linéaires déjà lisibles dans le code (à mesurer, voir aussi [scaling.md](../scaling.md)) : `EntityTracker` joueurs x entités (`:332-345`) ; `ActivationRange` joueurs x chunks x entités, copie de `playerEntities` par tick (`ActivationRange.java:141-161`) ; `countEntities` x 4 types x entités à chaque apparition (`SpawnerAnimals.java:122`, `World.java:4036`) ; `getPendingBlockUpdates` x chunks sauvegardés (`WorldServer.java:805`) ; `loadedChunks.remove(chunk)` (`ChunkProviderServer.java:568`) ; `chunksPerPlayer` tombe à 1 dès ~400 joueurs (`World.java:3495` : `(650-400)/400+0.5 = 1,1`), `growthOdds` plancher 35 (`:3504`).
10. L'éclairage asynchrone (`use-async-lighting`, défaut faux) a une garde `isModded` qui semble inversée : asynchrone seulement si le monde EST modé (`World.java:4930`) alors que les compteurs `pendingLightUpdates` ne sont décrémentés que si NON modé (`:3917`). Ne pas l'activer sans test.
11. Les événements Bukkit synchrones passent tous par le moniteur du `SimplePluginManager` (`:491-494`) : un goulot pour toute phase parallèle qui poste des événements.

## État des éléments de la feuille de route

Source : phases 4, 5, 10 et 11 de la [feuille de route](../roadmap.md). Aucun interrupteur propre à GammaEngine n'existe encore pour ces éléments (la configuration du moteur, `gammaengine.yml`, n'a que trois clés : `gamma.network.chunkCompressionLevel`, `gamma.profiling.enabledAtStartup`, `gamma.native.enabled`).

| Élément | État | Preuve | Interrupteur |
|---|---|---|---|
| Collections sans boxing | partiel | fastutil est une dépendance (`build.gradle:96`) mais n'est importé que par Timings (`S/co/aikar/timings/TimingHandler.java:28`, `S/co/aikar/util/LoadingIntMap.java:11`) ; trove dans `World.java:209`, `DataWatcher.java:34-38` (enveloppé par `TDecorators` donc reboxé), `Chunk.java:72` ; koloboke seulement dans `ChunkBlockHashMap.java:9-12`. Boxing restant sur le chemin chaud : `activeChunkSet` (`World.java:168`), `eligibleChunksForSpawning` (`SpawnerAnimals.java:30,130`), `ConcurrentHashMap<Long,Chunk>` (`VanillaChunkHashMap.java:12`), `activity` (`World.java:4847`) | aucun |
| Itération sans suppressions en O(n) | absent | `remove(int)`/`removeAll` O(n) : `World.java:2411,2497,2545,2552`, `HashedArrayList.java:115-134` | aucun |
| TileEntities qui ne tickent pas exclues | présent | `CauldronHooks.canUpdate` aux 3 points d'ajout (`World.java:2652,3228,4739` ; `CauldronHooks.java:305-307`), liste noire alimentée au chargement (`AnvilChunkLoader.java:531-545`) | `tileentities.yml settings.prevent-invalid-tileentity-updates` |
| Cache des chunks éligibles au spawn | absent | reconstruit à chaque appel (`SpawnerAnimals.java:50-86`), appelé chaque tick (`ticks-per.monster-spawns: 1`, `WorldServer.java:250-255`) | aucun |
| Distance de simulation séparée de la distance de vue | absent | chunks actifs tirés dans `getViewDistance()` (`World.java:3497`, `WorldServer.java:1344-1347`), activation plafonnée par `viewDistance` (`ActivationRange.java:139`), apparition aussi (`SpawnerAnimals.java:62`) | aucun |
| Limites par chunk | partiel | `Chunk.entityCount` est tenu à jour mais jamais lu pour limiter (`Chunk.java:72,881,921`, seules occurrences) ; plafonds d'apparition par monde (`SpawnerAnimals.java:97-113`) ; plafonds de tickets Forge ; aucune limite d'entités ni de TileEntities par chunk | `bukkit.yml spawn-limits` ; `forgeChunkLoading.cfg` |
| Ralentissement adaptatif | partiel (rudimentaire) | `TickLimiter` coupe les boucles entités/TileEntities après `max-tick-time` (1000 ms, soit 20 ticks : sans effet réel) ; `ticksPerAnimalSpawns` fixe ; `tickUpdates` plafonné à 1000 (`WorldServer.java:720-732`) ; aucun lien avec le TPS | `spigot.yml max-tick-time.*` |
| Index des recettes de craft et de four | absent | parcours linéaire (`CraftingManager.java:358-377`, `FurnaceRecipes.java:88-113`) ; `sortRecipies=true` dans `forge.cfg` ne fait que trier | aucun |
| Cache des collisions | absent | `new ArrayList(30)` et boucle de blocs à chaque appel (`World.java:1948-1995`) | `cauldron.settings.checkEntityBoundingBoxes` (garde-fou, pas un cache) |
| Redstone sans mises à jour redondantes | absent | fil de redstone = algorithme vanille (diff Clean : seul `BlockRedstoneEvent`, `BlockRedstoneWire.java:164-172`) ; seuls limiteurs à horloge murale globale (`BlockRedstoneDiode.java:53`) | `redstoneRepeaterUpdateSpeed`, `redstoneTorchUpdateSpeed` (-1) |
| Exécuteurs Bukkit en bytecode | absent | `method.invoke` (`JavaPluginLoader.java:330`) ; seul le bus Forge génère du bytecode (`ASMEventHandler.java:73-`) | aucun |
| Détection des I/O bloquantes sur le thread principal | absent | aucune sonde ; `syncChunkLoad` (`ChunkProviderServer.java:198`), sauvegarde auto (`MinecraftServer.java:873-879`), `ChunkIOExecutor.tick` (`:931`) bloquent sans mesure ; seul le watchdog voit un tick > 30 s | aucun |
| Phases parallèles en lecture seule | absent | boucle séquentielle ; `AutoThreadRuntime` n'expose que `addTickTask`, exécuté en début de tick sur le thread serveur | aucun |
| Un thread par dimension | absent | boucle `for` séquentielle (`MinecraftServer.java:949-1031`), structures partagées sans verrou (`activity`, `CauldronHooks.*Cache`, `EntityTracker` statique) | aucun |
| Tick par régions | absent | aucun code ; le pool `RegionTick` de l'ancien plan, jamais utilisé, a été retiré | aucun |

Autres optimisations déjà présentes : structure à deux niveaux koloboke pour la table des chunks (`ChunkBlockHashMap`) ; chunks actifs en trove avec compteur de joueurs (`World.java:209,3519-3520`) ; `growthOdds` variable (`World.java:3504`) ; purge des TileEntities en un seul passage (`World.java:2510-2532`) ; carte d'`EntityTracker` en `HashSet` ; limiteurs de temps (`TickLimiter`) ; fusion d'objets et d'XP ; `max-entity-collisions` ; saut de `World.updateEntities` pour un monde vide depuis 1200 ticks (`WorldServer.java:657-663`) ; hoppers espacés ; `chunk-gc` (désactivé) ; lecture asynchrone des chunks et préchargement du spawn ; compression des paquets de chunk à niveau réglable avec tampons réutilisés (`ScratchBuffers`) ; éclairage asynchrone (désactivé) ; surveillance de contention de threads (`CauldronHooks.java:438`, option `enableThreadContentionMonitoring`).
