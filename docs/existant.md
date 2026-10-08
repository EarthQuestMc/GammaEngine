# Ce que Crucible fait déjà

Pour chaque élément de la [feuille de route](roadmap.md) : ce qui existe déjà dans le serveur, en
partie ou pas du tout. Les preuves (`chemin:ligne`) sont dans la [carte du code](carte/README.md) ;
cette page n'en garde que la conclusion. État au 8 octobre 2026, établi par lecture du code : aucun
coût n'a encore été mesuré sur un vrai modpack, c'est le rôle de la [phase 0](phase-0.md).

## En bref

* **Rien n'est parallèle dans le tick.** Mondes, entités, TileEntities, ticks de blocs, génération,
  décoration, sauvegarde et traitement des paquets s'exécutent l'un après l'autre sur le thread
  serveur. Hors de ce thread : Netty (dont la compression des paquets de chunks), la lecture des
  chunks par le pool de Forge, l'écriture des fichiers région, la file de la console.
* **Beaucoup de réglages hérités existent déjà** (Spigot, Cauldron, Thermos) : intervalles de tick
  par classe, exclusion des TileEntities qui ne tickent pas, limiteurs de temps, portées
  d'activation et de suivi, sauvegarde des structures désactivable. Plusieurs sont inopérants ou
  réglés de façon neutre : voir les défauts plus bas.
* **Le système de compatibilité automatique n'existe pas du tout** : ni hash par jar, ni analyse de
  bytecode, ni garde-fou de thread (l'`AsyncCatcher` de Spigot n'est pas présent), ni cache.
* **Le démarrage n'a aucun cache** : chaque jar est relu et chaque classe retransformée à chaque
  lancement.

Légende : ✅ présent, 🟡 partiel ou inopérant, ❌ absent.

## Phase 1 — JVM 21 avec ZGC

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Fonctionnement sur Java 9 à 21 | 🟡 | lwjgl3ify intégré : remap des paquets retirés, enums extensibles, `Unsafe` ; refus de Java 17 avant 17.0.6 |
| `--add-opens` dans le lanceur | 🟡 | Liste complète dans `java9args.txt`, à recopier à la main ; rien dans le manifeste du jar |
| Script de lancement | ❌ | Aucun ; `relaunchMain` ne relance pas de processus malgré son nom ; `.gitignore` ignore `*.sh` et `*.bat` |
| ZGC | ❌ | Aucun argument JVM dans le dépôt |
| Transformers pour la réflexion sur les internes et le cast du classloader | 🟡 | Contournement dans `CoreModManager.addUrlToClassloader` pour le moteur ; rien pour le code des mods |
| Bibliothèques retirées du JDK | 🟡 | JAXB (jakarta), Nashorn 15.4, servlet, persistence ; `javax.servlet` est réécrit en `jakarta.servlet` qui n'est pas fourni |
| Code du moteur en Java 21 | ❌ | Build en JDK 8 |

## Phase 2 — Code inutile

Détail élément par élément : [audit du code inutile](audit-code-inutile.md).

| Élément | État actuel |
| --- | --- |
| Snooper | **Actif** par défaut (envoi vers `snoop.minecraft.net` toutes les 15 min) |
| Auto-updater CraftBukkit | Déjà neutralisé par Spigot |
| Vérification de version Forge | Déjà neutralisée par Crucible |
| Anciennes métriques | **Actives** : MCStats (Spigot) et bStats, ce dernier sous l'identifiant 6555 du projet Crucible |
| Interface Swing | Active sur une machine avec écran sans `nogui` |
| Mode démo | Inactif, sans coût |
| Convertisseurs d'anciens formats | Vérifications légères à chaque démarrage |
| Ping legacy | Actif, coût négligeable ; à garder pour les outils de supervision |
| Restes de Cauldron et Thermos | Présents et utilisés (index de chunks, remapping, intervalles de tick) : inventaire, pas de retrait |
| Spawn chunks | Maintien en mémoire déjà coupé par défaut ; 625 chunks chargés au démarrage ; l'overworld retient 128 blocs autour du spawn |
| Sauvegarde des données de structures | Active ; désactivable par `save-structure-info` (Spigot) |
| Profiler vanilla | Déjà inactif |
| Timings v1 | Déjà une façade vers v2 ; v2 désactivé par défaut |
| RCON et Query | Déjà désactivés par défaut |
| Aide Bukkit | Construite à chaque démarrage |
| Ebean | Déjà paresseux |

