# Feuille de route GammaEngine

Ce document est à la fois le plan de travail et le suivi d'avancement. Il dit ce que fait chaque
phase, comment elle se déroule, à quoi on reconnaît qu'elle est terminée, et où en est le projet.

## Où on en est

| Phase | Sujet | État |
| --- | --- | --- |
| 0 | Build, référence, métriques | **Terminée** |
| 1 | Optimiser les chemins existants sans toucher au threading | **En cours** |
| 2 | Snapshots de chunks, chargement et sauvegarde asynchrones | À faire |
| 2B | Passage à l'échelle joueurs : tracking, streaming de chunks, sauvegarde étalée | À faire |
| 3 | Régions : propriété et contexte, traçage seul | À faire |
| 4 | Suivi des accès, détection de conflits, graphe de dépendances | À faire |
| 5 | Ordonnanceur de régions, premiers ticks parallèles | À faire |
| 6 | Ordonnanceur d'entités, transactions inter-régions | À faire |
| 6B | Plugins, commandes et API en multithread | À faire |
| 7 | Instrumentation bytecode des mods et plugins | À faire |
| 8 | Auto-quarantaine et apprentissage | À faire |
| 9 | Bibliothèque native Rust | **Priorité 1 faite**, priorité 2 en attente |
| 10 | Pathfinding, lighting, collisions, réseaux moddés | À faire |
| 11 | Dégradation adaptative et comptabilité des coûts | À faire |

La phase 9 a été commencée hors ordre parce que la compression et le hachage qu'elle contient ne
dépendent pas du modèle de threading, et parce que le banc de mesure dont elle a besoin existait
déjà depuis la phase 0.

Les phases 2B et 11 ont été ajoutées après l'analyse de [scaling.md](scaling.md), qui montre que
les premiers murs à 400 joueurs ne sont pas dans la boucle de tick : le tracking d'entités et le
streaming de chunks croissent plus vite que le nombre de joueurs, et la sauvegarde automatique
écrit tous les chunks modifiés sur le thread serveur. Ce sont des problèmes algorithmiques, ils ne
demandent pas le modèle de régions, et ils doivent être réglés avant que le parallélisme puisse
montrer sa vraie valeur.

## Comment ce plan a été construit

Trois contraintes ont fixé l'ordre.

**Mesurer avant de changer.** Un serveur multithreadé est une machine à produire des bugs
impossibles à reproduire. Sans référence et sans métriques par sous-système, chaque modification
ultérieure n'est qu'une opinion. La phase 0 construit donc les instruments, et chaque phase
suivante doit montrer ses chiffres.

**L'infrastructure de correction avant le parallélisme.** La propriété des objets, le suivi des
accès et la détection de conflits ne servent à rien seuls, mais les faire tourner un moment sur un
serveur monothread est la seule façon d'apprendre ce que les mods touchent vraiment, avec un risque
nul. Les phases 3 et 4 arrivent donc *avant* toute exécution parallèle, et elles arrivent activées.
Quand la phase 5 tickera enfin deux régions en même temps, le détecteur qui attraperait l'erreur
sera déjà en production et déjà éprouvé.

**Le travail le moins cher et le plus réversible d'abord.** Les phases 1, 2 et 2B apportent des
gains réels sans toucher au modèle de threading, donc elles se valident avec des tests ordinaires et
s'annulent indépendamment. Le travail risqué arrive ensuite, sur une base déjà plus rapide et déjà
instrumentée.

Dans chaque phase la méthode est la même : lire le code, écrire l'invariant qui doit tenir, écrire
ou adapter le test, implémenter, compiler, tester, mesurer contre la référence, garder ou annuler.
Une phase n'est pas finie tant que le serveur ne démarre pas, ne ticke pas et ne s'arrête pas
proprement avec la modification en place.

---

## Phase 0 — Build, référence, métriques

**Terminée.**

Objectif : savoir compiler le serveur non modifié, le lancer et le mesurer, avant de changer le
moindre comportement.

Déroulé :

1. Identification des versions exactes : Minecraft 1.7.10, MCP 9.08, Forge 10.13.4.1614, Bukkit
   1.7.10-R0.1-SNAPSHOT, Gradle 8.0, JDK 8 pour le build avec une toolchain Java 17 pour `buildSrc`.
