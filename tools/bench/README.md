# Banc de test : bots de charge

Espace de travail Cargo de la phase 0 : des clients Minecraft 1.7.10 minimaux (protocole 5,
Forge, mode hors ligne) qui rejoignent le serveur, y restent et s'y déplacent, pour mesurer le
serveur sous une charge de joueurs reproductible. La séquence des paquets et les réglages du
serveur sont dans [`docs/phase-0-bots.md`](../../docs/phase-0-bots.md).

Le même exécutable porte l'orchestrateur du banc : `gamma-bots run <scénario.toml>` joue un
scénario de `bench/scenarios/` de bout en bout (serveur neuf, bots, chauffe, enregistrement,
arrêt, résultats dans `bench/results/`) et `gamma-bots compare <avant> <après>` écrit le tableau
avant et après. Mode d'emploi : [`docs/banc-de-test.md`](../../docs/banc-de-test.md).

## Les deux crates

### `gammaengine-protocol` (`protocol/`, bibliothèque)

Le format des échanges, sans socket, sans thread ni horloge : la même crate servira au proxy de
la phase 12.

| Module | Contenu |
|---|---|
| `varint` | VarInt et VarLong, avec un budget d'octets pour les VarInt de Forge |
| `buf` | `Reader` et `Writer` : entiers et flottants gros-boutistes, chaînes Minecraft avec les limites du serveur |
| `frame` | trames `longueur VarInt, id VarInt, corps` ; `FrameDecoder` incrémental qui jette les trames filtrées au fil de l'eau sans les garder en mémoire ; `Frame::raw` rend la trame entière, pour la relayer telle quelle |
| `ids` | identifiants des paquets par état et par sens, noms pour les journaux |
| `packets` | paquets typés (trait `Packet`) : handshake, statut, login, et en jeu ce qu'un client doit lire ou écrire |
| `fml` | VarShort de `S3F`, varint et chaînes de Forge, messages `FML\|HS` (ModIdData gardé brut), charge de `REGISTER`, machine d'états `ClientHandshake` d'un client Forge |
| `json`, `status` | lecteur JSON maison, réponse de statut (`modinfo.modList`), texte d'un composant de chat |

### `gamma-bots` (`bots/`, exécutable)

| Fichier | Contenu |
|---|---|
| `main.rs` | aiguillage entre les bots seuls et les sous-commandes `run` et `compare` ; exécution des bots seuls |
| `fleet.rs` | statut du serveur, arrivée étalée des bots, progression, arrêt et collecte des mesures, communs aux deux usages |
| `bot.rs` | connexion, machine d'états, mesures ; deux threads bloquants par bot (lecture, cadence de 50 ms) |
| `behaviour.rs` | déplacements des comportements `idle`, `wander`, `explore` |
| `report.rs` | CSV et résumé |
| `args.rs`, `signal.rs` | ligne de commande, Ctrl-C |
| `orchestrate.rs` | `run` : préparation du serveur, répétitions, collecte, `run.json` |
| `server.rs` | processus du serveur : console sur des tubes, attente d'une ligne, commandes, arrêt ou mise à mort ; arguments JVM |
| `scenario.rs`, `toml.rs` | fichiers de scénario et lecteur du sous-ensemble de TOML qu'ils utilisent |
| `compare.rs` | `compare` : lecture des résultats, médiane et étendue des répétitions, tableau Markdown |
| `sha256.rs`, `util.rs` | empreintes du jar et du monde ; dates UTC, JSON, CSV, copies de dossiers |

Aucune dépendance externe : la bibliothèque standard suffit. Le Ctrl-C passe par
`SetConsoleCtrlHandler` (Windows) ou `signal` (Unix), déclarés directement ; de même, sous
Windows, l'objet job qui tue le serveur si l'orchestrateur meurt (`CreateJobObjectW`).

## Compiler

Rust 1.70 ou plus récent (essayé avec 1.97).

```sh
cd tools/bench
cargo build --release
cargo test
```

Le binaire est `target/release/gamma-bots` (`gamma-bots.exe` sous Windows).

## Lancer

```sh
gamma-bots --host 127.0.0.1 --port 25565 --count 20 --prefix bot --rate 5 \
           --behaviour wander --duration 120 --report bots.csv
```

