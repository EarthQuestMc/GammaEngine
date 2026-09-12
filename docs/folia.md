# Folia comme référence, et là où GammaEngine doit diverger

Folia est la barre à atteindre. C'est le seul serveur qui ticke réellement un monde sur plusieurs
threads en production, et sa conception est le bon point de départ. Ce document dit ce que
GammaEngine reprend de Folia, ce qu'il ne peut pas reprendre, et ce que cela coûte.

## Ce que fait Folia

Folia découpe le monde chargé en régions de chunks adjacents qui n'interagissent pas, et donne à
chaque région sa propre boucle de tick sur un pool de threads. Une région possède ses chunks, ses
entités et ses TileEntities ; rien d'extérieur n'a le droit d'y toucher. Les régions fusionnent
quand elles se rapprochent assez pour interagir et se séparent quand elles s'éloignent. Les plugins
disposent de quatre schedulers : global, région, entité et asynchrone. Le code qui touche la
mauvaise région est attrapé par un contrôle de thread et lève une exception.

Cette dernière phrase résume toute la différence entre Folia et ce projet.

## Pourquoi on ne peut pas le copier tel quel

| | Folia | GammaEngine |
| --- | --- | --- |
| Compatibilité plugins | Cassée volontairement, les plugins doivent être réécrits pour Folia | Ne doit pas casser. Les plugins 1.7.10 ne sont plus maintenus |
| Mods | Aucun. Paper n'a pas de mods Forge | Des milliers de mods Forge, pleins d'état statique |
| Accès depuis le mauvais thread | Lève une exception, et l'auteur du plugin corrige | Doit être absorbé : détecter, sérialiser, et continuer |
| Travail préalable | Des années de réécriture du système de chunks et du tracking dans Paper | Doit être fait ici d'abord |
| Version cible | 1.20+, code moderne | 1.7.10, une base décompilée de 2014 |

Folia a déplacé le coût de la correction sur les auteurs de plugins. Nous n'avons personne sur qui
le déplacer : un modpack 1.7.10 est un ensemble figé de jars que personne ne mettra à jour. Le même
travail de correction doit donc se faire dans le serveur, à l'exécution, et c'est exactement ce
qu'est le runtime AutoThread : observer ce qu'un morceau de code touche vraiment, ne l'exécuter en
parallèle que lorsque c'est prouvé sûr, et le sérialiser silencieusement sinon.

C'est strictement plus difficile que Folia, et c'est la seule version du problème qui soit utile
ici.

## Ce qu'on reprend de Folia, tel quel

* **Les régions de chunks adjacents comme unité de parallélisme.** Pas un thread par chunk, pas un
  thread par entité, pas un thread par mod.
* **La fusion et la séparation** pilotées par la proximité et l'interaction, pas par une grille fixe.
* **Une région possède tout ce qu'elle contient**, et un tick de région est monothread du point de
  vue du code qui s'y exécute. Le code des mods continue de voir le monde monothread pour lequel il
  a été écrit ; c'est cette propriété qui rend toute l'approche viable.
* **Une boucle de tick par région.** Le tick global devient un coordinateur qui gère l'horloge, la
  maintenance et les phases réellement globales, pas l'endroit où se fait la simulation.
* **Quatre schedulers** pour le code neuf : global, région, entité, asynchrone. Proposés, jamais
  obligatoires.
* **La migration atomique d'entité** entre régions, sans qu'elle soit jamais tickée deux fois.

## Là où on va plus loin, par obligation

* **De la détection au lieu d'exceptions.** Chaque accès qui lèverait une exception dans Folia
  devient ici une observation : quelle région, quel objet, quel mod, quel type d'accès.
* **Sérialisation automatique en cas de conflit**, le conflit étant compté et rapporté plutôt que
  montré au mod.
* **Quarantaine granulaire.** Quand un composant persiste à entrer en conflit, il perd son
  parallélisme seul : une classe, un type de TileEntity, un listener, un objet, jamais le mod
  entier.
* **Apprentissage conservé entre redémarrages**, invalidé quand un jar de mod, le serveur ou
  l'instrumentation changent.

## Le préalable que Paper avait et que nous n'avons pas

Folia n'a pas commencé par les régions. Paper a d'abord réécrit le système de chunks pour charger et
générer de façon asynchrone, puis réécrit le tracking d'entités, et seulement ensuite régionalisé.
L'ordre n'est pas un hasard : les régions ne peuvent rien tant que le système de chunks bloque le
tick et tant que le tracking coûte le carré du nombre de joueurs.

C'est pourquoi le plan de ce projet place maintenant la phase 2 (snapshots de chunks, chargement et
sauvegarde asynchrones) et la phase 2B (tracking d'entités, streaming de chunks, sauvegarde étalée)
**avant** le travail sur les régions des phases 3 à 6. Les mesures de [scaling.md](scaling.md)
disent la même chose indépendamment : à 400 joueurs les premiers murs sont le tracking et le
streaming, pas la boucle de tick.

## Au-delà de Folia : le catalogue d'optimisations 1.7.10

« Niveau Folia » veut aussi dire tout le travail monothread accumulé par Paper au fil des années, et
l'équivalent que la communauté 1.7.10 a déjà éprouvé. Voici les candidats, chacun à mesurer sur ce
dépôt avant d'être livré, chacun indépendant du modèle de threading :

| Domaine | Optimisation | Pourquoi elle compte ici |
| --- | --- | --- |
| Paquets de chunks | Cache partagé de charge utile compressée, réglage du niveau, compression hors thread | Mesuré : jusqu'à 1,4 cœur à 400 joueurs |
| Recherche d'entités | Index spatial au lieu de balayages linéaires pour le tracking et les collisions | Supprime un coût quadratique |
| Redstone | Algorithme de mise à jour type Alternate Current | Les tempêtes de redstone sont un tueur de tick classique en 1.7.10 |
| Hoppers et pipes | Vérifications de transfert par lots, ignorer les inventaires inactifs | La TileEntity la plus nombreuse sur les serveurs moddés |
| Lighting | Calcul hors thread avec application par le propriétaire, validé contre la sémantique 1.7.10 | Pics de génération de chunks |
| Collisions | Tests d'AABB par lots, rejet précoce | Zones à forte densité d'entités |
| Entités item | Fusion agressive et politique de despawn sous pression | Bases avec des milliers d'items au sol |
| Spawn de mobs | Caps et budget de spawn par région au lieu d'une passe globale | Suit le nombre de joueurs, pas la taille du monde |
| NBT | Moins d'allocations, tags immuables partagés, encodage binaire natif | Deuxième source de déchets après le streaming de chunks |
| Mémoire | Collections primitives sur les structures chaudes, pas de coordonnées boxées | Fait baisser directement les pauses GC |

Rien de tout cela n'est exotique. C'est la différence entre un serveur qui survit à 250 joueurs et
un serveur qui n'y survit pas.

## Ce que « pratique » veut dire

L'hybride doit rester un hybride. Les règles qui en découlent, et qu'aucune optimisation n'a le
droit de casser :

* Un administrateur dépose ses jars dans `mods/` et `plugins/` et démarre le serveur. Rien d'autre.
* Aucune configuration par mod, aucune liste de compatibilité à maintenir, aucun mode de threading à
  choisir.
* Les mondes existants se chargent, les sauvegardes existantes restent valides, le protocole 1.7.10
  n'est pas touché.
* Un mod qui ne peut pas être parallélisé tourne sérialisé et correct, il ne plante pas.
* Tout ce que le runtime décide est visible dans `/autothread` et dans les rapports, pour qu'un
  problème puisse être expliqué au lieu d'être deviné.