2. Création du workspace patché avec `setupCrucible` et compilation du serveur non modifié, pour
   disposer d'un artefact de référence.
3. Cartographie des chemins de tick que les phases suivantes vont reprendre, consignée dans
   [architecture.md](architecture.md) : boucle serveur, tick serveur, tick par monde, tick des
   entités, tick des TileEntities, bus d'événements Forge, scheduler Bukkit, fournisseur de chunks,
   chargeur de chunks, fichier region.
4. Construction de la couche de mesure : histogrammes de latence avec p50/p95/p99, registre de
   métriques, TPS glissant et percentiles MSPT exacts, sessions de profilage qui écrivent un
   rapport sur disque.
5. Branchement du runtime dans `MinecraftServer` en quatre points seulement : démarrage, serveur
   démarré, début et fin de tick, arrêt.
6. Démarrage d'un vrai serveur et enregistrement de la référence.

Référence, monde vide, sans mods, machine de développement :

| Métrique | Valeur |
| --- | --- |
| TPS | 19,94 |
| MSPT moyen | 0,22 ms |
| MSPT p99 | 0,62 ms |
| Démarrage, création du monde | 4,0 s |
| Démarrage, monde existant | 1,1 s |

Critères de sortie, tous atteints : build reproductible documenté, serveur qui démarre et s'arrête
proprement, `/autothread` qui affiche des chiffres réels, tests unitaires au vert.

---

## Phase 1 — Optimiser les chemins existants

**En cours.** Le modèle de threading n'est pas touché dans cette phase : un seul thread de
simulation, le même ordre des opérations, les mêmes résultats.

Objectif : rendre le serveur monothread mesurablement plus rapide, pour que le travail parallèle
démarre d'une base propre et pour qu'on puisse distinguer plus tard les gains du parallélisme de
ceux de l'optimisation ordinaire.

Déroulé :

1. Profiler un serveur chargé et classer les chemins chauds par coût total, avec le rapport de la
   phase 0.
2. Les entrées-sorties de chunks d'abord : aujourd'hui `AnvilChunkLoader.saveChunk` sérialise et
   compresse sur le thread appelant, et `RegionFile` se synchronise autour de chaque lecture et
   écriture. La compression part sur le pool de workers, avec la bibliothèque native si elle est
   présente.
3. La pression d'allocation dans les boucles de tick : listes et maps temporaires dans les boucles
   d'entités et de TileEntities, coordonnées boxées, objets NBT créés à chaque tick. On ne remplace
   que ce que le profileur montre, avec les collections primitives déjà livrées avec le serveur
   (fastutil, koloboke).
4. Le choix des collections sur les structures chaudes : les listes d'entités et de TileEntities
   sont parcourues et modifiées à chaque tick.
5. Le réseau : sortir la compression du tick, réutiliser les buffers là où c'est prouvé sûr, ne
   jamais changer le protocole.

Méthode pour chaque point : mesurer avant, modifier, mesurer après, ne garder que si le gain est
réel et le comportement identique.

Critères de sortie : un tableau avant/après documenté par optimisation, aucun changement de
comportement, tests au vert, un monde identique octet pour octet après le même scénario.

---

## Phase 2 — Snapshots de chunks, chargement et sauvegarde asynchrones

Objectif : sortir le travail disque, la compression et le NBT du thread de simulation sans jamais
sérialiser un chunk qu'un autre thread peut modifier.

Déroulé prévu :

1. `ChunkSaveSnapshot` : une copie immuable et compacte de tout ce dont une sauvegarde de chunk a
   besoin, prise sur le thread propriétaire dans la fenêtre la plus courte possible.
2. `AsyncChunkSaver` : snapshot pendant le tick, puis sérialisation, compression et écriture sur le
   pool d'entrées-sorties.
3. `AsyncChunkLoader` : lecture, décompression et décodage NBT hors du thread ; seul le commit final
   dans le monde se fait sur le thread propriétaire.
4. Garder le `ChunkIOExecutor` existant de Forge fonctionnel du début à la fin : il est remplacé,
   pas contourné.