## Phase 3 — Compatibilité automatique

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Découverte et hash par jar | ❌ | Seul un MD5 des bibliothèques du serveur ; `XxHash64` existe dans le moteur |
| Analyse de bytecode par classe | ❌ | Seul le scan d'annotations de FML (`ASMDataTable`) |
| Classement série ou parallèle | ❌ | — |
| Garde-fou de thread | ❌ | `AsyncCatcher` de Spigot absent |
| Rétrogradation à chaud | ❌ | Mods et moteur partagent un seul classloader : rétrograder ne pourra passer que par un état lu par un transformer, pas par un rechargement de classe |
| Cache des décisions | ❌ | — |
| Rapport au démarrage | 🟡 | Durée de vérification des bibliothèques, bannière ; rien par mod |
| Correction de mods par transformer | 🟡 | Précédents : Streams, Recurrent Complex, Climate Control (KImagine), FastCraft désactivé |

Point d'accroche identifié pour brancher les transformers du moteur avant ceux de FML et des
coremods : `CoreModManager.handleLaunch`, à l'endroit où Crucible appelle déjà lwjgl3ify.

## Phase 4 — Boucle de tick

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Collections sans boxing | 🟡 | fastutil embarqué mais utilisé seulement par Timings ; trove et koloboke sur quelques structures ; boxing restant sur les chunks actifs, les chunks éligibles au spawn, la table des chunks |
| Itération sans suppressions en O(n) | ❌ | `remove(int)` et `removeAll` en O(n) dans les deux boucles |
| TileEntities qui ne tickent pas exclues | ✅ | `CauldronHooks.canUpdate` et liste noire au chargement (`tileentities.yml`) |
| Cache des chunks éligibles au spawn | ❌ | Recalculés à chaque tick |
| Distance de simulation séparée de la distance de vue | ❌ | Tout dépend de `view-distance` |
| Limites par chunk | 🟡 | Compteur d'entités par chunk tenu mais jamais lu ; plafonds de tickets Forge |
| Ralentissement adaptatif | 🟡 | `max-tick-time` à 1000 ms, donc sans effet réel ; rien n'est lié au TPS |
| Index des recettes | ❌ | Parcours linéaire du craft et du four |
| Cache des collisions | ❌ | Nouvelle liste et boucle de blocs à chaque appel |
| Redstone sans mises à jour redondantes | ❌ | Algorithme vanilla, limiteurs grossiers à l'horloge murale |
| Exécuteurs d'événements Bukkit en bytecode | ❌ | `Method.invoke` ; le bus Forge, lui, génère déjà du bytecode |
| Détection des I/O bloquantes | ❌ | Seul le watchdog voit un tick de plus de 30 s |

## Phase 5 — Phases parallèles en lecture seule et pipeline par joueur

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Phases parallèles (cibles, pathfinding, collisions, spawn, tracker) | ❌ | — ; l'entity tracker est en joueurs × entités |
| Compression des paquets de chunks hors du thread principal | 🟡 | Faite sur Netty, mais une fois par joueur : pas de cache partagé ; l'extraction des sections reste sur le thread principal |
| Visibilité des entités par joueur | 🟡 | Vanilla et portées de suivi Spigot, sur le thread principal |
| Sauvegarde des données joueur hors thread | ❌ | Synchrone à l'autosave et à la déconnexion |

## Phase 6 — Chargement des chunks et prégénération

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Pool d'I/O dimensionné selon le disque | 🟡 | Pool Forge : 1 thread + 1 par tranche de 50 joueurs ; élargi au démarrage par le préchargement GammaEngine |
| File à priorités | ❌ | FIFO |
| Annulation des demandes inutiles | 🟡 | Seulement pour une tâche pas encore commencée |
| Budget de finalisation par tick | ❌ | Toute la file finie est vidée en un tick |
| Entités et TileEntities créées en parallèle | ❌ | Sur le thread principal |
| Génération hors thread | ❌ | Génération et décoration synchrones |
| Décoration en série avec budget | ❌ | — |
| Journalisation des chargements synchrones | 🟡 | `cauldron.logging.chunkLoad` : pile complète à chaque appel, sans compteur ni agrégation |
| Délai avant déchargement, cache des chunks récents | 🟡 | Réglages présents (`chunkGCGracePeriod`, cache dormant Forge) mais à zéro |
| Limite d'envoi par joueur | 🟡 | 5 chunks par tick (`max-bulk-chunks`), sans contre-pression |
| Distance de vue adaptative | 🟡 | Changement à chaud possible, jamais piloté |
| Pas de spawn chunks permanents | 🟡 | Voir phase 2 |
| Prégénération avec bordure de monde | ❌ | — |

