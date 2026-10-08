# Modèle de threading de GammaEngine

Ce document est le contrat. Tout patch, sous-système ou optimisation qui viole une règle ci-dessous
est faux, même s'il rend le banc d'essai plus rapide.

## Ordre de priorité

Quand deux objectifs s'opposent, le premier de la liste gagne :

1. **Intégrité du monde.** Aucune mise à jour perdue, aucun item dupliqué, aucun inventaire
   désynchronisé.
2. **Aucune corruption silencieuse.** Une défaillance doit être bruyante. Une optimisation qui peut
   corrompre en silence est retirée, pas documentée.
3. **Compatibilité.** Les mods Forge, les plugins Bukkit, les mondes existants et le protocole
   1.7.10 continuent de fonctionner, sans modification.
4. **Stabilité.** Aucun deadlock, aucun blocage sans borne, aucun crash qu'un seul thread n'aurait
   pas eu.
5. **Performances.** En dernier, et uniquement dans le respect des quatre règles précédentes.

## Le thread propriétaire

* Chaque monde a un thread propriétaire. Aujourd'hui c'est le thread serveur pour tous les mondes ;
  la phase 10 donnera un thread par dimension, la phase 11 des zones tickées en parallèle.
* Le code des mods et des plugins s'exécute sur le thread propriétaire de son monde, sauf s'il a été
  classé sûr pour le parallèle par le système de compatibilité (phase 3).
* Une donnée mutable du monde n'est écrite que par son thread propriétaire. Les autres threads
  travaillent sur des copies (snapshots) ou sur des données qui ne touchent pas le monde : I/O,
  compression, calcul pur.

## Série par défaut

Une classe que le moteur n'a pas analysée, ou dont l'analyse n'est pas certaine, suit le chemin en
série identique à Crucible. La promotion au parallèle est l'exception et se gagne par l'analyse ; la
rétrogradation est immédiate : une violation ou une exception liée à une optimisation renvoie la
classe ou le mod en série sans arrêter le serveur, et la décision est conservée.

## Phases parallèles

Le parallélisme du tick prend une seule forme tant que la phase 11 n'est pas faite :

1. le monde est figé ;
2. un calcul en lecture seule s'exécute sur plusieurs threads ;
3. le résultat est appliqué en série par le thread propriétaire.

Aucune écriture du monde n'a lieu pendant l'étape 2. Les candidats sont les cibles des mobs, le
pathfinding, les collisions, les positions de spawn et l'entity tracker.

## Garde-fou

Tout accès au monde depuis un mauvais thread est détecté, puis redirigé vers le thread propriétaire
ou journalisé avec le nom du mod. Les mods ne voient jamais une exception de threading que le moteur
sait résoudre lui-même.

## Règles pour tout nouveau code du fork

* Les patches sur les classes Minecraft, Forge et Bukkit délèguent à `io.github.gammaengine` ; ils
  ne contiennent pas de logique.
* L'invariant de thread de tout code concurrent est écrit en commentaire, au-dessus du code qu'il
  protège : quel thread écrit, quels threads lisent, ce qui garantit la visibilité.
* Tout code concurrent arrive avec un test de charge.
* Rien de ce qui peut bloquer (disque, réseau, `synchronized` sur une structure partagée) ne
  s'exécute pendant une phase parallèle du tick.
* Un thread de plus n'est ajouté que s'il réduit la part en série du tick, mesure à l'appui.