Critères de sortie : les mondes sauvegardés par le nouveau chemin sont lisibles par le client
vanilla et par Crucible amont ; un crash pendant une grosse sauvegarde laisse un monde chargeable ;
réduction mesurée du pic de sauvegarde dans le p99 du MSPT.

---

## Phase 2B — Passage à l'échelle joueurs

Objectif : supprimer les coûts qui croissent plus vite que le nombre de joueurs. Rien ici ne demande
le modèle de régions, et tout est rentable dès le serveur monothread. L'analyse complète, avec les
mesures derrière chaque point, est dans [scaling.md](scaling.md).

Déroulé prévu :

1. **Tracking d'entités indexé par chunk.** Aujourd'hui chaque entité suivie parcourt toute la liste
   des joueurs et chaque joueur qui bouge parcourt tout l'ensemble des entités suivies, soit 9
   millions d'itérations par tick à 400 joueurs et 30 000 entités. En indexant les entités par chunk
   et en ne considérant que les chunks à portée de tracking, on divise le travail par environ 75, et
   cela se parallélise proprement plus tard.
2. **Cache partagé de charge utile de chunk.** Les octets extraits et compressés d'un chunk ne
   dépendent que du chunk. On les met en cache, on les invalide à la modification, et deux cents
   joueurs au spawn paient une fois au lieu de deux cents fois.
3. **Niveau et placement de la compression.** Les paquets de chunks compressent au niveau 4 ; le
   niveau 1 mesure 70 % plus rapide pour 3,5 % d'octets en plus sur des données de chunk. On règle
   le niveau et on déplace le travail sur le pool de workers avec le compresseur natif.
4. **Budget d'envoi par joueur et contre-pression.** Un client congestionné retarde son propre flux
   de chunks, jamais le tick.
5. **Sauvegarde continue étalée.** Remplacer le pic de sauvegarde automatique, aujourd'hui tous les
   chunks modifiés sérialisés sur le thread serveur, par une file de chunks sales vidée avec un
   budget fixe par tick. La sauvegarde des données joueur part aussi hors du thread.

Critères de sortie : à nombre de joueurs synthétiques fixé, le coût de tick par joueur cesse de
croître avec le nombre de joueurs ; le pic de sauvegarde disparaît du p99 ; les mondes restent
identiques octet pour octet après le même scénario.

---

## Phase 3 — Régions : propriété et contexte

Objectif : donner à chaque chunk, entité et TileEntity un propriétaire logique unique, et rendre ce
propriétaire connaissable depuis n'importe quel thread. Rien ne s'exécute en parallèle dans cette
phase.

Déroulé prévu :

1. `Region`, `RegionOwnership`, `RegionManager` : regrouper les chunks chargés en régions qui
   n'interagissent pas, avec fusion et séparation au fil des changements du monde.
2. `RegionContext` : la réponse à « quelle région ai-je le droit d'écrire », disponible pour tout
   code qui la demande, assez peu chère pour être consultée sur les chemins chauds.
3. Les régions sont créées et entretenues, la propriété est suivie, et le serveur ticke toujours
   tout sur un seul thread. Le seul effet visible est `/autothread regions`.

Critères de sortie : la propriété est correcte et stable sur une journée de jeu réel, les fusions et
séparations de régions ne perdent aucun objet, aucun coût mesurable ajouté au tick.

---

## Phase 4 — Suivi des accès, conflits, graphe de dépendances

Objectif : apprendre ce que les mods chargés touchent réellement, tant qu'il est encore impossible
de corrompre quoi que ce soit.

Déroulé prévu :

1. `AccessTracker` : enregistre les accès par type (lecture et écriture locales à la région,
   globales, inter-régions, calcul asynchrone), attribués au mod ou au plugin appelant.
2. `ConflictDetector` : à partir des accès enregistrés, signale ce qui *aurait* été en conflit si les
   régions avaient été tickées en parallèle. Dans cette phase, il ne fait que signaler.
3. `DependencyGraph` : construit les relations réelles entre chunks, TileEntities, inventaires et
   réseaux moddés, et se met à jour avec le monde.
4. Rapport agrégé, parce qu'un serveur chargé produit des centaines de milliers d'événements
   identiques.

