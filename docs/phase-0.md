# Phase 0 — Banc de test : plan détaillé

## But

Mesurer le serveur sous une charge de joueurs réaliste et reproductible, pour que chaque phase
suivante montre ses chiffres avant et après. Rien dans cette phase ne change le comportement du
serveur : tout ajout côté serveur est de l'observation, coupé par défaut ou de coût négligeable.

Le banc doit répondre, pour un scénario donné, à quatre questions :

1. Combien coûte un tick : MSPT moyen, p50, p95, p99, maximum, TPS.
2. Combien coûte un joueur : MSPT divisé par le nombre de joueurs, l'indicateur principal du projet.
3. Qui coûte : temps de tick par mod et par plugin, par chunk, par phase du tick.
4. Ce que paie la JVM : pauses GC, allocation, tas, hors-tas.

## Ce qui existe déjà

Détail et preuves dans la [carte de la mesure](carte/mesure.md). En résumé :

* `TickStatistics` et `LatencyHistogram` (`io.github.gammaengine.profiler`) : TPS glissant et
  percentiles MSPT exacts sur la dernière minute, testés.
* `/autothread profile start|stop|report` : sessions de profilage, rapport texte dans
  `gammaengine/reports/`.
* `/autothread bench` (`WorldBenchmark`) : charge synthétique dans le monde (chunks maintenus
  chargés, entités avec IA, TileEntities qui tickent, graine fixe), avec temps CPU, GC et tas. Il ne
  simule aucun joueur.
