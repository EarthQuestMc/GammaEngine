# Feuille de route GammaEngine

Ce document est le plan de travail et le suivi d'avancement. Il remplace, depuis le 2 octobre 2026,
le plan précédent qui commençait par le tick parallèle par régions ; la correspondance entre les
deux plans est donnée en fin de document.

## Objectif

Tenir un maximum de joueurs à 20 TPS et démarrer vite, en optimisant et en parallélisant le moteur,
sans rien demander aux mods ni aux plugins. Le serveur visé est EarthQuest.

| Cible | Valeur |
| --- | --- |
| Tick moyen en pointe | 30 à 35 ms |
| Tick p99 | moins de 50 ms (50 ms = limite de 20 TPS) |
| Pauses GC | moins de 10 ms |
| Édition en masse | 5 à 10 ms par tick au plus |
| Indicateur principal | coût par joueur : 50 ms divisé par le nombre de joueurs |

Le coût par joueur est l'indicateur qui décide : tout ce dont le coût par joueur augmente avec le
nombre de joueurs casse le budget avant que les cœurs ne soient saturés. Le but est de réduire la
part en série du tick, pas d'ajouter des threads.

## Contraintes non négociables

1. Les mods et plugins se déposent dans `mods/` et `plugins/` sans être recompilés ni modifiés. Un
   jar tiers n'est jamais édité : toute correction se fait par transformation de bytecode au
   chargement.
2. Le moteur ne connaît pas à l'avance les mods et plugins. Il gère automatiquement n'importe quel
   jar, sans liste fournie et sans intervention de l'administrateur.
3. Le code des mods et plugins s'exécute toujours sur le thread propriétaire de son monde, sauf s'il
   a été classé sûr pour le parallèle.
4. Les signatures et structures visibles par les mods (champs publics de `World` et `Chunk`, listes
   d'entités et de TileEntities) gardent leur type et leur comportement.
5. Le serveur fonctionne sans aucune configuration. Les interrupteurs de `gammaengine.yml` sont
   facultatifs, et chaque optimisation en a un.
6. On ne supprime jamais une classe qu'un mod peut référencer : on la garde comme coquille et on
   neutralise ce qu'elle fait.

Conventions : fichier de configuration `gammaengine.yml`, préfixe de log `[GammaEngine]`, nouvelles
classes dans `io.github.gammaengine`, séparées du code hérité de Crucible.

## Méthode

* **Mesurer avant d'optimiser.** Aucun changement de performance sans profil avant et après (Spark,
  JFR, async-profiler) sur le banc de test de la phase 0.
* **Vérifier ce que Crucible fait déjà** avant de supposer : la [carte du code](carte/README.md) et
  l'[état de l'existant](existant.md) servent à cela.
* **Respecter le système de patches** (`setupCrucible`, `genPatches`) : un patch par sujet, petit et
  relisible ; la logique vit dans `io.github.gammaengine` et le patch d'une classe vanilla se réduit
  à un appel.
* **Code concurrent** : l'invariant de thread est écrit en commentaire, et un test de charge
  l'accompagne.
* **Mods inconnus** : chaque optimisation est testée contre des mods que le moteur ne connaît pas ;
  le cas par défaut doit toujours fonctionner.
* **Risque de casse** : une optimisation qui peut casser un mod est signalée, jamais livrée en
  silence. En cas de doute sur une contrainte, la question est posée avant de trancher.

Compte rendu attendu après chaque tâche : ce qui a changé, les mesures avant et après,
l'interrupteur de configuration ajouté, les risques de compatibilité, la suite proposée.

## Où on en est

