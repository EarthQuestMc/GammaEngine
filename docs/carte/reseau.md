# Carte du code — réseau

Chemins relatifs à la racine du dépôt. `E/` = `eclipse/cauldron/src/main/java/`, `S/` = `src/main/java/`, `net/…` sans préfixe = sous `E/`. Netty 4.0.10.Final (dépendance déclarée dans `jsons/`) ; Minecraft 1.7.10, protocole 5, sans compression de paquets (seulement le chiffrement en mode en ligne).

## Vue d'ensemble

Pipeline Netty d'une connexion serveur, dans l'ordre : `timeout` (ReadTimeoutHandler) → `legacy_query` (PingResponseHandler) → `splitter` → `decoder` → `prepender` → `encoder` → `packet_handler` (le NetworkManager) : `E/net/minecraft/network/NetworkSystem.java:94,99`. Mode en ligne : `decrypt` avant `splitter`, `encrypt` avant `prepender` (`NetworkManager.java:354-355`). Après le login, `fml:packet_handler` (NetworkDispatcher) s'insère avant `packet_handler` (`NetworkDispatcher.java:124`).

**Threads.** Un seul `NioEventLoopGroup(0, …)` statique, threads démons « Netty IO #%d » (`NetworkSystem.java:49`) ; NIO uniquement, aucun Epoll (`grep Epoll` dans `net/minecraft/network` : non trouvé). `TCP_NODELAY=false` et `IP_TOS=24` (`:78,87`). Le thread principal appelle `networkTick()` depuis `MinecraftServer.java:1037` (section « connection »).

**Entrant, étape par étape.**
1. Thread Netty : `ReadTimeoutHandler(FMLNetworkHandler.READ_TIMEOUT)`, 30 s par défaut, propriété JVM `fml.readTimeout` (`E/cpw/mods/fml/common/network/internal/FMLNetworkHandler.java:62`). 30 s sans aucun octet reçu → `disconnect.timeout` (`NetworkManager.java:123-126`).
2. Thread Netty : découpage VarInt, décodage, puis `NetworkManager.channelRead0` (`:135-148`).
3. Si `packet.hasPriority()` : `processPacket` **sur le thread Netty**, tout de suite. Sinon : file `receivedPacketsQueue` (ConcurrentLinkedQueue) vidée par le thread principal. Priorité vraie pour : `C00Handshake` (`net/minecraft/network/handshake/client/C00Handshake.java:52`), `C00PacketKeepAlive` (`play/client/C00PacketKeepAlive.java:39`), `C00PacketServerQuery` (`status/client/…:22`), `C01PacketPing` (`status/client/…:39`), `C01PacketChatMessage` si le texte ne commence pas par `/` (patch CraftBukkit `play/client/C01PacketChatMessage.java:57-60`). Tout le reste passe par la file : `C00PacketLoginStart`, `C03/C04/C05/C06`, commandes `/…`, interactions.
4. Thread principal, `networkTick` : par NetworkManager, canal fermé → `onDisconnect` (`NetworkSystem.java:164-176`), sinon `processReceivedPackets()` : vide la file sortante, **au plus 1001 paquets par connexion et par tick** (`for (int i = 1000; … i >= 0; …)`, `NetworkManager.java:243`), puis `netHandler.onNetworkTick()` (`:275`) puis `channel.flush()` (`:279`). Exception → `S40PacketDisconnect("Internal server error")` (`NetworkSystem.java:200-211`). Tout se fait sous `synchronized (networkManagers)` (`:148`).

**Sortant, étape par étape.**
1. N'importe quel thread : `NetHandlerPlayServer.sendPacket` (`net/minecraft/network/NetHandlerPlayServer.java:977`) → `NetworkManager.scheduleOutboundPacket` (`NetworkManager.java:157`).
2. Canal ouvert : vide d'abord la file d'attente (`:209-219`), puis `dispatchPacket` (`:171-207`) : hors boucle Netty, `eventLoop().execute(…)` qui fait `writeAndFlush` ; **un flush par paquet**. Canal fermé : mise en file `outboundPacketsQueue`.
3. Thread Netty : `MessageSerializer.encode` appelle `Packet.writePacketData` (`net/minecraft/util/MessageSerializer.java:29,46`). **C'est ici que les paquets de chunks sont compressés.**
4. Chiffrement (mode en ligne) puis socket.