| Option | Défaut | Effet |
|---|---|---|
| `--host`, `--port` | `127.0.0.1`, `25565` | adresse du serveur |
| `--count` | 1 | nombre de bots |
| `--prefix`, `--first` | `bot`, 1 | noms `<prefix><n>` à partir de `n = first`, 16 caractères au plus, `[A-Za-z0-9_]` |
| `--rate` | 5 | connexions par seconde, arrivée étalée ; 0 = toutes d'un coup |
| `--behaviour` | `idle` | `idle`, `wander` ou `explore` |
| `--duration` | 60 | durée en secondes, comptée depuis la première connexion (arrivée comprise) |
| `--report` | aucun | fichier CSV, une ligne par bot |
| `--seed` | 1 | graine des trajets d'errance et des directions d'exploration |
| `--wander-radius` | 16 | rayon d'errance autour du point d'apparition, en blocs |
| `--wander-height` | 0 | hauteur d'errance au-dessus du point d'apparition, en blocs |
| `--fly-height` | 100 | hauteur de vol des explorateurs au-dessus du point d'apparition du monde |
| `--join-timeout` | 30 | une connexion qui n'a pas reçu `S01 JoinGame` au bout de ce délai est abandonnée |
| `--attempts` | 3 | connexions qu'un bot peut ouvrir avant d'être en jeu |
| `--trace` | 0 | affiche sur la sortie d'erreur les n premiers paquets reçus par le premier bot |
| `--wait` | 0 | réessaie le statut pendant ce délai si le serveur ne répond pas encore |

Une exécution lit d'abord le statut du serveur (liste de mods, protocole), lance les bots au
rythme demandé, affiche une ligne de progression toutes les 10 s, arrête tout à la fin de la
durée ou sur Ctrl-C, puis écrit le CSV et le résumé. Code de sortie : 0 si chaque bot était en
jeu à la fin, 1 sinon, 2 pour une erreur d'usage ou un serveur injoignable.

Le serveur doit être en `online-mode=false`, avec `allow-flight=true` (les explorateurs volent,
les errants peuvent flotter) et assez de places (`max-players`). Les autres réglages conseillés
sont dans le cahier des charges ; `127.0.0.1` échappe au throttle de connexion.

## Ce que fait un bot

1. Handshake (protocole 5, état 2) et `LoginStart`, Nagle coupé.
2. `LoginSuccess` : passage en état de jeu.
3. Poignée de main `FML|HS` comme un client Forge 1.7.10 : sur ServerHello, `REGISTER`
   (`FML|HS`, `FML` et les canaux annoncés par le `REGISTER` du serveur), ClientHello (2) et
   ModList, recopie de la `modList` du statut ; Ack(2) sur la ModList du serveur, Ack(3) sur
   ModIdData (sautée sans être décodée), Ack(4) et Ack(5) sur les Ack(2) et Ack(3) du serveur.
4. `S01 JoinGame` : envoi de `C15` (`en_US`, comme un client par défaut).
5. Chaque `S08` est confirmé tout de suite par un `C06` aux x et z identiques au bit près, pieds =
   y reçu − 1,6200000047683716, stance = y reçu, avant tout déplacement. Le deuxième paquet de
   déplacement qui suit répète la position du premier (voir les écarts).
6. Keep-alive renvoyé aussitôt ; vie à 0 : `C16 0` (renvoyé toutes les 5 s sans réponse) ;
   `S40` ou déconnexion pendant le login : la raison est notée et le bot s'arrête.
7. Tous les autres paquets sont sautés par leur longueur sans être gardés ; les chunks (`0x21`,
   `0x26`) sont comptés, jamais décompressés.
8. Toutes les 50 ms : `C03` (immobile) ou `C04`.
9. À l'arrêt : demi-fermeture (FIN), lecture jusqu'à ce que le serveur ferme (3 s au plus) ; le
   serveur journalise un simple « Disconnected ».
10. Une connexion qui n'a pas reçu `S01` après `--join-timeout` est fermée et le bot se reconnecte,
    jusqu'à `--attempts` connexions.

## Comportements

