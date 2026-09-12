# Passer à 400 joueurs, et ce que « consommer moins » veut dire concrètement

Ce document répond à une seule question : qu'est-ce qui empêche un serveur 1.7.10 Forge + Bukkit de
tenir 20 TPS avec 250 à 400 joueurs sur un gros modpack, et que doit faire le moteur pour y
remédier. Chaque affirmation est soit mesurée sur ce dépôt, soit déduite de code cité avec son
fichier et sa ligne.

La conclusion d'abord : **la boucle de tick n'est pas le premier mur.** Trois coûts qui croissent
plus vite que le nombre de joueurs arrivent avant, et aucun n'était dans la feuille de route
initiale. Ils sont maintenant les phases 2B et 11.

## Le budget

La cible matérielle est 8 cœurs physiques à 3,8 GHz de base, 16 threads matériels, 128 Go de RAM,
NVMe.

| Ressource | Par tick à 20 TPS | Par joueur à 400 |
| --- | --- | --- |
| CPU, parallélisme parfait | 400 cœur-ms | 1,00 cœur-ms |
| CPU, réaliste (25 % pour GC, Netty, IO, OS) | 300 cœur-ms | 0,75 cœur-ms |
| CPU à 250 joueurs | 300 cœur-ms | 1,20 cœur-ms |

Tout le problème d'ingénierie tient dans cette ligne : **chaque joueur doit coûter moins d'une
milliseconde de CPU par tick**, ses entités, ses chunks, ses paquets et sa part de simulation du
monde compris. Tout ce dont le coût par joueur augmente avec le nombre de joueurs casse le budget
avant que les cœurs ne soient saturés.

## Mur 1 : le tracking d'entités est quadratique

`EntityTracker.updateTrackedEntities()`
(`eclipse/cauldron/src/main/java/net/minecraft/entity/EntityTracker.java:315`) fait deux choses à
chaque tick :

1. Pour chaque entité suivie, `sendLocationToAllClients(theWorld.playerEntities)`. Dès que l'entité
   a bougé de plus de 4 blocs, cet appel parcourt **toute la liste des joueurs**
   (`EntityTrackerEntry.java:551`).
2. Pour chaque joueur qui a bougé, il reparcourt **tout l'ensemble des entités suivies**
   (`EntityTracker.java:332`).

Le coût est donc proportionnel à joueurs × entités. Sur un monde moddé avec 30 000 entités et 300
des 400 joueurs qui bougent dans un tick donné :

| Grandeur | Valeur |
| --- | --- |
| Itérations par tick | 9 000 000 |
| À 5 ns par itération | 45 ms d'un cœur |

Cela consomme le tick entier sur un seul cœur, et on ne peut pas simplement paralléliser puisque
chaque itération peut envoyer un paquet. Les réglages de portée de tracking de Spigot, que Crucible
embarque déjà (`org/spigotmc/TrackingRange.java`), réduisent la constante mais pas l'exposant.

La correction consiste à arrêter de balayer. Les entités sont déjà indexées par chunk, et la portée
de tracking maximale est de 64 blocs, soit 4 chunks. Un joueur n'a besoin de considérer que les
entités des 81 chunks autour de lui :

| Approche | Itérations par tick |
| --- | --- |
| Aujourd'hui, 300 joueurs en mouvement × 30 000 entités | 9 000 000 |
| Indexé par chunk, 300 joueurs × 81 chunks × ~5 entités | 121 500 |

Soit 75 fois moins de travail, et contrairement à la boucle actuelle cela se parallélise proprement
puisque deux joueurs dans des régions différentes ne partagent rien.

## Mur 2 : le streaming de chunks est quadratique en distance de vue

Une colonne de chunk 1.7.10 avec 16 sections peuplées se sérialise en 16 × 10 240 = 163 840 octets
avant compression (`S21PacketChunkData.func_149269_a`), puis le paquet la compresse
(`S21PacketChunkData.java:46`, `S26PacketMapChunkBulk.java:74`).

Mesuré sur cette machine, sur des données en forme de chunk, avec le harnais de `NativeEngineTest` :