| Phase | Sujet | État |
| --- | --- | --- |
| 0 | Banc de test : bots 1.7.10, scénarios, métriques | **Plan prêt** : [phase-0.md](phase-0.md) |
| 1 | JVM 21 avec ZGC, script de lancement, compatibilité | **Premier essai fait**, sans mods : [phase-1-essai-java21.md](phase-1-essai-java21.md) |
| 2 | Audit et neutralisation du code inutile | **Audit fait** : [audit-code-inutile.md](audit-code-inutile.md) |
| 3 | Système de compatibilité automatique | À faire |
| 4 | Boucle de tick | À faire |
| 5 | Phases parallèles en lecture seule, pipeline par joueur | À faire |
| 6 | Chargement des chunks et prégénération | À faire |
| 7 | Lumière asynchrone, sauvegarde hors thread, réseau | À faire |
| 8 | Édition en masse | À faire |
| 9 | Démarrage rapide | Partiel : vérification parallèle des bibliothèques, préchargement du spawn |
| 10 | Un thread par dimension | À faire |
| 11 | Tick hybride par régions | À faire |
| 12 | Proxy et stockage en Rust, sharding | Partiel : bibliothèque native (zlib, XXH64), pas encore appelée |

Le détail de ce qui existe déjà, élément par élément, est dans [existant.md](existant.md).

---

## Phase 0 — Banc de test

Objectif : pouvoir mesurer n'importe quel changement, de façon reproductible, avant d'en faire un
seul.

Contenu : des bots qui parlent le protocole 1.7.10 et passent le handshake Forge ; des scénarios de
charge rejouables (joueurs groupés au spawn, dispersés, exploration, bases industrielles, mobs,
TileEntities, édition en masse) ; l'export des métriques : MSPT moyen et percentiles, TPS, coût par
chunk, coût par mod et par plugin, pauses GC, coût par joueur.

Critères de sortie : une commande lance un scénario et produit un rapport comparable à un autre ;
une référence est enregistrée sur monde vanilla et sur le modpack d'EarthQuest.

Plan détaillé, fichiers concernés et risques : [phase-0.md](phase-0.md).

## Phase 1 — JVM 21 avec ZGC

Objectif : faire tourner le serveur sur une JVM 21 avec ZGC générationnel, sans que les mods, qui
restent en bytecode Java 8, s'en aperçoivent.

Contenu :

1. Script de lancement (Linux et Windows) avec les arguments JVM, dont les `--add-opens`
   nécessaires, et ZGC générationnel (`-XX:+UseZGC -XX:+ZGenerational` sur Java 21).
2. Transformers de compatibilité appliqués au chargement : réflexion sur les internes du JDK, cast
   du classloader système, API retirées.
3. Bibliothèques retirées du JDK embarquées quand elles manquent encore.
4. Build : le code du moteur peut utiliser Java 21 (threads virtuels pour les I/O) ; le reste garde
   sa cible actuelle.

Critères de sortie : le serveur démarre et tient le banc sur Java 21 avec le modpack d'EarthQuest ;
pauses GC sous 10 ms mesurées ; comparaison avant et après sur le banc. Java 25 est à évaluer plus
tard : il demande d'abord de remplacer le `SecurityManager` de FML, retiré en Java 24.

Premier essai sur un monde vide, sans mod : [phase-1-essai-java21.md](phase-1-essai-java21.md).

## Phase 2 — Code inutile

Objectif : retirer du serveur le travail qui ne sert à rien, sans casser une seule référence.

Contenu, après vérification que chaque élément existe :

* neutraliser : snooper, auto-updater CraftBukkit, vérification de version Forge, anciennes
  métriques, interface Swing, mode démo, convertisseurs d'anciens formats, ping legacy, restes de
  Cauldron et Thermos ;