Critères de sortie : un serveur moddé tourne des heures avec le suivi activé, le rapport de conflits
est stable et explicable, et le surcoût est assez faible pour laisser le suivi actif.

---

## Phase 5 — Ordonnanceur de régions, premiers ticks parallèles

Objectif : ticker en même temps un petit nombre de régions dont l'indépendance est prouvée.

Déroulé prévu :

1. `RegionScheduler` sur le pool de tick de régions, en commençant par deux régions et un repli
   strict vers la sérialisation.
2. Le détecteur de la phase 4 reste actif, cette fois comme garde-fou : un conflit fait baisser le
   parallélisme immédiatement.
3. Élargir progressivement, scénario par scénario, jamais plus vite que les preuves.

Critères de sortie : des exécutions répétées du même scénario produisent des mondes identiques,
aucune duplication, aucune mise à jour perdue, et un gain réel de MSPT sous charge.

---

## Phase 6 — Ordonnanceur d'entités, transactions inter-régions

Objectif : traiter tout ce qui franchit légitimement une frontière de région.

Déroulé prévu :

1. `EntityScheduler` : migration atomique d'une entité d'une région à une autre, jamais tickée deux
   fois, jamais perdue, pour les joueurs, les mobs, les items, les projectiles, les véhicules et les
   entités moddées.
2. `CrossRegionCoordinator` et `MultiRegionTransaction` : déterminer les régions concernées, les
   verrouiller par identifiant croissant pour rendre tout cycle impossible, exécuter, libérer, avec
   timeout et métriques.
3. Couvrir les cas réels : pipes et transfert d'items, réseaux d'énergie et de fluides,
   téléportations, explosions, multiblocs.

Critères de sortie : aucun deadlock sous stress, aucune duplication d'item à travers une frontière,
transactions visibles et mesurées dans `/autothread conflicts`.

---

## Phase 6B — Plugins, commandes et API en multithread

Objectif : que l'exécution des plugins cesse d'être un point de sérialisation global, sans qu'aucun
plugin existant n'ait à le savoir.

Ce qui se passe aujourd'hui : le scheduler Bukkit exécute toutes les tâches synchrones sur le thread
serveur, chaque commande de plugin s'exécute sur le thread serveur, et chaque événement Bukkit est
distribué sur le thread serveur. Un seul plugin lent bloque tout le monde.

Déroulé prévu :

1. **Commandes.** Une commande n'est presque jamais globale : elle agit sur un joueur, un bloc, un
   inventaire, donc sur une région. Le runtime détermine la région visée à partir de l'émetteur et
   des arguments, exécute la commande dans le contexte de cette région, et retombe sur une exécution
   globale quand la cible est ambiguë ou globale. Une commande qui se met à toucher plusieurs
   régions déclenche une transaction, pas une exception.
2. **Scheduler Bukkit.** `runTask` continue de signifier « exécute-moi là où c'est sûr ». Le runtime
   choisit ensuite : région, global, asynchrone ou sérialisé, selon ce que la tâche a touché les
   fois précédentes. `runTaskAsynchronously` garde sa sémantique actuelle.
3. **Événements.** Un événement déclenché dans une région est distribué dans cette région. Les
   listeners sont observés individuellement : celui qui ne touche que la région active devient
   parallélisable, celui qui écrit un état global est sérialisé, et cette décision est prise par
   listener, pas par plugin.
4. **API interne** pour le code neuf : `GlobalScheduler`, `RegionScheduler`, `EntityScheduler`,
   `AsyncScheduler`. Proposée, jamais nécessaire pour faire tourner un plugin classique.

La même mécanique s'applique aux mods : un handler d'événement Forge est observé exactement comme un
listener Bukkit, et un mod dont une partie est parallélisable et une autre non voit seulement la
partie fautive sérialisée.

Critères de sortie : un lot de plugins courants tourne sans modification, deux commandes visant deux
régions différentes s'exécutent réellement en même temps, et aucun plugin ne reçoit d'exception liée
au threading.

---

## Phase 7 — Instrumentation des mods et plugins

Objectif : étendre le suivi et la protection au code qu'on ne possède pas, sans rien demander aux
auteurs.

