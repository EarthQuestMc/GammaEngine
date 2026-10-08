# Carte du code

Ces documents décrivent ce que fait réellement le serveur aujourd'hui, thread par thread, avec la
preuve de chaque affirmation (`chemin:ligne`). Ils servent à vérifier ce que Crucible fait déjà
avant de changer quoi que ce soit.

| Document | Sujet |
| --- | --- |
| [tick.md](tick.md) | La boucle de tick : serveur, mondes, entités, TileEntities, événements, mécanismes hérités |
| [chunks.md](chunks.md) | Chargement, génération, déchargement, sauvegarde, envoi aux joueurs et lumière |
| [reseau.md](reseau.md) | Netty, états de connexion, handshake Forge, services annexes, journaux |
| [chargement.md](chargement.md) | Du `java -jar` au premier tick : classloaders, transformers, mods, plugins, Java moderne |
| [patches-et-build.md](patches-et-build.md) | Système de patches, tâches Gradle, tests, bibliothèque native, CI |
| [mesure.md](mesure.md) | Ce qui mesure déjà le serveur, Timings, sites de sonde pour l'attribution par mod et par chunk |

Synthèse par phase de la feuille de route : [existant.md](../existant.md).

## Conventions de lecture

* Les chemins préfixés `E/` ou `E:` désignent `eclipse/cauldron/src/main/java/`, le workspace patché
  généré par `setupCrucible` et absent de git. Les numéros de ligne s'y rapportent : ils changent à
  chaque modification d'un patch.
* `S/` ou `S:` désigne `src/main/java/`, et `P/` le dossier `patches/`.
* Rien n'a été exécuté pour écrire ces cartes : tout vient de la lecture du code. Ce qui est déduit
  sans avoir été confirmé à l'exécution est signalé comme tel.
* Le serveur de test `run/` ne contient ni mod ni plugin : aucun coût de ces documents n'est
  chronométré sur un vrai modpack. Ces mesures sont l'objet de la [phase 0](../phase-0.md).

## Le serveur en une page

* **Un seul thread simule tout.** Tick du serveur, des mondes, des entités, des TileEntities, des
  événements Forge et Bukkit, génération et décoration des chunks, sauvegarde automatique : tout
  passe par le thread serveur.
* **Ce qui tourne ailleurs.** Netty (entrées-sorties, encodage, compression des paquets de chunks,
  handshake Forge et probablement la fin de l'entrée en jeu des joueurs moddés), le pool de
  lecture de chunks de Forge (lecture, décompression, NBT ; pas les entités), le thread d'écriture
  des régions, la file de la console.
* **Un seul classloader** porte Minecraft, Forge, le moteur et tous les mods ; chaque plugin Bukkit
  a le sien, avec un remapping paresseux classe par classe.
* **Ce que le fork a déjà changé** : points d'accroche du cycle de vie et mesure du tick dans
  `MinecraftServer`, préchargement parallèle du spawn, tampons et niveau de compression des paquets
  de chunks, vérification parallèle des bibliothèques au démarrage, marque.