* désactiver par défaut : spawn chunks, sauvegarde des données de structures, profiler vanilla,
  Timings v1 (en gardant la façade d'API), RCON et Query, génération de l'aide Bukkit, chargement
  d'Ebean ;
* ne pas toucher : statistiques et succès (sauvegarde asynchrone seulement), remapping des plugins
  (mis en cache), commandes vanilla, Forge et Bukkit.

Critères de sortie : chaque élément a un interrupteur ; aucune classe référençable supprimée ;
démarrage et tick mesurés avant et après.

## Phase 3 — Compatibilité automatique

Objectif : que le moteur sache seul, pour n'importe quel jar, ce qui peut tourner en parallèle et ce
qui doit rester en série.

Contenu :

1. Découverte au démarrage : scan de `mods/` et `plugins/`, un hash par jar.
2. Analyse de bytecode par classe (TileEntity, Entity, listeners, générateurs de monde) : état
   statique modifiable, accès au-delà des blocs voisins, threads créés, API retirées du JDK,
   réflexion sur les internes.
3. Classement prudent : par défaut, chemin en série identique à Crucible ; promotion au parallèle
   seulement si l'analyse est certaine.
4. Garde-fou de thread : tout accès au monde depuis un mauvais thread est détecté, redirigé ou
   journalisé avec le nom du mod.
5. Rétrogradation automatique à chaud : en cas de violation ou d'exception liée à une optimisation,
   la classe ou le mod repasse en série sans arrêt du serveur, et la décision est conservée.
6. Cache des décisions indexé par hash de jar, invalidé quand un jar change.
7. Rapport informatif à chaque démarrage : classement par mod, corrections appliquées,
   rétrogradations.

Critères de sortie : un mod de test volontairement dangereux est classé en série ; une violation
provoquée en jeu rétrograde la classe sans arrêt ; le cache survit à un redémarrage et s'invalide
quand le jar change.

## Phase 4 — Boucle de tick

Objectif : rendre le tick monothread moins cher, à comportement identique.

Contenu : collections sans boxing (fastutil) ; itération des TileEntities et des entités sans
suppressions en O(n) ; TileEntities qui ne tickent pas exclues de la boucle ; cache des chunks
éligibles au spawn ; distance de simulation séparée de la distance de vue ; limites par chunk
(entités, TileEntities, chunkloaders) ; ralentissement adaptatif quand le TPS baisse ; index des
recettes de craft et de four ; cache des collisions ; redstone sans mises à jour redondantes ;
exécuteurs d'événements Bukkit générés en bytecode ; détection des I/O bloquantes sur le thread
principal.

Critères de sortie : chaque point a son interrupteur et sa mesure ; les listes visibles par les
mods gardent leur type et leur comportement.

## Phase 5 — Phases parallèles en lecture seule et pipeline par joueur

Objectif : sortir du tick série les calculs qui ne font que lire.

Contenu :

1. Phases parallèles dans le tick : monde figé, calcul sur plusieurs threads, application en série.
   Candidats : cibles des mobs, pathfinding, collisions, positions de spawn, entity tracker.
2. Pipeline par joueur : construction et compression des paquets de chunks, visibilité des entités,
   sauvegarde des données joueur.

Critères de sortie : résultats identiques à l'exécution en série sur le banc ; réduction mesurée de
la part série du tick ; seules les classes classées sûres en phase 3 sont concernées.

## Phase 6 — Chargement des chunks et prégénération

Contenu : pool d'I/O dimensionné selon le disque ; file à priorités (proximité et direction des
joueurs) ; annulation des demandes inutiles ; budget de temps par tick pour la finalisation sur le
thread principal ; création des entités et TileEntities en parallèle uniquement pour les classes
sûres ; génération de base hors thread seulement si aucun mod ne s'y accroche ; décoration en série
avec budget ; journalisation et accélération des chargements synchrones déclenchés par les mods ;
délai avant déchargement et cache des chunks récents ; limite d'envoi par joueur ; distance de vue
adaptative ; pas de spawn chunks permanents ; outil de prégénération de carte avec bordure de monde.

Critères de sortie : scénario d'exploration mesuré avant et après ; aucun chunk perdu ou dupliqué ;
mondes lisibles par Crucible amont.

## Phase 7 — Lumière, sauvegarde, réseau

Contenu : sous-systèmes hors du thread principal. Sauvegarde : snapshot sur le thread propriétaire
puis écriture ailleurs. Lumière asynchrone. Réseau et logs hors du thread principal.

Critères de sortie : le pic de sauvegarde disparaît du p99 ; un crash pendant une sauvegarde laisse
un monde chargeable ; éclairage identique à la référence sur les scénarios du banc.

## Phase 8 — Édition en masse

Objectif : WorldEdit et les autres plugins d'édition, sans toucher à leur jar.

Contenu : détection automatique quand un plugin pose beaucoup de blocs dans un tick ; écriture
directe dans les sections de chunk ; lumière différée ; un paquet de chunk au lieu d'un paquet par
bloc ; écritures en file étalées sur plusieurs ticks ; calculs hors thread sur des snapshots. Les
blocs avec TileEntity ou logique de pose gardent le chemin normal.