**Chunks.** Construction du `S26PacketMapChunkBulk` (extraction des sections vers `field_149268_i`) sur le thread qui envoie : `EntityPlayerMP.java:373`, lot de `max-bulk-chunks` (5, `spigot.yml`, lu `SpigotWorldConfig.java:240`). La compression `Deflater` est faite dans `writePacketData` → `compress()` (`S26PacketMapChunkBulk.java:89,213`) donc sur Netty. `S21PacketChunkData` : même schéma, `deflate()` sous sémaphore `deflateGate` (`S21PacketChunkData.java:46,152-161`), un paquet est partagé entre les joueurs d'un chunk (`PlayerManager.java:582`). **Changements GammaEngine** (commit 54622928) : niveau de deflate configurable (`GammaConfig.chunkCompressionLevel()`, défaut 4, Crucible/Spigot : 6→4), tampon de travail par thread `ScratchBuffers` à la place du tampon statique partagé (`S21PacketChunkData.java:57,197`), `Deflater` par thread dans S26 (`S26…:30-37`), copie des seuls octets compressés (plus de tampon sortie de la taille de l'entrée), débordement géré par `ByteArrayOutputStream` (vanilla tronquait). S26 a **perdu** le sémaphore Cauldron `deflateGate` : sûr tant qu'un S26 n'est envoyé qu'à un joueur (à revérifier si un jour il est mutualisé).

## Classes et méthodes clés

| Élément | `chemin:ligne` | Rôle | Patch Crucible / ajout |
|---|---|---|---|
| NetworkSystem | `E/net/minecraft/network/NetworkSystem.java:46-224` | groupe Netty (49), `addLanEndpoint` (65-104), `networkTick` (144-219) | Thermos `processing`/`stack` (56-57, 97-98, 216-218) ; Spigot `player-shuffle` (152-155) |
| NetworkManager | `net/minecraft/network/NetworkManager.java:60-407` | file entrante/sortante, `processReceivedPackets` (221-280), `enableEncryption` (352) | Crucible : watchdog + chronométrage de paquet (238-276) ; CraftBukkit : saute les paquets si `autoRead` coupé (247-251) ; Spigot : versions 4/5 (76-89) |
| PingResponseHandler | `net/minecraft/network/PingResponseHandler.java:25-104` | ping legacy (0xFE) sur Netty | aucun patch (absent de `patches/`) |
| NetHandlerHandshakeTCP | `net/minecraft/server/network/NetHandlerHandshakeTCP.java:43-157` | throttle 59-103 ; protocole 5 exigé 105-116 ; BungeeCord 121-145 | CraftBukkit (throttle), Spigot (bungee) |
| NetHandlerStatusServer | `…/NetHandlerStatusServer.java:55-107` | statut : `ServerListPingEvent`, version `5` (93) | CraftBukkit ; bloc `modinfo` ajouté à la sérialisation (`ServerStatusResponse.java:266` → `FMLNetworkHandler.java:229-243`) |
| NetHandlerLoginServer | `…/NetHandlerLoginServer.java:65-76` (tick), `94-114` (UUID), `117-142` (fin), `160-178` (LoginStart) | machine d'états HELLO→READY_TO_ACCEPT→ACCEPTED | Spigot `initUUID` ; CraftBukkit `attemptLogin` |
| ThreadPlayerLookupUUID | `S/net/minecraft/server/network/ThreadPlayerLookupUUID.java:26-110` | **un thread par login**, même hors ligne ; `AsyncPlayerPreLoginEvent` (77) | Spigot/Cauldron |
| ServerConfigurationManager | `net/minecraft/server/management/ServerConfigurationManager.java:149` (join), `530-593` (`attemptLogin`) | bannissement, liste blanche, plein (578), `PlayerLoginEvent` | CraftBukkit |
| NetHandlerPlayServer | `…/NetHandlerPlayServer.java:219-253` (tick), `299-600` (mouvement), `977` (envoi), `1057-1208` (chat), `2382` (keep-alive) | traitement de jeu | CraftBukkit + Thermos (moved too quickly 509-510) |
| EnumConnectionState | `net/minecraft/network/EnumConnectionState.java:108-232` | table des identifiants de paquets | Forge : `S3F/C17` |
| NetworkDispatcher | `E/cpw/mods/fml/common/network/handshake/NetworkDispatcher.java:101-201` | greffe FML dans le canal ; `serverInitiateHandshake` (141-176) | Thermos : attente `fml:packet_handler` (144-151) ; Cauldron : canaux Bukkit (317) |
| FMLHandshakeServerState | `…/handshake/FMLHandshakeServerState.java:15-102` | états serveur START/HELLO/WAITINGCACK/COMPLETE/DONE | Crucible : `thermos_logging_clientModList` |
| DedicatedServer.startServer | `net/minecraft/server/dedicated/DedicatedServer.java:103-` | thread console JLine (105-149), retrait des ConsoleAppender (161-197), propriétés (226-261), Query/RCON (372-384) | Crucible : « go nuclear on all loggers » (171-192) |
| SpigotConfig | `S/org/spigotmc/SpigotConfig.java:183-204,237` | `bungeecord`, `netty-threads`, `player-shuffle` | Spigot |
| RCON / Query | `net/minecraft/network/rcon/RConThreadMain.java:26-100`, `RConThreadQuery.java:44-90,261-` | services annexes | `RConThreadClient` patché (fermeture propre) |