* `/autothread memory` : tas, hors-tas, threads, contenu chargé par monde.
* Timings v2 (`co.aikar.timings`) branché dans le tick par Crucible, désactivé par défaut.
* `tickTimeArray` et `worldTickTimes` de Minecraft, `/tps` de Spigot (dont le TPS courant ne
  mesure qu'un seul tick sur cent).

Ce qui manque : des joueurs simulés, des scénarios rejouables, l'attribution par mod et par chunk,
l'export lisible par une machine, et la comparaison de deux exécutions.

## Les cinq livrables

### 1. Les bots

Un client minimal au protocole 1.7.10 (version 5), capable de passer le handshake Forge, de rester
connecté et de se déplacer. Il ne dessine rien, ne garde pas le monde en mémoire et saute les paquets
dont il n'a pas besoin, pour qu'une seule machine porte 500 bots sans fausser la mesure.

La séquence exacte des paquets, les obstacles qui expulsent un bot et les réglages du serveur de
banc sont dans le [cahier des charges des bots](phase-0-bots.md).

Comportements :

| Comportement | Ce que fait le bot | Ce qu'il charge côté serveur |
| --- | --- | --- |
| Immobile | Reste à sa position, répond aux keep-alive | Coût fixe par joueur, tracking |
| Errance | Marche au hasard dans un rayon donné | Mouvement, collisions, tracking |
| Exploration | Avance en ligne droite à vitesse de sprint ou en vol | Chargement, génération et envoi de chunks |
| Dispersion | Téléporté par `spreadplayers` sur un grand rayon | Nombre de chunks chargés, tick par chunk |
| Commandes | Envoie des commandes ou du chat à un rythme donné | Commandes, événements, plugins |

Mesures côté bot : temps pour rejoindre, aller-retour des keep-alive, chunks reçus par seconde,
octets reçus, expulsions avec leur raison.

**Langage : décision à prendre.** Recommandation : Rust, avec une crate `protocol` partagée.
Raisons : 500 connexions sur une machine pour un coût CPU minime, et le même code (trames,
VarInt, paquets 1.7.10, handshake Forge) servira au proxy de la phase 12, qui doit relayer le
handshake Forge. Le cahier des charges réserve d'ailleurs Rust aux outils hors ligne. Alternatives :
Java 21 avec threads virtuels (même langage que le moteur), ou `node-minecraft-protocol` avec
`minecraft-protocol-forge` (existe déjà, gère le handshake `FML|HS` de 1.7.10, mais plus lourd par
bot et ajoute Node au projet).

### 2. Les scénarios

Un fichier par scénario, versionné dans `bench/scenarios/` : monde de départ, réglages du serveur,
nombre de bots et rythme d'arrivée, comportement, durée de chauffe, durée de mesure, nombre de
répétitions, niveau de mesure.

| Scénario | Charge | Phase qui en dépend |
| --- | --- | --- |
| `spawn-groupe` | N bots au spawn, immobiles ou en errance courte | 5 (tracking, paquets de chunks) |
| `disperses` | N bots dispersés sur un grand rayon | 4 et 6 (tick par chunk, chunks chargés) |
| `exploration` | N bots en ligne droite en terrain neuf | 6 (chargement, génération, envoi) |
| `base-industrielle` | Bots près des bases du monde d'EarthQuest | 3 et 4 (TileEntities moddées) |
| `mobs` | Zone peuplée de mobs, bots en survie | 5 (IA, pathfinding, collisions) |
| `tile-entities` | Rangées de hoppers et de machines | 4 (boucle des TileEntities) |
| `edition-en-masse` | Un bot opérateur lance une grosse édition WorldEdit | 8 |
| `connexions` | 100 bots qui rejoignent en dix secondes | 7 (réseau, handshake) |

Chaque scénario se joue à N = 0, 50, 100, 200 et 400 bots, pour tracer le coût par joueur en
fonction du nombre de joueurs : c'est cette courbe qui montre un coût qui croît plus vite que le
nombre de joueurs.

### 3. La sonde serveur et l'export

Trois niveaux, pour que la mesure ne fausse pas ce qu'elle mesure :

| Niveau | Contenu | Coût | Activation |
| --- | --- | --- | --- |
| 1, toujours actif | Durée de chaque tick et de ses grandes phases, joueurs, chunks, entités, TileEntities chargés, pauses GC (notifications des MXBeans), tas | Une dizaine de `nanoTime` par tick | Toujours, export coupé par défaut |
| 2, attribution | Temps par classe d'entité et de TileEntity, ramené au mod ou au plugin propriétaire ; temps par chunk ; temps par handler d'événement et par tâche planifiée | Deux `nanoTime` par objet tické ; ordre de grandeur attendu de quelques dizaines de ns, non mesuré : c'est la première mesure à faire | Interrupteur, pendant le banc seulement |
| 3, profil | Enregistrement JFR du scénario, attribué aux mods après coup grâce à la table classe → jar écrite au démarrage | Environ 1 % | Option du scénario |

Le propriétaire d'une classe se retrouve par son jar d'origine (`CodeSource` de la classe, comparé à
`ModContainer.getSource()` pour les mods et au jar du `PluginClassLoader` pour les plugins) ; les
classes de Minecraft, Forge et Crucible sont regroupées sous leur nom. Les sites de sonde exacts sont
dans la [carte de la mesure](carte/mesure.md). Ce qui se réutilise au lieu d'être réécrit :

* Timings v2 attribue déjà exactement les événements et les tâches Bukkit à leur plugin ; il ne
  sert qu'activé, et il exporte vers un site externe, donc le banc lit ses compteurs sans passer
  par son export.
* Forge connaît le mod d'une entité enregistrée (`EntityRegistry.lookupModSpawn`) et celui de chaque
  générateur de monde (`GameRegistry.worldGenMap`).
* Le mod propriétaire d'un handler d'événement Forge est stocké dans un champ privé
  d'`ASMEventHandler` : il faut un accesseur, donc un patch d'une ligne.
* `World.entitiesTicked` et `tilesTicked` comptent déjà les objets réellement tickés par monde.

À retenir pour l'interprétation : le `/tps` de Spigot ne mesure qu'un tick sur cent, et Timings v2
crée un identifiant à chaque création d'entité ou de TileEntity même désactivé. La référence du banc
est le MSPT exact de `TickStatistics`.

Export, dans `gammaengine/bench/<exécution>/` :

| Fichier | Contenu |
| --- | --- |
| `ticks.csv` | Une ligne par tick : durée, phases, joueurs, chunks, entités, TileEntities |
| `gc.csv` | Une ligne par pause : collecteur, durée, tas avant et après |
| `mods.csv` | Niveau 2 : temps cumulé par mod ou plugin et par type d'objet |
| `chunks.csv` | Niveau 2 : les chunks les plus coûteux avec leur contenu |
| `summary.json` | Résumé : percentiles, coût par joueur, versions, arguments JVM, commit |

Interrupteurs dans `gammaengine.yml` : `bench.export` (faux par défaut) et `bench.attribution` (faux
par défaut). Pilotage par la console du serveur, sans ouvrir de port : l'orchestrateur écrit les
commandes sur l'entrée standard du processus.

### 4. L'orchestrateur et la comparaison

Une commande lance un scénario de bout en bout : copie du monde de départ dans un dossier neuf,
écriture des réglages, démarrage du serveur avec des arguments JVM fixés et journalisés, attente de
la fin du démarrage, arrivée des bots, chauffe, mesure, arrêt, collecte des fichiers dans
`bench/results/<date>_<scénario>_<commit>/`.

Une seconde commande compare deux exécutions et produit le tableau avant et après en Markdown, prêt
pour le compte rendu de chaque tâche. Si les bots sont écrits en Rust, l'orchestrateur est une
sous-commande du même binaire, ce qui évite les scripts `.sh` et `.bat`, ignorés par git aujourd'hui.

### 5. Les mondes de départ et la référence

* Monde vanilla : graine fixe, zone prégénérée, sans mods.
* Monde EarthQuest : copie du monde de production avec ses mods et plugins, sans les données des
  joueurs.
* Les mondes ne vont pas dans git : leur chemin et leur hash sont écrits dans le scénario, pour
  qu'une exécution ne se lance pas sur un autre monde que celui prévu.

La référence est le build actuel de `main`, sur la machine de référence, avec la JVM de production
actuelle. Chaque scénario est joué trois fois ; on garde la médiane et l'écart.

## Fichiers concernés

Nouveaux :

| Chemin | Contenu |
| --- | --- |
| `tools/bench/` | Bots, crate `protocol`, orchestrateur et comparaison (si Rust est retenu) |
| `bench/scenarios/*.toml` | Les scénarios |
| `src/main/java/io/github/gammaengine/metrics/` | Sonde de niveau 1 et 2, résolution classe → mod, export CSV et JSON, écoute du GC |
| `src/test/java/io/github/gammaengine/metrics/` | Tests de la sonde et de l'export |
| `docs/banc-de-test.md` | Mode d'emploi |

Modifiés, chacun par un appel d'une ligne vers `io.github.gammaengine` :

| Fichier | Pourquoi |
| --- | --- |
| `patches/net/minecraft/server/MinecraftServer.java.patch` | Bornes des grandes phases du tick (déjà en partie présentes) |
| `patches/net/minecraft/world/World.java.patch` | Sondes de niveau 2 dans les boucles d'entités et de TileEntities |
| `patches/net/minecraft/world/WorldServer.java.patch` | Bornes du spawn, des ticks de blocs, du déchargement, du tracker |
| Gestionnaire d'événements Forge et Bukkit, scheduler Bukkit | Attribution des handlers et des tâches, si Timings v2 ne la donne pas déjà |
| `src/main/java/io/github/gammaengine/config/GammaConfig.java` | Interrupteurs `bench.*` |
| `.gitignore` | Exceptions pour les scripts et les résultats du banc |

Repris de l'existant : `TickStatistics`, `LatencyHistogram`, `MemoryReport`. `WorldBenchmark` reste
la source des scénarios `mobs` et `tile-entities` tant qu'aucun monde de départ ne les remplace.

## Découpage en commits

1. Mode d'emploi et format des scénarios.
2. Crate `protocol` : trames, handshake, statut, login, avec des tests sur des échanges enregistrés.
3. Handshake Forge, testé contre le serveur de développement.
4. Comportements des bots et mesures côté bot.
5. Sonde de niveau 1 et export, avec interrupteur.
6. Attribution par mod, par plugin et par chunk (niveau 2).
7. Orchestrateur et comparaison.
8. Mondes de départ, puis exécution et publication de la référence.

## Risques

| Risque | Conséquence | Parade |
| --- | --- | --- |
| Liste de mods refusée par le handshake Forge | Bots expulsés | Le bot lit la liste des mods dans la réponse de statut et la renvoie telle quelle, mod par mod et version comprise |
| Position non confirmée après une téléportation | Mouvements ignorés, un renvoi de position par seconde | Le bot répond à chaque position imposée par une position identique au bit près |
| Entrée en jeu terminée sur un thread Netty (déduit du code) | Une rafale de connexions peut toucher au monde en même temps que le tick : un plantage pendant le scénario `connexions` viendrait d'un défaut existant | Arrivée des bots étalée (5 à 10 par seconde) ; le scénario `connexions` sert aussi à confirmer ce point |
| Une exception de mod arrête le serveur (`removeErroringEntities` à faux) | Une exécution s'interrompt sans résultat | Le résumé enregistre la cause de l'arrêt ; ne pas changer ce réglage pendant le banc |
| Mod ou plugin qui attend une réponse du client sur son propre canal | Bots expulsés par ce mod | Journaliser la raison ; ce n'est pas une liste dans le moteur, seulement un réglage du banc |
| Plugins anti-triche ou anti-bot d'EarthQuest | Expulsions, ou coût différent de la production | Décider s'ils restent pendant le banc, et le noter dans chaque résultat |
| Mode hors ligne nécessaire pour les bots | Faille si ce réglage arrivait en production | Réglages du banc appliqués à une copie, jamais au serveur de production |
| Bots sur la même machine que le serveur | CPU volé au serveur, mesure faussée | Bots sur une autre machine, ou cœurs réservés et CPU des bots mesuré |
| Coût de la sonde de niveau 2 | MSPT absolu gonflé | Comparer niveau 2 contre niveau 2 ; les chiffres absolus viennent du niveau 1 |
| JFR indisponible sur la JVM 8 installée (Oracle 8u202) | Pas de profil sur la référence Java 8 | OpenJDK 8u262 ou plus récent, ou Java 21 après la phase 1 |
| Variance (spawn de mobs, GC, disque) | Gains illusoires | Trois répétitions, médiane et écart, monde de départ identique |
| Poste de développement Windows contre machine cible Linux | Chiffres absolus différents | Comparer seulement sur la même machine ; la référence vient de la machine cible |

## Questions ouvertes

1. Langage des bots : Rust (recommandé), Java 21 ou Node ?
2. Machine de référence pour les mesures, et machine pour les bots ?
3. JVM et arguments utilisés aujourd'hui en production sur EarthQuest ?
4. Accès au modpack et à une copie du monde d'EarthQuest pour les scénarios réalistes ?
5. Plugins anti-triche ou anti-bot : les garder pendant le banc ?

## Critères de sortie

* Une commande joue un scénario et produit les fichiers d'export et le résumé.
* Une seconde commande compare deux exécutions.
* 400 bots tiennent dix minutes sans expulsion sur le serveur de banc.
* La référence est publiée pour tous les scénarios sur monde vanilla, et pour les scénarios
  réalistes sur le modpack d'EarthQuest si le monde est fourni.