## Phase 7 — Lumière, sauvegarde, réseau

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Sauvegarde hors thread par snapshot | 🟡 | Seuls la compression et l'écriture sont sur le thread d'E/S ; NBT construit sur le thread principal en partageant les tableaux de blocs vivants ; l'autosave attend la fin de toutes les écritures |
| Lumière asynchrone | 🟡 | Code de Paper présent (`use-async-lighting`) mais inopérant : sa garde est inversée |
| Réseau hors du thread principal | 🟡 | E/S, encodage, compression et cinq types de paquets prioritaires sur Netty ; tout le jeu traité par `networkTick` sur le thread principal |
| Logs hors du thread principal | ❌ | Log4j2 entièrement synchrone ; seule la console passe par une file |

## Phase 8 — Édition en masse

❌ Rien n'existe : chaque bloc posé passe par le chemin normal, avec un marquage de mise à jour par
bloc dont le dédoublonnage est quadratique.

## Phase 9 — Démarrage rapide

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Cache des classes transformées | ❌ | Seulement des caches mémoire du `LaunchClassLoader` |
| Cache du scan d'annotations | ❌ | Chaque jar relu à chaque démarrage |
| Cache du remapping des plugins | 🟡 | Tables en mémoire, rien sur disque ; remapping paresseux, classe par classe |
| AppCDS, CRaC | ❌ | — |
| Vérification des bibliothèques en parallèle | ✅ | Ajout GammaEngine (244 ms à 92 ms mesurés) ; le MD5 de 103 Mo reste refait à chaque démarrage |
| Préchargement parallèle du spawn | ✅ | Ajout GammaEngine, pour les chunks déjà générés |

## Phases 10 à 12

| Élément | État | Ce qui existe |
| --- | --- | --- |
| Un thread par dimension | ❌ | Boucle séquentielle sur les mondes ; caches statiques partagés sans verrou |
| Tick hybride par régions | ❌ | — |
| Proxy | 🟡 | Forwarding BungeeCord ; ni Velocity ni PROXY protocol |
| Stockage des chunks en Rust | 🟡 | Bibliothèque native (zlib, XXH64, arithmétique des secteurs) chargée mais pas encore appelée |
| Sharding | ❌ | — |

## Défauts hérités repérés en route

Ils ne sont pas dans la feuille de route mais conditionnent plusieurs phases. Aucun n'a été corrigé.

| Défaut | Gravité | Où | Phase concernée |
| --- | --- | --- | --- |
| `HashedArrayList.listIterator()` s'appelle lui-même (récursion infinie) et `set()` retire le mauvais élément ; c'est le type réel de `loadedEntityList` et `loadedTileEntityList` | Haute : un mod qui appelle `listIterator()` fait tomber le serveur | [tick.md](carte/tick.md), piège 3 | 4, à corriger en premier |
| Une exception dans une entité ou une TileEntity de mod arrête le serveur (`removeErroringEntities` à faux) | Haute | [tick.md](carte/tick.md), piège 5 | 3 et 4 |
| L'éviction du cache de régions peut fermer une région pendant une écriture : chunk perdu possible (déduit, à confirmer par test) | Haute si confirmé | [chunks.md](carte/chunks.md), piège 1 | 6 et 7 |
| L'ordre des générateurs de monde des mods dépend d'un `HashSet` : génération non reproductible | Moyenne | [chunks.md](carte/chunks.md), piège 4 | 0 (déterminisme) et 6 |
| L'entrée en jeu d'un joueur moddé se termine sur un thread Netty, hors du thread principal (déduit, à confirmer) | Moyenne | [reseau.md](carte/reseau.md), piège 10 | 3 et 7 |
| L'activation range de Spigot est quasi inopérante | Moyenne (coût) | [tick.md](carte/tick.md), piège 1 | 4 |
| Télémétrie active sans consentement, dont bStats sous l'identifiant de Crucible | Moyenne (vie privée, marque) | [audit-code-inutile.md](audit-code-inutile.md) | 2 |
| `netty-threads` probablement sans effet, `ping-packet-limit` lu par personne, `crucible.chunkCacheSize` ne règle pas le plafond réel | Faible | [reseau.md](carte/reseau.md), [chunks.md](carte/chunks.md) | 7 |