Déroulé prévu :

1. Un transformer ASM, chargé par le chemin coremod LaunchWrapper/Forge existant, ciblant uniquement
   les points d'accès sensibles : modification du monde et des chunks, collections d'entités et de
   TileEntities, inventaires, bus d'événements Forge, scheduler Bukkit, singletons globaux connus.
2. Instrumenter étroitement. Chaque site instrumenté doit justifier son coût.
3. Augmenter le parallélisme à mesure que les preuves instrumentées s'accumulent.

Critères de sortie : un gros modpack démarre avec l'instrumentation active, le coût au démarrage est
acceptable, et les données collectées sont plus riches que ce que la phase 4 pouvait voir.

---

## Phase 8 — Auto-quarantaine et apprentissage

Objectif : rendre le runtime capable de s'améliorer et de se protéger sans administrateur dans la
boucle.

Déroulé prévu :

1. `AutoQuarantineManager` : sur conflits répétés, exceptions de threading, blocages ou
   non-déterminisme détecté, baisser le parallélisme de la plus petite unité responsable : un objet,
   un type de TileEntity, une méthode, un listener, une zone géographique. Jamais un mod entier.
2. Apprentissage : persister le comportement observé par classe et par méthode, invalidé quand le
   hash du jar du mod, la version du serveur ou l'instrumentation changent.
3. Retest périodique, pour qu'un composant mis en quarantaine par une mauvaise interaction puisse
   regagner son parallélisme.

Critères de sortie : le profil survit à un redémarrage, un mod de test délibérément cassé est mis en
quarantaine à la bonne granularité, et le reste de ce mod continue de tourner en parallèle.

---

## Phase 9 — Bibliothèque native Rust

**Priorité 1 faite.** `native/` produit une bibliothèque dynamique Rust, le serveur la charge quand
elle est présente et retombe sur Java quand elle est absente, la refuse en cas d'ABI incompatible,
et attrape les panics à la frontière.

Implémenté et mesuré sur 200 ko de données en forme de chunk, JNI compris :

| Opération | Java | Rust | Écart |
| --- | --- | --- | --- |
| Compression zlib | 195 Mio/s | 265 Mio/s | +36 % |
| Décompression zlib | 1010 Mio/s | 1544 Mio/s | +53 % |
| XXH64 | 4296 Mio/s | 7509 Mio/s | +75 % |
| Taille compressée | 19 268 o | 18 806 o | −2,4 % |

L'arithmétique des secteurs de fichiers region et la validation de la table de localisation sont
implémentées en fonctions pures testées, prêtes pour l'écrivain de la phase 2.

Priorité 2, une fois l'architecture Java stable : encodage et décodage binaire NBT, pathfinding sur
snapshots immuables, collisions et recherches spatiales par lots. Chacun doit battre Java sur le
banc existant, sinon il ne part pas. Détails dans [native-engine.md](native-engine.md).

---

## Phase 10 — Chemins lourds

Objectif : sortir du thread de région le travail coûteux restant, une fois que l'architecture peut
le supporter.

Prévu : lighting asynchrone (snapshot, calcul sur worker, application par le propriétaire),
pathfinding sur snapshots, collisions par lots, et adaptateurs dédiés aux gros réseaux moddés (AE2,
BuildCraft, IC2, Thermal Expansion, Ender IO, Mekanism, GregTech, Railcraft, ComputerCraft,
OpenComputers). Le mécanisme générique doit fonctionner sans ces adaptateurs ; les adaptateurs ne
font qu'accélérer les cas connus.

---

## Phase 11 — Dégradation adaptative et comptabilité des coûts

Objectif : tenir 20 TPS en dépensant de la qualité plutôt que du temps de tick, automatiquement, et
savoir exactement qui coûte quoi.

Déroulé prévu :

1. **Distance de vue et de simulation adaptatives**, pilotées par le budget de tick mesuré. Le nombre
   de chunks chargés par joueur croît comme le carré de la distance de vue, donc la baisser de deux
   crans dans une zone bondée retire plus de 40 % de tous les coûts par chunk d'un coup. Appliqué
   localement, annulé automatiquement.