## Handshake Forge

Tout transite en état PLAY dans `S3FPacketCustomPayload` (serveur→client) et `C17PacketCustomPayload` (client→serveur). Forge a modifié S3F : longueur en **VarShort** (`S3FPacketCustomPayload.java:42,49`, `ByteBufUtils.java:64-89`) ; C17 garde un `short` simple, charge ignorée si la longueur n'est pas dans ]0 ; 32767[ (`C17PacketCustomPayload.java:47-53`), nom de canal ≤ 20 caractères (`:46`). Un message FML = canal `FML|HS`, 1er octet = discriminant (`FMLIndexedMessageToMessageCodec.java:49-50`). Discriminants (`FMLHandshakeCodec.java:10-15`) : 0 ServerHello, 1 ClientHello, 2 ModList, 3 ModIdData, -1 (0xFF) HandshakeAck, -2 HandshakeReset. Chaîne FML : `varint` Forge (7 bits, poids faible d'abord, taille max en octets passée en paramètre), `String` = varint(≤2 octets) + UTF-8 (`ByteBufUtils.java:116-136`).

Séquence côté serveur (machine `FMLHandshakeServerState`, ordinaux START0 HELLO1 WAITINGCACK2 COMPLETE3 DONE4 ERROR5) :
1. `NetHandlerLoginServer.func_147326_c` envoie `S02PacketLoginSuccess`, puis `FMLNetworkHandler.fmlServerHandshake` (`:140`) → `NetworkDispatcher.serverToClientHandshake` coupe l'autoRead, insère le dispatcher (`:114-125`), `handlerAdded` déclenche START (`:133-139`).
2. START : `serverInitiateHandshake` crée le `NetHandlerPlayServer`, met `player.playerNetServerHandler = null`, passe le NetworkManager en PLAY (`:158-163`). Envoie (a) S3F `REGISTER` = `"FML|HS\0FML\0<canaux serveur séparés par \0>"` (`FMLHandshakeMessage.java:26-31`), (b) S3F `FML|HS` `[0x00][byte 2][int dimension]` = ServerHello (`:44-49` ; `FML_PROTOCOL = 2`, `NetworkRegistry.java:68`). → HELLO.
3. HELLO : reçoit ClientHello `[0x01][byte protocole]` (journalisé, reste en HELLO), puis ModList `[0x02][varint(≤2) n][n × (String modid, String version)]` (`FMLHandshakeMessage.java:109-130`). `checkModList` (voir plus bas). Succès : envoie la ModList du serveur (`Loader.getActiveModList`) → WAITINGCACK ; échec : `rejectHandshake` = `S40PacketDisconnect` « Mod rejections [...] » (`NetworkDispatcher.java:380`, `FMLNetworkHandler.java:160-190`).
4. WAITINGCACK : **toute** réception est un ACK (le contenu n'est pas lu). Hors canal local, envoie ModIdData `[0x03][varint(≤3) n][n × (String nom, varint(≤3) id)][varint nBlocs][String…][varint nItems][String…]` construit à chaque connexion par `GameData.buildItemDataList()` (`FMLHandshakeServerState.java:68`), puis `HandshakeAck(2)` `[0xFF][2]`, puis `fireNetworkHandshake` → COMPLETE. Piège : le compteur d'items écrit est `blockSubstitutions.size()` (`FMLHandshakeMessage.java:213`) ; un client qui n'a pas besoin des tables n'a pas à les décoder.
5. COMPLETE : **toute** réception déclenche `HandshakeAck(3)` + `CompleteHandshake` → `completeServerSideConnection` → `ServerConfigurationManager.initializeConnectionToPlayer` (`NetworkDispatcher.java:194-201`) → S01 JoinGame (`ServerConfigurationManager.java:200`), `MC|Brand`, S05, S39, S09, S38 (liste des joueurs), S08 PosLook (`:226`), temps/météo, chunks. DONE ignore la suite.

**Fil d'exécution du handshake et de l'entrée en jeu (déduit du code, non confirmé à l'exécution).** `NetworkDispatcher` est un maillon du pipeline placé avant `packet_handler` (`NetworkDispatcher.java:124,203-221`) ; il consomme les `C17` de canal `FML|HS`, `REGISTER`, `UNREGISTER` sur le **thread Netty**, sans les mettre dans la file du tick (`handled = true`, `:318-335`). La machine d'états, `checkModList`, `buildItemDataList` et, au dernier message, `completeServerSideConnection` → `initializeConnectionToPlayer` (`:194-201`) s'exécutent donc sur un thread Netty : lecture du fichier joueur, `playerLoggedIn`, `PlayerJoinEvent`, `spawnEntityInWorld`, ajout au `PlayerManager`. Aucun report vers le thread principal dans `HandshakeCompletionHandler` (le patch n'ajoute qu'une garde contre `null`, `patches/cpw/mods/fml/common/network/internal/HandshakeCompletionHandler.java.patch`). Les paquets de canaux de mods enregistrés (`hasChannel`) suivent le même chemin (`:336-342`).

Séquence d'un vrai client (`FMLHandshakeClientState.java:29-152`, ordinaux START0 HELLO1 WAITINGSERVERDATA2 WAITINGSERVERCOMPLETE3 PENDINGCOMPLETE4 COMPLETE5) : C17 `REGISTER` ; ClientHello ; ModList ; sur ModList serveur → Ack(2) ; sur ModIdData → Ack(3) ; sur Ack(2) du serveur → Ack(4) ; sur Ack(3) → Ack(5). Le serveur n'en exige que trois : ModList, un message après sa ModList, un message après son ModIdData.

**Vérification des mods (serveur)** : `FMLNetworkHandler.checkModList` (`:160-190`) parcourt le registre réseau **du serveur**. Par mod : `NetworkModHolder.check` (`internal/NetworkModHolder.java:212`). `DefaultNetworkChecker` (`:54-59`) : le mod doit figurer dans la liste du client avec une version acceptée (`acceptVersion` `:202-210` : plage `acceptableRemoteVersions` sinon égalité stricte avec la version du serveur) ; absent côté client = refus (`side == Side.SERVER` faux). `acceptableRemoteVersions="*"` → `IgnoredChecker` (`:173-176`), toujours vrai. Méthode `@NetworkCheckHandler` → `MethodNetworkChecker` (`:66-79`). Thermos refuse aussi si un mod serveur a un nom contenant `cjb`, `xray`, `radarbro`… (`FMLNetworkHandler.java:177-182`). **Conséquence** : un bot doit renvoyer la liste `modinfo.modList` du statut (id, version) telle quelle. Le statut : JSON avec `"modinfo":{"type":"FML","modList":[{"modid":…,"version":…}]}` (`FMLNetworkHandler.java:229-243`).

**Client vanilla** : aucun message `FML|HS` n'arrive ; le serveur reste en AWAITING_HANDSHAKE, minuterie « client vanilla » de **10 heures** (`NetworkDispatcher.java:365`, identique dans `forge/fml/…/NetworkDispatcher.java:353`) puis `kickVanilla` « This is modded. No modded response received. Bye! » (`:252-255`). Le client vanilla répond aux keep-alive, donc le `ReadTimeout` de 30 s ne l'éjecte pas (déduction du code, non testé). Il n'a jamais de S01 JoinGame. Pas d'éjection rapide : un banc ne doit pas en laisser.

## Configuration existante

| Clé | Fichier | Défaut | Effet |
|---|---|---|---|
| `online-mode` | `server.properties` (`DedicatedServer.java:226`) | `true` (`run/` : `false`) | `false` : pas de chiffrement ni de session Mojang, UUID hors ligne (`NetHandlerLoginServer.java:167-177`) |
| `server-ip`, `server-port` | `server.properties` (`:227,261`) | vide, 25565 | adresse d'écoute |
| `max-players` | `server.properties` (`DedicatedPlayerList.java:26`) | 20 (`run/` : 100) | refus « server is full » si `playerEntityList.size() >= max` (`ServerConfigurationManager.java:578`) ; S01 plafonne à 60 (`:185`) |
| `white-list` | `server.properties` (`DedicatedPlayerList.java:27`) | `false` | `attemptLogin` `:557` |
| `allow-flight` | `server.properties` (`DedicatedServer.java:233`) | `false` (`run/` : `true`) | coupe le kick « Flying is not enabled » (`NetHandlerPlayServer.java:568`) |
| `player-idle-timeout` | `server.properties` (`:237`) | `0` | kick d'inactivité en minutes (`NetHandlerPlayServer.java:249-252`) |
| `view-distance` | `server.properties` | 10 (`run/` : 8) | rayon de chunks envoyés |
| `enable-rcon`, `rcon.port`, `rcon.password` | `server.properties` (`:379`, `RConThreadMain.java:29-30`) | `false`, 0 → port jeu + 10, vide (RCON refusé sans mot de passe `:117`) | RCON TCP |
| `enable-query`, `query.port` | `server.properties` (`:372`, `RConThreadQuery.java:46,74-80`) | `false`, 0 → port jeu | Query UDP |
| `snooper-enabled` | `server.properties` (`DedicatedServer.java:485`) | `true` | voir snooper plus bas |
| `settings.connection-throttle` | `bukkit.yml` (`CraftServer.java:629-636`) | 4000 ms ; `-1` imposé si `bungeecord` | `NetHandlerHandshakeTCP.java:63-75` |
| `settings.ping-packet-limit` | `bukkit.yml` (`CraftServer.java:624`) | 100 | **inutilisé** : aucun appelant trouvé |
| `auto-updater.*` | `bukkit.yml` | `enabled: false` | ignoré : `updater.setEnabled(false)` en dur (`CraftServer.java:250`) |
| `settings.bungeecord` | `spigot.yml` (`SpigotConfig.java:188`) | `false` | forwarding d'IP/UUID/profil (`NetHandlerHandshakeTCP.java:121-145`) |
| `settings.netty-threads` | `spigot.yml` (`SpigotConfig.java:191-197`) | `-1` → cœurs logiques | pose `io.netty.eventLoopThreads` ; **probablement sans effet** (voir pièges) |
| `settings.player-shuffle` | `spigot.yml` (`:237`) | `0` | mélange l'ordre des connexions (`NetworkSystem.java:152`) |
| `settings.timeout-time`, `restart-on-crash` | `spigot.yml` (`:174-180`) | 90 s, `true` | chien de garde du thread principal, **pas** un délai réseau |
| `commands.tab-complete`, `commands.log` | `spigot.yml` (`:135-154`) | `true`, `true` | complétion ; journal des commandes |
| `messages.*` | `spigot.yml` (`:163-171`) | textes anglais | kick plein, liste blanche, version |
| `world-settings.*.max-bulk-chunks` | `spigot.yml` (`SpigotWorldConfig.java:240`) | 5 | chunks par S26 |
| `fml.readTimeout`, `fml.loginTimeout` | propriétés JVM (`FMLNetworkHandler.java:62-63`) | 30 s, 600 ticks | délai de lecture ; délai de login (n'agit que tant que le handler est `NetHandlerLoginServer`) |
| `cauldron.logging.userLogin` | `Gamma.yml` (`CrucibleConfigs.java:100`) | `false` | journal détaillé du login |
| `thermos.logging.clientModList` | `Gamma.yml` (`:124`) | `true` | journalise la liste de mods de chaque client |
| `crucible.logging.packetTimeout`, `packetTimeoutMs` | `Gamma.yml` (`:154,157`) | `false`, 500 | alerte si un paquet dépasse 500 ms |
| `gamma.network.chunkCompressionLevel` | `gammaengine.yml` (`GammaConfig.java`) | 4 (borné 1-9) | niveau de deflate des chunks |

## Services annexes

- **Ping legacy** : `PingResponseHandler` en tête du pipeline (`legacy_query`), thread Netty, répond aux octets `0xFE` (<1.3, 1.4-1.5, 1.6) avec MOTD et nombres de joueurs lus directement (`:44,54,73`), puis ferme ; se retire sinon (`:100`). Aucun interrupteur.
- **Statut moderne** : `NetHandlerStatusServer` : `C00PacketServerQuery` puis `C01PacketPing`, le tout sur Netty (priorité). Appelle `ServerListPingEvent` (plugins) sur ce thread (`:66-88`). Le serveur envoie **deux** `S00PacketServerInfo` (`:58` puis `:94`) et **deux** `S01PacketPong` (`:100,106`). Réponse = MOTD, `players{max,online}`, `version{name,protocol:5}`, favicon, `modinfo`.
- **RCON** : thread « RCON Listener » (`accept`, `RConThreadMain.java:82-100`) + un thread par client (`RConThreadClient`) ; commande exécutée par `MinecraftServer.handleRConCommand` via la file `processQueue` du thread principal (`MinecraftServer.java:1830-1848`). Désactivé par défaut.
- **Query** : un thread UDP « Query Listener », paquet de 1460 octets, clés `hostname, gametype, game_id, version, plugins, map, numplayers, maxplayers, hostport, hostip` puis `player_` (`RConThreadQuery.java:154-195`). Désactivé par défaut.
- **Snooper** : `Timer("Snooper Timer")` qui POSTe vers `http://snoop.minecraft.net/server?version=2` toutes les 900000 ms (`PlayerUsageSnooper.java:28,39,56-81`) ; démarré à partir du tick 100 (`MinecraftServer.java:893-896`). `snooper-enabled=true` par défaut, **y compris sur `run/`**.
- **Vérifications de version** : Forge neutralisée (`ForgeVersion.startVersionCheck` ne fait que `status = UP_TO_DATE`, `ForgeVersion.java:71-74`) ; Bukkit AutoUpdater désactivé en dur (`CraftServer.java:250`, `AutoUpdater.java:72`). Seules connexions sortantes restantes : `MojangNameLookup` (`sessionserver.mojang.com`, `MojangNameLookup.java:26`, lookups de noms), timings (`TimingsExport.java:323`, sur commande), téléchargement de bibliothèques au démarrage (`CrucibleServerMainHook.java:20-24`), session Mojang en mode en ligne.

## Journaux

`src/main/resources/log4j2.xml` : tous les appenders sont synchrones, aucun `AsyncRoot`/`AsyncLogger`/`Async` (`:1-64`), pas de `disruptor` dans `run/libraries`. `File` = `RollingRandomAccessFile` sans `immediateFlush` explicite (défaut Log4j2 : vidage à chaque événement, non vérifié dans le dépôt), écriture sur le thread appelant. Console : `DedicatedServer` retire tous les `ConsoleAppender` (`:161-191`), la console passe par l'appender `Queue` « TerminalConsole » (`log4j2.xml:10-12`) lu par `TerminalConsoleWriterThread` (`S/org/bukkit/craftbukkit/v1_7_R4/util/TerminalConsoleWriterThread.java:35-67`, boucle `Thread.yield()` si file vide, JLine `reader.print/drawLine` par message) lancé en `DedicatedServer.java:195`. La saisie : thread démon « Server console handler » (`:105-149`). `System.out/err` redirigés vers log4j (`:196-197`).

## Pièges pour la suite

1. Tout le jeu reste sur le thread principal, sous le verrou `networkManagers` pendant toute la boucle (`NetworkSystem.java:148-215`). `processing` n'est pas `volatile` (`:56`) : un thread Netty qui ne le voit pas vrai bloque sur `networkManagers.add` jusqu'à la fin du tick.
2. Un `flush` par paquet sortant (`NetworkManager.java:189,203`) plus un flush par tick (`:279`), avec Nagle actif : beaucoup d'appels système à 400 joueurs. La coalescence est à faire au niveau de `dispatchPacket`.
3. La compression des chunks est par joueur et par paquet (S26) sur les threads Netty ; `new Deflater` à chaque S21 (`S21PacketChunkData.java:52`). Le commentaire de `gamma.network.chunkCompressionLevel` dans `GammaConfig.java` dit que le partage inter-joueurs est la suite logique : non fait.
4. `netty-threads` : `SpigotConfig.nettyThreads` pose `io.netty.eventLoopThreads` (`:196`) depuis `DedicatedServer.java:266`, mais `NetworkSystem.eventLoops` est statique et créé dès `new NetworkSystem` (`MinecraftServer.java:221`, `NetworkSystem.java:49` avec `0`) : probablement évalué avant. `run/logs/latest.log:57` affiche « Using 24 threads » sans prouver la taille réelle du groupe (à mesurer avec `jstack`).
5. Chaque login crée un thread (`NetHandlerLoginServer.java:176`) et, si un plugin écoute le `PlayerPreLoginEvent` déprécié, bloque ce thread sur le thread principal (`ThreadPlayerLookupUUID.java:95-97`). Le login lui-même progresse d'une étape par tick (LoginStart non prioritaire).
6. `serverInitiateHandshake` peut dormir jusqu'à 21 × 10 ms (`NetworkDispatcher.java:144-151`) sur le thread qui exécute `handlerAdded` (probablement une boucle Netty avec Netty 4.0.10, non vérifié).
7. ModIdData est reconstruit puis sérialisé à chaque connexion (`FMLHandshakeServerState.java:68`) ; avec 400 reconnexions simultanées c'est un pic CPU et mémoire.
8. Le nettoyage du throttle compare des horodatages à une durée (`NetHandlerHandshakeTCP.java:90`) : il vide la table tous les 200 logins ; la limite de 4 s par IP s'applique donc, mais elle est contournable en rafale. `127.0.0.1` est exempté (`:68`).
9. Journaux par connexion : « UUID of player … », « logged in with entity id … » (`ServerConfigurationManager.java:177`), liste de mods FML, `moved too quickly`/`moved wrongly` en WARN : 400 bots multiplient les écritures synchrones.
10. L'entrée en jeu d'un joueur modé se termine sur un thread Netty (voir « Fil d'exécution du handshake ») : c'est déjà du travail de monde hors du thread principal, non sérialisé avec le tick. À prendre en compte avant de déplacer d'autres éléments hors du thread principal, et pour les rafales de connexions du banc.
11. Le keep-alive est piloté en ticks (40 ticks, `NetHandlerPlayServer.java:225`), pas en temps ; le serveur ne déconnecte jamais sur absence de réponse (`:2382-2389` met seulement `ping` à jour). Seul le `ReadTimeout` de 30 s coupe.

## État des éléments de la feuille de route

| Élément | État | Preuve | Interrupteur |
|---|---|---|---|
| Réseau hors du thread principal | partiel | E/S, encodage, compression et 5 types de paquets prioritaires sur Netty (`NetworkSystem.java:49`, `NetworkManager.java:139-141`) ; tout le jeu traité par `networkTick` (`:243-273`) | aucun |
| Logs hors du thread principal | absent | `log4j2.xml` tout synchrone ; seule la console passe par une file (`log4j2.xml:10-12`) | aucun |
| Compression des chunks hors thread | présent (Netty), non mutualisé | `S26PacketMapChunkBulk.java:89,213`, `MessageSerializer.java:46` ; ajouts GammaEngine pour la sûreté multi-thread | `gamma.network.chunkCompressionLevel` (niveau seulement) |
| Visibilité des entités par joueur | présent (vanilla/Spigot), inchangé | `EntityTrackerEntry.java:409-433` (rayon, chunk surveillé, `canSee`) ; `entity-tracking-range` (`spigot.yml:79-84`) | `entity-tracking-range.*` |
| Support proxy et forwarding | partiel | BungeeCord : `NetHandlerHandshakeTCP.java:121-145` ; Velocity/PROXY protocol : non trouvé (cherché `velocity`, `haproxy`, `PROXY protocol` dans `S/` et `net/minecraft/{network,server}`) | `settings.bungeecord` (`spigot.yml`) |
| Ping legacy | présent | `PingResponseHandler.java:25-104` | aucun |
| RCON | présent | `RConThreadMain.java`, `DedicatedServer.java:379` | `enable-rcon`, `rcon.password` |
| Query | présent | `RConThreadQuery.java`, `DedicatedServer.java:372` | `enable-query` |
| Snooper | présent, actif par défaut | `PlayerUsageSnooper.java:39,81` ; `MinecraftServer.java:893` | `snooper-enabled` |
| Vérifications de version | absent (neutralisées) | `ForgeVersion.java:73` ; `CraftServer.java:250` ; `AutoUpdater.java:72` | sans objet |