- `idle` : `C03` au sol toutes les 50 ms. Le serveur ne fait vivre l'entité d'un joueur qu'à la
  réception de ses paquets de mouvement : un bot immobile doit quand même en envoyer.
- `wander` : marche à 4,317 blocs/s vers des points tirés au hasard (uniformément dans le disque
  de rayon `--wander-radius` autour du point où le bot est apparu), à y constant (y d'apparition
  plus `--wander-height`), avec des pauses de 0 à 2 s.
- `explore` : monte d'un bloc par paquet jusqu'à y = y du point d'apparition du monde (`S05`)
  plus `--fly-height`, soit 164 sur le monde de test, 250 au plus, puis avance en ligne droite à
  5,6 blocs/s. Chaque bot a sa direction ; l'angle d'or les répartit également quel que soit leur
  nombre.

Un paquet ne déplace jamais de plus d'un bloc (9 au plus en garde-fou ; le serveur refuse au-delà
de 10). Le bot ne voit pas le terrain : quand le serveur refuse un déplacement (un `S08` proche
de la position, appelé ici recul), le bot reste immobile 0,5 s (durée doublée à chaque recul
rapproché, 30 s au plus, remise à zéro après 5 s sans recul), puis :

- s'il était à sa hauteur de croisière, il la relève de 2 blocs (errance) ou de 8 (exploration) ;
- si c'est sa montée qui a été bloquée (feuillage au-dessus de lui), il part 2 s sur le côté à
  cette hauteur, dans une nouvelle direction au hasard, puis remonte.

Sur un terrain accidenté ou boisé, une errance commencée au sol finit donc quelques blocs plus
haut, au-dessus des obstacles rencontrés.

## Le CSV

Une ligne par bot, écrite à la fin.

| Colonne | Contenu |
|---|---|
| `name` | nom du bot |
| `connected` | `yes` si le bot a reçu `S01 JoinGame` |
| `attempts` | connexions ouvertes (1 sauf reconnexion) |
| `join_ms` | de l'ouverture TCP (de la dernière connexion) à `S01 JoinGame` |
| `first_chunk_ms` | de `S01` au premier paquet de chunks |
| `in_game_s` | temps passé en jeu |
| `connected_at_end` | `yes` si le bot était encore en jeu quand le banc s'est arrêté |
| `kick_reason` | texte de `S40` ou de la déconnexion du login, codes `§` retirés |
| `error` | erreur réseau ou de protocole, ou étape atteinte par un bot jamais entré en jeu |
| `first_failure` | raison de l'échec de la première connexion quand il a fallu se reconnecter |
| `keepalives` | keep-alive reçus (tous renvoyés) |
| `keepalive_interval_ms` | intervalle moyen entre deux keep-alive : 40 ticks, 2000 ms à 20 TPS ; au-delà, le serveur prend du retard |
| `keepalive_interval_max_ms` | plus long intervalle : le pire ralentissement vu par ce bot |
| `bytes_received` | octets reçus |
| `chunk_packets` | paquets `0x21` et `0x26` reçus |
| `s08_received` | `S08` reçus (apparition, reculs, téléportations, réapparitions) |
| `setbacks` | reculs : `S08` qui ont annulé un déplacement du bot |
| `deaths` | morts (réapparition demandée) |
| `x`, `y`, `z` | dernière position, y aux pieds |

La sortie standard donne un résumé : bots lancés, entrés en jeu et présents à la fin, expulsions
et erreurs groupées par raison, temps de connexion (min, p50, p95, max), délai du premier chunk,
keep-alive, octets et chunks par seconde, `S08` et reculs.

## Limites connues

- Pas de RTT des keep-alive : le serveur ne répond pas au `C00` et ne publie plus le ping des
  joueurs (voir les écarts). Le bot donne l'intervalle entre keep-alive, qui montre le retard du
  serveur mais pas le réseau.
- Le terrain est invisible : sur le monde de test, boisé autour du point d'apparition, 20 errants
  partis du sol font environ 10 reculs chacun la première minute, sans aucun avertissement du
  serveur ; `--wander-height 16` n'y change presque rien, car la plupart viennent de la montée
  initiale sous les arbres. De 5 à 8 % des bots nés dans un trou ou sous un arbre bas peuvent
  rester coincés près du point d'apparition : ils restent immobiles entre des essais de plus en
  plus espacés, et la colonne `setbacks` les signale.