| Niveau deflate | Débit | Taille de sortie |
| --- | --- | --- |
| 1 | 462 Mio/s | 19 989 o |
| 4 (ce que Minecraft utilise pour les paquets de chunks) | 272 Mio/s | 19 283 o |
| 6 (ce qu'utilisent les fichiers region) | 196 Mio/s | 19 268 o |

Le niveau 4 coûte 39 % de CPU en plus que le niveau 1 et économise 3,5 % des octets. Sur des données
de chunk la redondance est faite de longues suites de blocs identiques, que le niveau 1 capture
déjà ; les niveaux supérieurs cherchent des correspondances qui n'existent pas.

Le volume maintenant. Un joueur qui franchit une frontière de chunk en distance de vue 8 reçoit
2 × 8 + 1 = 17 nouvelles colonnes. Un joueur qui sprinte franchit environ 0,35 frontière par
seconde, soit à peu près 6 colonnes par seconde.

| Scénario | Colonnes/s | Entrée deflate | CPU niveau 4 | CPU niveau 1 |
| --- | --- | --- | --- | --- |
| 400 joueurs se déplaçant en terrain neuf | 2 400 | 393 Mo/s | 1,42 cœur | 0,83 cœur |
| 100 joueurs en mouvement (moyenne réaliste) | 600 | 98 Mo/s | 0,36 cœur | 0,21 cœur |

Et cela alloue : 393 Mo/s de gros tableaux d'octets dans le pire cas, ce qui est la source dominante
de déchets sur un serveur chargé.

Trois correctifs, par effet décroissant :

1. **Distance de vue adaptative.** Le nombre de chunks chargés par joueur suit (2·DV+1)². Passer de
   8 à 6 retire 42 % de tous les coûts par chunk du serveur d'un seul coup : mémoire, streaming,
   ticking, sauvegarde. C'est le plus gros levier de tout le projet et il ne coûte presque rien au
   joueur s'il n'est appliqué que sous pression.
2. **Cache partagé de charge utile de chunk.** Les octets extraits et compressés d'un chunk ne
   dépendent que du chunk, pas du joueur. Deux cents joueurs au spawn paient aujourd'hui deux cents
   fois le même travail. On met la charge utile compressée en cache et on l'invalide à la
   modification du chunk.
3. **Compression hors du thread**, sur le pool de workers de chunks, avec la bibliothèque native si
   elle est présente.

## Mur 3 : le blocage de la sauvegarde automatique

`MinecraftServer.tick()` appelle `saveAllWorlds(true)` tous les `ticks-per.autosave`
(`MinecraftServer.java:869`), qui appelle `worldserver.saveAllChunks(true, null)` puis `flush()`
(`MinecraftServer.java:519`) **sur le thread serveur**.

Avec 100 000 chunks chargés et 20 % d'entre eux modifiés, cela fait 20 000 chunks × 164 ko = 3,3 Go
d'entrée deflate au niveau 6, soit environ **17 secondes de travail monothread dans un seul tick**.
En pratique les hébergeurs contournent en augmentant la période de sauvegarde ou en la désactivant,
ce qui échange un blocage contre une perte plus grande en cas de crash.

Le correctif est la sauvegarde continue : une file de chunks sales vidée avec un budget fixe par
tick, snapshot sur le thread propriétaire, compression et écriture sur le pool d'entrées-sorties. La
sauvegarde automatique devient alors un flush qui n'a presque plus rien à faire, et un crash perd
des secondes au lieu de minutes.

## Mémoire

Une colonne de chunk moddée avec 8 sections peuplées coûte environ 100 ko rien qu'en données de
blocs (`ExtendedBlockStorage` : 4096 octets de blocs, 2048 de bits hauts, 2048 de métadonnées, 2048
de lumière de bloc, 2048 de lumière du ciel par section), avant les entités, les TileEntities et le
NBT.

| Distance de vue | Chunks par joueur | Chunks à 400 joueurs dispersés | Données de blocs |
| --- | --- | --- | --- |
| 8 | 289 | 115 600 | 11,6 Go |
| 6 | 169 | 67 600 | 6,8 Go |
| 4 | 81 | 32 400 | 3,2 Go |

128 Go de RAM couvrent n'importe lequel de ces cas. Le problème n'est pas la capacité, c'est le
ramasse-miettes : un tas de 40 Go avec un gigaoctet d'allocations par seconde produit des pauses qui
se voient directement dans le p99 du temps de tick. Consommer moins de mémoire ici veut dire allouer
moins, pas acheter plus de RAM, et c'est le chemin de streaming de chunks qui alloue.

## Réseau

| Trafic | Volume à 400 joueurs |
| --- | --- |
| Mises à jour d'entités, ~100 entités visibles chacune à 20 Hz, ~15 o | 12 Mo/s, 96 Mbit/s |
| Streaming de chunks, pire cas 2 400 colonnes/s à 20 ko | 48 Mo/s, 384 Mbit/s |

La bande passante n'est pas la contrainte sur un lien de 1 à 5 Gbit/s, mais elle coûte de l'argent,
elle sature en pic quand cinquante joueurs se connectent en même temps, et chaque octet envoyé est
aussi un octet compressé, copié et mis en file. Baisser le coût réseau se fait dans cet ordre, du
plus efficace au moins efficace :

1. **Envoyer moins de chunks.** La distance de vue adaptative retire 42 % des colonnes envoyées en
   passant de 8 à 6. Aucun autre levier n'approche ce chiffre.
2. **Ne pas envoyer deux fois la même chose.** Le cache de charge utile évite de recompresser, et le
   suivi des chunks déjà envoyés à un joueur évite de renvoyer ce qu'il possède déjà.
3. **Mieux compresser quand c'est gratuit.** Sans cache, monter le niveau de compression coûte du
   CPU par joueur, donc on reste bas. Avec le cache, la compression a lieu une fois par chunk pour
   tous les joueurs : le niveau peut alors monter, et chaque octet économisé l'est pour tout le
   monde. Le réglage de compression et le cache sont donc une seule décision, pas deux.
4. **Ne pas envoyer ce que le joueur ne peut pas voir.** Portées de tracking ajustées, entités
   lointaines mises à jour moins souvent, pas de paquet pour une donnée inchangée.
5. **Contre-pression sur les clients lents.** Une connexion congestionnée retarde ou abandonne son
   propre flux de chunks, jamais le tick.

## Ce que cela change dans le plan

Déjà couvert par les phases existantes : tick parallèle par régions, chargement et sauvegarde
asynchrones des chunks, compresseur natif, détection de conflits.

**Manquant, et maintenant ajouté :**

| Manque | Où il va |
| --- | --- |
| Tracking d'entités indexé par chunk | Phase 2B |
| Cache partagé de charge utile, compression hors thread et niveau réglé | Phase 2B |
| Budget d'envoi de chunks par joueur et contre-pression sur clients lents | Phase 2B |
| Sauvegarde continue étalée, données joueur asynchrones | Phase 2B |
| Distance de vue et de simulation adaptatives pilotées par le runtime | Phase 11 |
| Caps de mobs, throttling des TileEntities, fusion d'items sous pression | Phase 11 |
| Comptabilité des coûts par joueur, par mod et par plugin | Phase 11 |
| Réduction des allocations du chemin de streaming, budget mémoire des caches | Transverse |
| Configuration GC documentée par taille de tas | Transverse |

La phase 2B est volontairement placée avant le travail sur les régions. Elle n'a besoin d'aucun
modèle de propriété, d'aucun suivi d'accès et d'aucun tick parallèle : c'est du travail purement
algorithmique sur des chemins aujourd'hui quadratiques, et c'est rentable dès le serveur monothread.
La faire en premier rend aussi le travail parallèle qui suit plus facile à mesurer, parce que le
bruit qu'elle supprime est plus gros que les gains qu'elle masquerait.

## Politique de dégradation, ou comment un serveur refuse de descendre sous 20 TPS

Consommer moins n'est pas seulement un problème d'optimisation, c'est un problème de régulation. Le
moteur mesure son propre budget de tick ; quand ce budget est menacé, il doit sacrifier de la
qualité avant de sacrifier la fréquence de tick, et le faire lui-même, dans cet ordre :

1. Baisser la distance de vue des joueurs dans les zones les plus chargées, un cran à la fois.
2. Ralentir les TileEntities dont il a été prouvé qu'elles le tolèrent.
3. Resserrer les portées d'activation et les caps de mobs dans les zones chargées mais sans joueur.
4. Réduire la fréquence de tracking des entités lointaines.
5. Seulement ensuite, laisser le tick s'allonger.

Tout est réversible et restauré automatiquement une fois la pression passée. Aucun administrateur ne
règle quoi que ce soit par mod, c'est la même règle que dans tout le reste du projet.

## Verdict honnête sur 400 joueurs

250 joueurs sur un gros modpack à 20 TPS avec un p95 sous 50 ms est un objectif raisonnable sur ce
matériel une fois les phases 1, 2, 2B et 5 en place.

400 joueurs sur le même modpack et la même machine est un objectif d'étirement, et la raison tient
de l'arithmétique plus que de l'ambition : à 0,75 cœur-ms par joueur et par tick, le budget par
joueur est déjà plus petit que ce que coûte aujourd'hui une seule base moddée avec quelques
centaines de machines. L'atteindre dépend bien plus de la suppression des coûts quadratiques et de
la baisse du nombre de chunks chargés par joueur que de l'ajout de cœurs ou de threads. Les leviers
qui comptent, dans l'ordre : distance de vue, tracking d'entités, streaming de chunks, throttling
des TileEntities. Le parallélisme multiplie ce qui reste ; il ne peut pas réparer quelque chose qui
croît comme le carré du nombre de joueurs.