2. **Caps de mobs, throttling des TileEntities et fusion d'items adaptatifs**, sur les classes dont
   la couche d'apprentissage de la phase 8 a prouvé qu'elles le tolèrent.
3. **Fréquence de tracking des entités** réduite pour les entités lointaines avant toute autre
   dégradation.
4. **Comptabilité des coûts par joueur, par mod et par plugin**, parce qu'à 400 joueurs une réponse
   de support a besoin d'un nom, pas d'une moyenne.

L'ordre compte : la qualité est sacrifiée dans l'ordre ci-dessus, et le TPS est la dernière chose à
céder. Aucun administrateur ne configure quoi que ce soit par mod.

Critères de sortie : sous une surcharge délibérée, le serveur baisse la qualité, garde 20 TPS, et
restaure les réglages précédents tout seul une fois la charge passée.

---

## Travaux transverses, présents dans toutes les phases

**Bancs d'essai.** Un harnais reproductible et les huit scénarios : joueurs groupés au spawn,
joueurs dispersés, grosses bases industrielles, exploration et génération de chunks, beaucoup de
mobs, beaucoup de TileEntities, gros réseaux, grosses modifications de monde. De 50 à 500 joueurs.
Cible : 250 joueurs sur un gros modpack à 20 TPS avec un p95 MSPT à 50 ms ou moins, sur la machine
de référence AMD EPYC 4344P.

**Intégrité des sauvegardes et déterminisme.** Un monde doit rester lisible après un crash
contrôlé, un arrêt sous charge, des sauvegardes répétées, des migrations de régions, des
transactions inter-régions et un déchargement-rechargement de chunk. Des exécutions répétées d'un
même scénario doivent produire le même monde, vérifié en hachant le contenu décompressé des chunks.

**Mémoire et GC.** Objectif explicite : que le serveur consomme le moins de RAM possible à charge
égale. Ce n'est pas une option de configuration, c'est un travail d'ingénierie sur sept fronts,
chacun mesuré avant et après :

1. **Ne pas garder de tampon surdimensionné.** Un paquet de chunk conserve aujourd'hui le tableau de
   sortie de la compression à la taille de l'entrée, soit 164 ko retenus pour environ 20 ko utiles,
   pour chaque paquet en attente d'envoi. Premier correctif livré en phase 1.
2. **Allouer moins sur les chemins chauds.** Le streaming de chunks est la plus grosse source de
   déchets d'un serveur chargé, loin devant les boucles de tick. Moins d'allocations veut dire moins
   de pauses GC, donc un meilleur p99, pas seulement un tas plus petit.
3. **Structures compactes.** Collections primitives sur les structures chaudes, pas de coordonnées
   boxées, pas de `Integer`/`Long` en clés de map.
4. **Moins de chunks chargés.** C'est le levier dominant : la mémoire suit le nombre de chunks, qui
   suit le carré de la distance de vue. La distance adaptative de la phase 11 est aussi une
   optimisation mémoire.
5. **Déchargement plus agressif mais sûr** des chunks sans joueur, et politique de despawn et de
   fusion des entités item sous pression.
6. **Budget mémoire pour les caches du moteur**, respecté par éviction, jamais dépassé en silence.
7. **Configuration GC documentée par taille de tas**, sans aucun flag expérimental obligatoire.

**Catalogue d'optimisations monothread.** Redstone, hoppers, lighting, collisions, entités item,
spawn de mobs, allocations NBT. Indépendant du modèle de threading, mesuré un par un, listé dans
[folia.md](folia.md).

**Intégration continue.** Build Java, build Rust, tests unitaires, tests d'intégration, test de
démarrage du serveur et banc d'essai de fumée, sur Linux x86_64 en priorité puis Windows x86_64.

**Banc de compatibilité.** IC2, BuildCraft, AE2, Thermal Expansion, Thaumcraft, Ender IO, Mekanism,
GregTech, Galacticraft, Twilight Forest, Railcraft, MineFactory Reloaded, ComputerCraft,
OpenComputers, plus WorldEdit, WorldGuard, Vault, PermissionsEx, un équivalent Essentials et un
ProtocolLib compatible 1.7.10.
