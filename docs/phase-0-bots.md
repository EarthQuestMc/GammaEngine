# Cahier des charges d'un bot de charge (Minecraft 1.7.10, protocole 5, Forge, mode hors ligne)

Chemins relatifs à la racine du dépôt. `E/` = `eclipse/cauldron/src/main/java/`, `S/` = `src/main/java/`. Constat de départ : le serveur de test `run/` est en `online-mode=false`, `allow-flight=true`, `max-players=100`, sans liste blanche, **sans mod ni plugin** (`run/mods` et `run/plugins` vides, `run/server.properties`). Les affirmations viennent de la lecture du code ; rien n'a été exécuté.

## Principes de codage

- Trame : `VarInt longueur`, `VarInt id`, charge. **Aucune compression** en protocole 5 ; le chiffrement n'existe qu'en mode en ligne (`NetHandlerLoginServer.java:167-177`), donc jamais pour le banc.
- `String` = `VarInt` (nombre d'octets UTF-8) + octets. `double`, `float`, `int` en gros-boutiste. Le bot peut **sauter** tout paquet serveur inconnu grâce à la longueur : il n'a besoin de décoder que S00, S01, S06, S08, S3F (canal `FML|HS`) et les déconnexions.
- Identifiants : `E/net/minecraft/network/EnumConnectionState.java:108-232`.

## Séquence de paquets

### A. Statut (une fois par exécution du banc, pour la liste de mods)
1. C→S, état HANDSHAKING, id 0x00 `C00Handshake` : `VarInt 5` (protocole), `String` hôte, `UShort` port, `VarInt 1` (statut). Lecture `E/net/minecraft/network/handshake/client/C00Handshake.java:31-37`, traitement sur Netty (paquet prioritaire, `:52`) par `NetHandlerHandshakeTCP.processHandshake` cas 2 (`E/net/minecraft/server/network/NetHandlerHandshakeTCP.java:150-153`). Le statut ne passe pas par le throttle.
2. C→S, état STATUS, 0x00 `C00PacketServerQuery` (vide) → `NetHandlerStatusServer.processServerQuery` (`…/NetHandlerStatusServer.java:55`).
3. S→C 0x00 `S00PacketServerInfo` : `String` JSON `{"description","players":{"max","online"},"version":{"name","protocol":5},"favicon"?,"modinfo":{"type":"FML","modList":[{"modid","version"}…]}}` (`ServerStatusResponse.java:266`, `FMLNetworkHandler.java:229-243`). **Reçu deux fois** : une première réponse par défaut (`NetHandlerStatusServer.java:58`), puis celle de l'événement `ServerListPingEvent` (`:94`). Garder la première `modList`.
4. C→S 0x01 `C01PacketPing` (`long`) → S→C 0x01 `S01PacketPong` (`long`), **émis deux fois** puis fermeture (`NetHandlerStatusServer.java:98-106`). Le bot peut fermer dès le premier pong.

### B. Connexion et login (par bot)
5. Nouvelle connexion TCP. C→S 0x00 `C00Handshake` : `VarInt 5`, `String` hôte (n'importe lequel si `bungeecord: false`), `UShort` port, `VarInt 2` (login). Cas 1 : throttle (`NetHandlerHandshakeTCP.java:59-103`), version exacte 5 (`:105-116`, sinon kick « Outdated … »), puis `NetHandlerLoginServer` en place (`:119`).
6. C→S, état LOGIN, 0x00 `C00PacketLoginStart` : `String` nom, **16 caractères au plus** (`E/net/minecraft/network/login/client/C00PacketLoginStart.java:25`). Non prioritaire : traité par le thread principal au tick suivant (`NetHandlerLoginServer.processLoginStart`, `NetHandlerLoginServer.java:160-178`). Hors ligne : pas de S01 Encryption Request ; démarre un thread « User Authenticator #N » (`:176`) qui fixe l'UUID `UUID.nameUUIDFromBytes("OfflinePlayer:"+nom)` (`:94-114`) et lance `AsyncPlayerPreLoginEvent` (`S/net/minecraft/server/network/ThreadPlayerLookupUUID.java:66-110`).
7. Au tick suivant `NetHandlerLoginServer.onNetworkTick` (`:65-70`) appelle `func_147326_c` (`:117-142`) : `attemptLogin` (bannissement, liste blanche, plein, `PlayerLoginEvent` : `ServerConfigurationManager.java:530-593`). Succès : S→C 0x02 `S02PacketLoginSuccess` : `String` UUID **avec tirets**, `String` nom (`E/net/minecraft/network/login/server/S02PacketLoginSuccess.java`). Échec : S→C 0x00 (état LOGIN) `S00PacketDisconnect`, `String` JSON du message. **Après le 0x02, tout est en état PLAY.**

### C. Handshake FML (canal `FML|HS`, en PLAY ; détails dans la [carte du réseau](carte/reseau.md))
Chaque message FML est un S3F 0x3F (serveur→client) ou un C17 0x17 (client→serveur) : `String` canal (≤ 20 caractères), longueur, octets. Longueur S3F = **VarShort** Forge (`short` ; si le bit 0x8000 est posé, un octet de plus donne les bits 15 à 22, `ByteBufUtils.java:64-74`), longueur C17 = `short` simple, charge lue seulement si 0 < longueur < 32767 (`C17PacketCustomPayload.java:47-53`). Charge FML = `byte` discriminant + corps (`FMLIndexedMessageToMessageCodec.java:49-50`, `FMLHandshakeCodec.java:10-15`).
8. S→C S3F `REGISTER` : `"FML|HS\0FML\0<canaux de mods séparés par \0>"` (`FMLHandshakeMessage.java:26-31`).
9. S→C S3F `FML|HS` `[0x00][byte 2][int dimension du joueur]` : ServerHello. Le serveur a créé le `NetHandlerPlayServer` et passé en PLAY (`NetworkDispatcher.java:141-176`).
10. C→S C17 `REGISTER` : `"FML|HS\0FML"` (copie du comportement du vrai client, `FMLHandshakeClientState.java:45`). Les canaux de mods sont facultatifs pour le banc.
11. C→S C17 `FML|HS` `[0x01][byte 2]` : ClientHello (`FMLHandshakeMessage.java:76-93`).
12. C→S C17 `FML|HS` `[0x02][varint n][n × (String modid, String version)]` : ModList, **recopie de la `modList` du statut**. Ici `varint` et `String` sont ceux de Forge : varint 7 bits, poids faible d'abord, sur 2 octets au plus pour n et pour la longueur des chaînes (`ByteBufUtils.java:98-136`). Verrou : `FMLNetworkHandler.checkModList` (`FMLNetworkHandler.java:160-190`) refuse si un mod du serveur à vérification par défaut manque ou n'a pas la même version (`NetworkModHolder.java:54-59,202-210`) ; refus = S40 « Mod rejections [...] ».
13. S→C S3F `FML|HS` `[0x02]` ModList du serveur (reçue et ignorée par le bot).
14. C→S C17 `FML|HS` `[0xFF][0x02]` : ACK. Le serveur ne lit pas la valeur (`FMLHandshakeServerState.java:61-74`), tout message suffit.
15. S→C S3F `FML|HS` `[0x03]…` ModIdData (grosse charge, à **sauter** sans la décoder, `FMLHandshakeServerState.java:68`), puis `[0xFF][0x02]` ACK.
16. C→S C17 `FML|HS` `[0xFF][0x03]` : ACK. Le serveur répond `[0xFF][0x03]` et termine (`:75-86`) ; le vrai client enverrait encore `[0xFF][0x04]` après l'ACK n°2 du serveur et `[0xFF][0x05]` après l'ACK n°3 ; ils sont ignorés (`:87-94`) mais peuvent être envoyés par fidélité.
17. Un bot qui n'envoie pas ces messages ne rejoint jamais : voir « client vanilla » dans la [carte du réseau](carte/reseau.md) (10 h).

### D. Entrée en jeu et maintien
18. S→C, dans cet ordre approximatif (`ServerConfigurationManager.java:200-238`) : 0x01 `S01PacketJoinGame` (`int` id d'entité, `ubyte` mode de jeu avec bit 8 = hardcore, `byte` dimension, `ubyte` difficulté, `ubyte` joueurs max, `String` type de monde ; `S01PacketJoinGame.java`), 0x3F `MC|Brand`, 0x05 spawn, 0x39 capacités, 0x09 slot, 0x38 liste des joueurs, **0x08 `S08PacketPlayerPosLook`** (`double x, double y, double z, float yaw, float pitch, boolean onGround` ; `y` = pieds + 1,62), temps 0x03, météo 0x2B, puis les chunks (0x26 `S26PacketMapChunkBulk` : `short n`, `int len`, `bool skylight`, `len` octets zlib, puis n × (`int x, int z, ushort primaire, ushort ajout`) ; ou 0x21 `S21PacketChunkData`). **Lecture ou saut** : sauter par la longueur de trame suffit, le serveur ne demande aucun accusé de chunk.
19. C→S 0x15 `C15PacketClientSettings` (facultatif, recommandé) : `String` langue (**≤ 7**), `byte` distance de vue, `byte` visibilité du chat (0 = tout), `bool` couleurs, `byte` difficulté, `bool` cape (`C15PacketClientSettings.java:38`). Fixe la distance de vue côté serveur.
20. **Confirmation de position, obligatoire.** Sur 0x08, répondre immédiatement C→S 0x06 `C06PacketPlayerPosLook` : `double x`, `double y_pieds` (= y reçu − 1,62), `double stance` (= y reçu), `double z`, `float yaw`, `float pitch`, `bool onGround` (`C03PacketPlayer.java:210-230`). Tant que x et z ne sont pas **égaux au bit près** à la position serveur et que |y_pieds − y serveur|² < 0,01, `hasMoved` reste faux (`NetHandlerPlayServer.java:317-325`), les mouvements sont ignorés, et le serveur renvoie un S08 toutes les 20 ticks de réseau (`:596-599`, événement `PlayerTeleportEvent` à chaque fois). Refaire cette confirmation après chaque 0x08 (téléportation, réapparition).
21. **Keep-alive.** Le serveur envoie 0x00 `S00PacketKeepAlive` (`int` id) tous les 40 ticks de réseau (`NetHandlerPlayServer.java:225-231`). Répondre 0x00 `C00PacketKeepAlive` avec le même `int` ; traité sur Netty (`:2382-2389` ne fait que mettre à jour `ping`). Ne pas répondre n'expulse pas.
22. **Mouvement.** Chaque tick (50 ms) : C→S 0x04 `C04PacketPlayerPosition` (`x, y_pieds, stance, z, onGround`) ou 0x03 `C03PacketPlayer` (`onGround` seul) ; au moins un paquet toutes les < 30 s (sinon `ReadTimeout`). Le serveur recalcule la position (`NetHandlerPlayServer.java:299-600`) : un bot sur le sol envoie `onGround=true`.
23. **Mort et réapparition.** S→C 0x06 `S06PacketUpdateHealth` (`float` vie, `short` faim, `float` saturation) avec vie ≤ 0 : envoyer C→S 0x16 `C16PacketClientStatus` `byte 0` (`C16PacketClientStatus.java:48`). Accepté seulement si la vie est ≤ 0 (`NetHandlerPlayServer.java:1673`) ; le monde n'est pas hardcore (sinon bannissement `:1657-1670`). Réponse : 0x07 `S07PacketRespawn` (`int` dimension, `byte` difficulté, `byte` mode, `String` type) puis un nouveau 0x08, à reconfirmer (étape 20).
24. Déconnexion serveur : S→C 0x40 `S40PacketDisconnect` (`String` JSON). Fermeture propre côté bot : fermer le socket (le serveur voit `endOfStream`).

## Obstacles qui expulsent ou bloquent un bot

| Obstacle | Fichier et clé | Défaut | Classe qui l'applique | Réglage conseillé pour le banc |
|---|---|---|---|---|
| Throttle de connexion par adresse IP | `bukkit.yml` `settings.connection-throttle` | 4000 ms (`run/` : 4000) | `NetHandlerHandshakeTCP.java:59-75` ; `CraftServer.java:629-636` | `-1` si les bots partent d'une autre machine (même IP) ; `127.0.0.1` est exempté (`:68`). Le throttle ne compte que les logins, pas le statut |
| Limite de connexions par IP | non trouvé (cherché dans `NetHandlerHandshakeTCP`, `CrucibleConfigs.java`, `SpigotConfig.java`) | aucune | — | sans objet |
| Serveur plein | `server.properties` `max-players` | 20 (`run/` : 100) | `ServerConfigurationManager.java:578` ; `DedicatedPlayerList.java:26` | ≥ nombre de bots + marge (ex. 500). Ne compte que les joueurs déjà entrés |
| Liste blanche | `server.properties` `white-list` | `false` | `ServerConfigurationManager.java:557` | laisser `false` |
| Bannissement par nom ou IP | `banned-players.json`, `banned-ips.json` | vides | `ServerConfigurationManager.java:544-574` | ne pas bannir |
| Version de protocole | — | 5 exigé | `NetHandlerHandshakeTCP.java:105-116` | envoyer 5 |
| Rejet des mods par FML | — | liste exacte des mods du serveur | `FMLNetworkHandler.java:160-190` | recopier `modinfo.modList` du statut |
| Même nom connecté deux fois | — | l'ancien est expulsé « You logged in from another location » | `ServerConfigurationManager.java:643-653` | noms uniques, `[A-Za-z0-9_]`, ≤ 16 |
| Délai de login | propriété JVM `fml.loginTimeout` | 600 ticks de réseau | `NetHandlerLoginServer.java:72` (n'agit que tant que le handler de login est actif, avant le handshake FML) | envoyer `LoginStart` sans délai ; à 5 TPS, 600 ticks durent 2 minutes |
| Délai de lecture réseau | propriété JVM `fml.readTimeout` | 30 s sans octet reçu | `NetworkSystem.java:94` ; `FMLNetworkHandler.java:62` | émettre au moins C00 ou C03 toutes les 10 s |
| Keep-alive | — | pas d'expulsion si absent | `NetHandlerPlayServer.java:2382-2389` | répondre quand même (mesure du ping) |
| Mouvement trop rapide (« moved too quickly ») | — | distance² > 100 par paquet | `NetHandlerPlayServer.java:509-515` | pas de saut > 10 blocs par paquet ; sinon simple retour arrière et WARN, pas de kick |
| Mouvement erroné (« moved wrongly ») | — | écart² > 0,0625 après collision, hors créatif | `NetHandlerPlayServer.java:543-564` | WARN + retour arrière ; rester sur un sol réel |
| Position illégale / NaN | — | `|x|,|z| > 3,2e7` ou NaN | `NetHandlerPlayServer.java:302-308,475-479` | ne jamais envoyer de NaN |
| Vol (« Flying is not enabled ») | `server.properties` `allow-flight` | `false` (`run/` : `true`) | `NetHandlerPlayServer.java:568-581` (81 paquets sans bloc dessous, hors créatif) | `true` |
| Inactivité | `server.properties` `player-idle-timeout` | `0` (désactivé) | `NetHandlerPlayServer.java:249-252` | laisser `0` |
| Anti-spam chat et commandes | — | +20 par message ou commande, kick `disconnect.spam` au-delà de 200 ; −1 par tick de réseau | `NetHandlerPlayServer.java:234,1171-1206` ; ops exemptés (`:1173`) | ≤ 1 message/s par bot, ou bot opérateur |
| Chat : texte | — | ≤ 100 caractères, caractères interdits → kick | `C01PacketChatMessage.java:28` ; `NetHandlerPlayServer.java:1071-1107` | ASCII imprimable |
| Chien de garde du serveur | `spigot.yml` `settings.timeout-time` | 90 s | `SpigotConfig.java:174-180` | n'agit pas sur les bots ; sert si le tick se bloque |
| Plugins de connexion | `plugins/` | aucun plugin sur `run/` | `AsyncPlayerPreLoginEvent`, `PlayerLoginEvent` | sur un vrai serveur, vérifier les plugins d'authentification et d'anti-bot |
| Raccourci d'IP/UUID BungeeCord | `spigot.yml` `settings.bungeecord` | `false` | `NetHandlerHandshakeTCP.java:121-145` | `false` (sinon l'hôte doit contenir 3 ou 4 champs séparés par `\0`) |
| Hôte inconnu `server-ip` | `server.properties` `server-ip` | vide | `DedicatedServer.java:227` | vide |

## Ce que chaque bot crée côté serveur

- **Données joueur** : `world/playerdata/<uuid>.dat` écrit à la déconnexion et à chaque sauvegarde automatique (`SaveHandler.java:266` ; `ServerConfigurationManager.java:349-358,448`) ; `bukkit.yml` `ticks-per.autosave: 6000`. L'UUID hors ligne est déterministe : un même nom retrouve son fichier.
- **Statistiques** : `world/stats/<uuid>.json`, créé à l'entrée (`ServerConfigurationManager.java:1732-1733`) et écrit avec les données joueur (`:358`).
- **Cache de profils** : `usercache.json` (`ServerConfigurationManager.java:155`), enregistré avec au plus 1000 entrées (`PlayerProfileCache.java:275`).
- **Journaux** : par login, un message « UUID of player … » (`ThreadPlayerLookupUUID.java:109`), « logged in with entity id … » (`ServerConfigurationManager.java:177`), « Client attempting to join with N mods : … » (`thermos.logging.clientModList=true`, `FMLHandshakeServerState.java:44-49`) dans `logs/fml-server-latest.log`, plus les WARN de mouvement. Mettre `thermos.logging.clientModList: false` dans `Gamma.yml` pour le banc (`CrucibleConfigs.java:124`).
- **Mémoire serveur** : une entité `EntityPlayerMP`, un `NetHandlerPlayServer`, un `NetworkDispatcher` + `EmbeddedChannel` FML par bot (`NetworkDispatcher.java:101-112`), une file de chunks (`loadedChunks`) et les chunks eux-mêmes ; le thread « User Authenticator » est éphémère.
- **Charge réseau à l'entrée** : le ModIdData est reconstruit pour chaque bot (`FMLHandshakeServerState.java:68`), lot de 5 chunks par S26 (`SpigotWorldConfig.java:240`) compressés au niveau `gamma.network.chunkCompressionLevel` (4).

## Commandes utiles pour piloter les bots

Depuis la console du serveur ou RCON (`enable-rcon=true`, `rcon.password`, `RConThreadMain.java:117`) ; la console n'a pas d'anti-spam.
- `op <nom>` : écrit `ops.json` ; l'opérateur est exempté de l'anti-spam de chat (Bukkit `OpCommand`, `SimpleCommandMap.java:43`). Ne rend pas les bots invulnérables.
- `gamemode <0|1|2> <nom>` (Bukkit `GameModeCommand`, `SimpleCommandMap.java:58`) : le créatif lève le contrôle de vol et de « moved wrongly » (`NetHandlerPlayServer.java:546,568`). Autre option : `defaultgamemode`.
- `tp <nom> <x> <y> <z>` (Bukkit `TeleportCommand`, `SimpleCommandMap.java:50`) : provoque un S08 que le bot **doit** confirmer (étape 20).
- `spreadplayers <x> <z> <distance> <rayonMax> <respecterEquipes> <noms…>` (commande vanilla, `E/net/minecraft/command/CommandSpreadPlayers.java:27`) : disperse les bots ; non testé via l'enrobage vanilla de Cauldron.
- Divers : `kill <nom>` (mort pour tester la réapparition), `time set`, `gamerule` (vanilla), `whitelist`, `kick <nom>`, `ban`.
- Tous les bots naissent au point d'apparition du monde (`worldserver.getSpawnPoint()`, `ServerConfigurationManager.java:179`) puis à leur dernière position si un fichier existe ; `bukkit.yml` `settings.use-exact-login-location: false`.

## Réglages conseillés pour un banc de 400 bots

`server.properties` : `online-mode=false`, `max-players=500`, `allow-flight=true`, `white-list=false`, `player-idle-timeout=0`, `snooper-enabled=false` ; le banc pilote le serveur par l'entrée standard de sa console, sans ouvrir RCON. `bukkit.yml` : `connection-throttle: -1`. `Gamma.yml` : `thermos.logging.clientModList: false`. Rythme de connexion : une étape de login par bot et par tick, plus un thread éphémère par login ; viser 5 à 10 connexions par seconde plutôt qu'une rafale (le join FML se termine sur un thread Netty, voir la [carte du réseau](carte/reseau.md), « Pièges » n° 10).

## Machine d'états minimale du bot

1. `CONNECT` : TCP, Nagle coupé côté bot.
2. `HANDSHAKE_LOGIN` : envoyer 0x00 handshake (suivant 2) puis 0x00 `LoginStart`.
3. `WAIT_LOGIN_SUCCESS` : lire des trames ; 0x02 → état PLAY ; 0x00 → journaliser le motif, terminer.
4. `FML_HANDSHAKE` : sur S3F `REGISTER` répondre C17 `REGISTER` ; sur `FML|HS` 0x00 envoyer ClientHello puis ModList ; sur 0x02 envoyer ACK(2) ; sur 0x03 ne rien faire ; sur 0xFF avec phase 2 envoyer ACK(3) ; **sortie** dès le premier S01 JoinGame (0x01) ou le premier S08.
5. `PLAY` : à chaque trame, selon l'id : 0x00 keep-alive → écho ; 0x08 → confirmer par C06 ; 0x06 → si vie ≤ 0, C16 `byte 0` ; 0x40 → journaliser et fermer ; 0x3F → ignorer sauf `FML|HS` tardif (HandshakeReset 0xFE, ignorer) ; autres ids → sauter.
6. Chaque 50 ms : envoyer C04 (ou C03) ; chaque message de chat éventuel ≤ 100 caractères, ≤ 1 par seconde.
7. Mesures utiles par bot : temps entre `LoginStart` et S01 JoinGame, temps entre S08 et réception du premier S26, intervalle entre keep-alive successifs (dérive = retard de tick serveur), RTT du keep-alive, nombre de S08 reçus après la confirmation initiale (doit rester nul sans téléportation).

## Points à vérifier au premier essai (non établis par la lecture)

- Que `spreadplayers` fonctionne depuis la console avec l'enrobage vanilla de Cauldron.
- Que l'ordre exact des paquets S→C après le ACK(3) correspond à l'étape 18 (lecture de `initializeConnectionToPlayer` seulement, `ServerConfigurationManager.java:149-250`).
- Le nombre réel de threads Netty (`jstack`, noms « Netty IO #N ») et sur quel thread s'exécute `initializeConnectionToPlayer` (nom du thread dans le journal : `NetworkDispatcher.java:197` journalise `Thread.currentThread().getName()` dans « Server side … connection established »).
- Le comportement d'un bot qui oublie le C06 de confirmation : un S08 par seconde de jeu côté serveur (`NetHandlerPlayServer.java:596-599`), utile comme test de charge sur `PlayerTeleportEvent`.