Critères de sortie : édition en masse limitée à 5 à 10 ms par tick ; résultat identique au chemin
normal.

## Phase 9 — Démarrage rapide

Contenu : cache disque des classes transformées et du scan d'annotations (clé : hash de `mods/`,
`plugins/` et de la configuration) ; cache du remapping des plugins ; AppCDS ; CRaC en option.

Critères de sortie : démarrage mesuré à froid et à chaud ; cache invalidé dès qu'un jar ou la
configuration change.

## Phase 10 — Un thread par dimension

Contenu : chaque dimension tickée sur son propre thread, avec verrouillage des téléportations et
des échanges entre dimensions.

## Phase 11 — Tick hybride par régions

Contenu : zones éloignées tickées en parallèle pour le vanilla et les classes classées sûres, puis
phase en série pour le reste.

## Phase 12 — Rust et sharding

Contenu : proxy en Rust (connexion, anti-bot, limitation de débit, routage, relais du handshake
Forge) ; stockage des chunks en Rust (zstd, I/O asynchrones, convertisseur Anvil) ; outils hors
ligne ; sharding : plusieurs instances derrière le proxy, données partagées en base.

Rust est réservé à ce qui traite de gros blocs d'octets avec peu d'allers-retours. Jamais le tick,
les entités, les TileEntities, la lumière, le pathfinding ni les API. Pont par JNI, avec repli en
Java pur pour chaque bibliothèque native.

---

## Travaux transverses

**Intégration continue.** Build Java, build Rust, tests unitaires, test de démarrage du serveur et
banc de fumée.

**Intégrité et déterminisme.** Un monde reste lisible après un crash contrôlé, un arrêt sous
charge, des sauvegardes répétées et un déchargement-rechargement de chunk ; deux exécutions d'un
même scénario produisent le même monde, vérifié en hachant le contenu décompressé des chunks.

**Banc de compatibilité.** IC2, BuildCraft, AE2, Thermal Expansion, Thaumcraft, Ender IO, Mekanism,
GregTech, Galacticraft, Twilight Forest, Railcraft, MineFactory Reloaded, ComputerCraft,
OpenComputers, plus WorldEdit, WorldGuard, Vault, PermissionsEx, un équivalent d'Essentials et un
ProtocolLib compatible 1.7.10.

## Correspondance avec l'ancien plan

Le plan précédent visait d'abord le tick parallèle par régions, avec un suivi des accès à
l'exécution. Le nouveau plan commence par la mesure et les gains sans risque, et ne régionalise
qu'en dernier. Ce qui était prévu n'est pas perdu, il change de place :

| Ancienne phase | Où elle va |
| --- | --- |
| 0 — Build, référence, métriques | 0 : la couche de mesure est reprise ; bots, scénarios et attribution par chunk et par mod restent à faire |
| 1 — Optimiser les chemins existants | 4 (boucle de tick), 5 (paquets de chunks), 9 (démarrage) |
| 2 — Chargement et sauvegarde asynchrones | 6 et 7 |
| 2B — Tracking, streaming de chunks, sauvegarde étalée | 5 (entity tracker, pipeline par joueur) et 7 (sauvegarde) |
| 3 — Régions : propriété et contexte | 11 |
| 4 — Suivi des accès et conflits | 3 (garde-fou de thread et classement) |
| 5 — Ordonnanceur de régions | 11 |
| 6 — Ordonnanceur d'entités, transactions inter-régions | 10 et 11 |
| 6B — Plugins, commandes et API en multithread | 3 (classement des listeners), puis 11 |
| 7 — Instrumentation bytecode des mods | 3 |
| 8 — Auto-quarantaine et apprentissage | 3 (rétrogradation à chaud, cache des décisions) |
| 9 — Bibliothèque native Rust | 12 (stockage des chunks) ; pathfinding et collisions en Rust abandonnés |
| 10 — Pathfinding, lumière, collisions | 5 (phases en lecture seule) et 7 (lumière) |
| 11 — Dégradation adaptative, comptabilité des coûts | 4 (ralentissement adaptatif), 6 (distance de vue adaptative), 0 (coût par mod) |
