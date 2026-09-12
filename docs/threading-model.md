# Modèle de threading de GammaEngine

Ce document est le contrat. Tout patch, sous-système ou optimisation qui viole une règle ci-dessous
est faux, même s'il rend un banc d'essai plus rapide.

## Ordre de priorité

Quand deux objectifs s'opposent, le premier de la liste gagne :

1. **Intégrité du monde.** Aucune mise à jour perdue, aucun item dupliqué, aucun inventaire
   désynchronisé.
2. **Aucune corruption silencieuse.** Une défaillance doit être bruyante. Une optimisation qui peut
   corrompre en silence est retirée, pas documentée.
3. **Compatibilité.** Les mods Forge, les plugins Bukkit, les mondes existants, les formats de
   sauvegarde existants et le protocole 1.7.10 continuent de fonctionner, sans modification.
4. **Stabilité.** Aucun deadlock, aucun blocage sans borne, aucun crash qu'un seul thread n'aurait
   pas eu.
5. **Performances.** En dernier, et uniquement dans le respect des quatre règles précédentes.

## Propriété

* Chaque chunk chargé a exactement une **région propriétaire** à un instant donné.
* Chaque entité a une région propriétaire unique, celle qui possède le chunk où elle se trouve.
* Chaque TileEntity appartient à la région propriétaire de son chunk.
* Une donnée mutable du monde n'est jamais écrite que par le thread qui exécute son propriétaire.
* Les changements de propriété sont atomiques et se produisent entre deux ticks des objets
  concernés, jamais pendant.

Le thread serveur principal possède tout ce qui n'est pas encore assigné à une région, et reste
propriétaire de l'état réellement global (l'horloge du monde, la liste des joueurs, les registres
globaux de Forge).

## La règle de l'accès inconnu

Le runtime démarre pessimiste et gagne son parallélisme :

```
accès inconnu -> ordonnancement sûr -> observation -> optimisation plus tard
```

Une classe, une méthode, un type de TileEntity ou un listener que le runtime n'a jamais vu s'exécute
sérialisé. Il ne devient éligible à l'exécution parallèle qu'après avoir été observé, de façon
répétée, en train de ne rien faire de dangereux. L'inverse est immédiat : un seul conflit, une seule
exception de threading ou un seul comportement non déterministe détecté, et le runtime rabaisse le
parallélisme de ce composant, à la granularité la plus fine possible (un objet, une classe, une
méthode), jamais en désactivant un mod entier.

## Classification des accès

| Type | Signification | Politique par défaut |
| --- | --- | --- |
| `READ` | Lit une donnée possédée par la région courante | Parallèle |
| `WRITE` | Écrit une donnée possédée par la région courante | Parallèle |
| `GLOBAL_READ` | Lit un état partagé par toutes les régions | Parallèle si l'état est immuable ou vu par snapshot |
| `GLOBAL_WRITE` | Écrit un état partagé | Sérialisé sur l'exécuteur propriétaire |
| `CROSS_REGION_READ` | Lit une donnée possédée par une autre région | Snapshot, ou transaction |
| `CROSS_REGION_WRITE` | Écrit une donnée possédée par une autre région | Transaction multi-régions |
| `ASYNC_COMPUTE` | Ne touche aucun état mutable du monde | Libre sur n'importe quel pool |

## Règles de verrouillage

* Les opérations multi-régions acquièrent les verrous de région **par identifiant croissant**,
  toujours. Un cycle de verrous devient donc impossible par construction, pas par vigilance.
* Chaque acquisition a un timeout. Un timeout est un événement de diagnostic, pas une nouvelle
  tentative silencieuse.
* Les propriétaires de verrous sont enregistrés, pour que le watchdog puisse nommer le thread, la
  région, la plage de chunks et la tâche bloquée.
* Aucun verrou global autour du code des mods. Un verrou global unique donnerait un serveur correct
  sans aucun des bénéfices, c'est-à-dire exactement le mode de défaillance que ce projet veut éviter.

## Ce que le runtime peut faire quand il détecte un conflit

Par ordre de préférence, du moins coûteux au plus perturbant :

1. Sérialiser les deux tâches sur le même exécuteur.
2. Reporter la tâche perdante au tick suivant de sa région.
3. Prendre un snapshot et laisser le lecteur travailler sur la copie.
4. Ouvrir une transaction multi-régions couvrant les deux régions.
5. Fusionner les deux régions, si elles continuent d'entrer en conflit.
6. Mettre en quarantaine la classe ou l'objet fautif, en abaissant son parallélisme jusqu'à son
   prochain test.

Les mods ne voient jamais une exception de threading que le runtime sait résoudre lui-même.

## Identité de thread

* `AutoThreadRuntime.isMainThread()` répond à « suis-je le thread serveur de Minecraft ».
* Le contexte de région, une fois les régions en place, répond à la question plus fine : « quelle
  région ai-je le droit d'écrire en ce moment ». Le code incapable de répondre n'a pas le droit
  d'écrire d'état du monde du tout.

## Règles pour tout nouveau code de ce fork

* Les patches sur les classes Minecraft, Forge et Bukkit délèguent ; ils ne contiennent pas de
  logique.
* Tout ce qui peut bloquer (disque, réseau, compression, `synchronized` sur des structures
  partagées) ne s'exécute jamais sur le pool de tick de régions.
* Tout ce qui touche l'état mutable du monde ne s'exécute jamais sur les pools IO, worker, async ou
  natif.
* Chaque nouveau chemin parallèle arrive avec la métrique qui prouve qu'il est plus rapide et le
  compteur qui prouve qu'il n'est pas en conflit.