- Un pas d'exploration fait 0,28 bloc : s'il heurtait un relief à la hauteur de vol, le serveur
  écrirait un « moved wrongly » avant que le bot ne remonte. Jamais vu à y = 164.
- Les positions persistent : un bot retrouve son fichier joueur (`world/playerdata`) et réapparaît
  là où il s'était arrêté, par exemple dans le ciel après une exploration. Pour des mesures
  reproductibles, partir d'un monde neuf ou changer de préfixe ; `gamma-bots run` repart
  d'un monde neuf à chaque répétition.
- Deux threads par bot : 1000 threads pour 500 bots, piles de 256 Kio.
- Ni chat ni commandes (comportement « Commandes » à venir) ; la dispersion se fait depuis la
  console (`spreadplayers`, `tp`), le bot confirme les téléportations.
- La ModList du serveur n'est pas vérifiée par le bot, et ModIdData n'est pas décodée.
- Protocole 5 et mode hors ligne seulement : pas de chiffrement.

## Écarts constatés avec docs/phase-0-bots.md

Constatés le 8 octobre 2026 contre le serveur de test (`test-server/`, sans mod ajouté).

1. **Ordre des paquets (étape 18 et « points à vérifier »).** Observé avec `--trace` :
   `S02 LoginSuccess`, `S3F REGISTER`, `FML|HS` ServerHello, ModList, ModIdData (11,6 Ko),
   Ack(2), `S3F FORGE`, Ack(3), puis `S01 JoinGame`, `S3F MC|Brand`, `S05`, `S39`, `S09`, `S37`
   (statistiques), `S02` (message d'arrivée), `S38` ×2, `S08`, `S03`, `S30`, `S2F` ×2, `S06`,
   `S1F`, puis les `S26`. Le cahier n'a ni `S37`, ni le chat, ni `S30`/`S2F`/`S06`/`S1F`, et met
   `S2B` (absent ici : il ne pleuvait pas) avant les chunks. Un message `FORGE` arrive entre les
   deux Ack du serveur.
2. **Thread de l'entrée en jeu (« points à vérifier »).** `initializeConnectionToPlayer` tourne
   sur le thread principal : le journal écrit `[Server thread] Server side modded connection
   established`, et non un thread Netty comme le déduisait la carte du réseau.
   `NetworkDispatcher.handleServerSideCustomPacket` repasse le `CompleteHandshake` par
   `context.fireChannelRead`, donc par la file du `NetworkManager` vidée par le thread principal.
3. **Étape 19 (C15).** La distance de vue envoyée n'est pas utilisée :
   `EntityPlayerMP.func_147100_a` calcule `256 >> distance` et n'en fait rien ; seule compte la
   `view-distance` du serveur.
4. **Étape 20, hauteur des pieds.** Le serveur ajoute `1.6200000047683716D` (le flottant 1,62
   élargi), pas 1,62 : le bot retranche cette valeur exacte pour retrouver la hauteur du serveur
   au bit près.
5. **Étapes 20 et 22, paquet ignoré après chaque `S08`.** Après une téléportation, CraftBukkit
   laisse `justTeleported` levé (`NetHandlerPlayServer.processPlayer`, bloc du
   `PlayerMoveEvent`). Le deuxième paquet de déplacement qui atteint ce bloc (déplacement de plus
   de √(2/256) bloc) trouve la position mémorisée en retard d'un pas : il est ignoré sans réponse
   et le drapeau tombe. Le suivant est alors mesuré depuis deux pas en arrière et, s'il touche un
   bloc, l'écart double déclenche « moved wrongly ». Le bot répète donc sa position dans ce
   deuxième paquet. Au premier essai, 20 errants au sol produisaient 155 « moved wrongly » en une
   minute ; avec la gestion des reculs mais sans ce correctif, encore 2 à 4 ; avec, aucun sur tous
   les essais suivants.
6. **Tableau des obstacles, « moved wrongly » et reculs.** Le contrôle ne mesure que l'écart
   horizontal (`if (d5 > -0.5D || d5 < 0.5D) d5 = 0.0D;` annule toujours l'écart vertical) : un
   pas de moins de 0,25 bloc ne peut pas le déclencher à lui seul. En revanche, un déplacement qui
   finit dans un bloc est annulé par un `S08` sans aucun message (`flag && !flag2`) : ces reculs
   silencieux n'apparaissent pas dans le journal, seulement dans la colonne `setbacks`.
7. **Étape 21 et mesure 7, RTT du keep-alive.** Non mesurable côté bot : le serveur ne répond pas
   au `C00`, et CraftBukkit a mis en commentaire la diffusion périodique du ping des joueurs dans
   `ServerConfigurationManager.sendPlayerInfoToAllPlayers` ; le seul `S38` du bot porte 1000 ms à
   l'entrée.
8. **Section C et machine d'états, Ack.** Le vrai client (`FMLHandshakeClientState`) envoie Ack(3)
   à la réception de ModIdData, puis Ack(4) et Ack(5) sur les Ack(2) et Ack(3) du serveur, et non
   Ack(3) sur l'Ack(2) du serveur. Le serveur accepte les deux ; le bot fait comme le vrai client.
9. **Étape 10, `REGISTER`.** Le vrai client enregistre `FML|HS`, `FML` et ses propres canaux ; le
   bot y ajoute les canaux du `REGISTER` du serveur (même liste de mods, mêmes canaux).
10. **Statut d'un serveur « sans mods ».** La `modList` n'est pas vide : `mcp`, `Crucible`, `FML`,
    `Forge` et `kimagine` y figurent, et doivent être renvoyés.
11. **Point d'apparition.** Les nouveaux joueurs n'apparaissent pas au point du monde mais à une
    dizaine de blocs autour (`WorldProvider.getRandomizedSpawnPoint`), sur le premier bloc solide
    qui n'est pas du feuillage (`World.getTopSolidOrLiquidBlock` saute `Material.leaves`), donc
    souvent sous un arbre : une montée verticale y est bloquée, d'où la sortie sur le côté décrite
    plus haut.

## Observations sur le serveur

Constatées pendant les essais, sans lien avec un défaut des bots.

> **Corrigé le 8 octobre 2026 (commit `92110591`)** : les trois points ci-dessous avaient une seule
> cause, le canal `FML` partagé par toutes les connexions. La fin de poignée de main est maintenant
> rattachée à sa connexion (`io.github.gammaengine.network.HandshakeCompletions`). Avec
> `--attempts 1` : 60 bots à 10 connexions/s sur 8 séries passent de 22 entrées perdues sur 480 à 0,
> 50 bots d'un coup de 49 perdues à 0, sans plus aucune CME ni alerte de fuite. Le texte d'origine
> reste ci-dessous pour mémoire.

- **Entrée en jeu perdue lors de connexions rapprochées.** 2 bots sur 50 à 5 connexions par
  seconde, 4 sur 60 à 10 par seconde, ont terminé la poignée de main (Ack(3) reçu) sans jamais
  recevoir `S01` ; le serveur n'écrit pas « logged in ». Lecture du code :
  `FMLNetworkHandler.forwardHandshake` range le `NetworkDispatcher` du joueur dans un attribut du
  canal `FML` côté serveur, un `EmbeddedChannel` unique pour toutes les connexions, et
  `HandshakeCompletionHandler` le reprend par `getAndRemove` quand le thread principal traite le
  paquet. Deux connexions dont les poignées de main se terminent avant ce traitement se marchent
  dessus : le premier paquet termine la connexion du second joueur, le second trouve l'attribut
  vide (garde de Cauldron contre `null`) et le premier joueur n'entre jamais en jeu ; un vrai
  client attendrait. Les bots se reconnectent (`--attempts`) et le signalent dans
  `first_failure` et le résumé.
- **`ConcurrentModificationException`.** Une connexion sur 60 à 10 par seconde a été coupée
  pendant l'entrée en jeu par `Internal Exception: java.util.ConcurrentModificationException`.
- **« Detected ongoing potential memory leak … FML : N ».** Le compteur augmente à chaque entrée
  en jeu, de 5 à 12 paquets selon le nombre de joueurs déjà présents. Les bots n'envoient rien sur
  le canal `FML`.
