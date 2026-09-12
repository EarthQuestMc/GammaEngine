# Moteur natif (Rust)

Le moteur natif est une bibliothèque Rust **optionnelle** qui accélère quelques opérations
gourmandes en CPU et traitables par lots. Le serveur démarre, tourne et produit des mondes
identiques sans elle. Quand `libgammaengine_native.so` (ou `gammaengine_native.dll`) est absente,
échoue au chargement ou annonce une version d'ABI différente, le moteur écrit une ligne de log et
utilise les implémentations Java.

## Ce qu'il accélère, et pourquoi ces choses-là

Rust est employé là où trois conditions sont réunies : le travail est limité par le CPU, il arrive
par blocs assez gros pour amortir une transition JNI, et il ne touche aucun état mutable du monde.
Cela exclut la boucle de tick, les entités, les TileEntities, le bus d'événements Forge et Bukkit,
et cela inclut :

| Opération | Où elle sert | État |
| --- | --- | --- |
| Compression zlib | Écriture d'un chunk dans un fichier region | Implémenté |
| Décompression zlib | Lecture d'un chunk depuis un fichier region | Implémenté |
| XXH64 | Hachage des snapshots de chunks, intégrité et déterminisme du monde | Implémenté |
| Arithmétique et validation des secteurs de fichier region | Réparation et allocation de fichiers region | Implémenté (fonctions pures) |
| Encodage et décodage binaire NBT | Pipeline de sauvegarde et chargement de chunks | Prévu |
| Pathfinding sur snapshots immuables | IA des mobs, hors du thread de région | Prévu, phase 10 |
| Collisions et recherches spatiales par lots | Déplacement des entités | Prévu, phase 10 |

## Résultats mesurés

Charge de référence : 200 000 octets de données en forme de chunk (longues suites de blocs
identiques entrecoupées de zones bruitées). Médiane de 50 exécutions après 200 itérations de
chauffe, transition JNI et copies de tableaux comprises. Machine : poste de développement, pas la
cible EPYC ; les chiffres valent pour le rapport, pas pour la valeur absolue.

| Opération | Java | Rust | Écart |
| --- | --- | --- | --- |
| Compression zlib niveau 6 | 195 Mio/s | 265 Mio/s | +36 % |
| Décompression zlib | 1010 Mio/s | 1544 Mio/s | +53 % |
| XXH64 | 4296 Mio/s | 7509 Mio/s | +75 % |
| Taille compressée | 19 268 o | 18 806 o | −2,4 % |

Le même banc mesure aussi le coût du niveau de compression, qui commande le budget de streaming de
chunks analysé dans [scaling.md](scaling.md) :

| Niveau deflate | Débit Java | Taille de sortie |
| --- | --- | --- |
| 1 | 462 Mio/s | 19 989 o |
| 4 (paquets de chunks) | 272 Mio/s | 19 283 o |
| 6 (fichiers region) | 196 Mio/s | 19 268 o |

Pour reproduire :

```bash
cd native && cargo build --release && cd ..
./gradlew :eclipse:cauldron:test --tests '*NativeEngineTest*'
# résultats dans eclipse/cauldron/build/test-results/test/TEST-*NativeEngineTest.xml
```

Une future accélération qui ne bat pas Java sur ce banc n'est pas intégrée.

## Règles de sûreté

* **Aucun panic ne franchit la frontière.** Chaque fonction exportée enveloppe son corps dans
  `catch_unwind` et renvoie un tableau nul (ou 0) en cas d'échec. Le côté Java interprète cela comme
  « utiliser Java », le compte, et continue.
* **Pas de petits appels.** Les tampons de moins de 4 Kio restent en Java : la transition et les deux
  copies de tableau coûtent plus cher que ce qu'elles économisent. Le seuil est
  `NativeEngine.MIN_NATIVE_BYTES`.
* **Une seule façade.** `NativeBindings` contient toutes les déclarations `native` et n'est pas
  publique ; `NativeEngine` est le seul appelant et détient le repli, les métriques et le contrôle
  d'ABI.
* **Versionnage d'ABI.** `ABI_VERSION` existe dans `native/src/lib.rs` et dans `NativeEngine`. Un
  écart désactive l'accélération native avec un avertissement, au lieu d'échouer mystérieusement
  plus tard.
* **Mêmes résultats, pas mêmes octets.** Le `Deflater` de Java et l'encodeur Rust produisent des flux
  zlib différents mais également valides. Les fichiers region restent lisibles par le client
  vanilla, par Forge et par les outils externes puisque les deux flux sont du zlib valide ; ce que
  les tests garantissent, c'est que chaque côté lit la sortie de l'autre et que le contenu
  décompressé et son hash sont identiques. Les hashes de monde sont donc toujours calculés sur le
  contenu décompressé, jamais sur les octets compressés.

## Compilation

```bash
cd native
cargo test            # 17 tests unitaires, dont un recoupement XXH64 avec une implémentation indépendante
cargo build --release # target/release/libgammaengine_native.so (ou .dll)
```

Installer la bibliothèque là où le serveur la trouvera, par ordre de priorité :

1. `-Dgammaengine.nativeLibrary=/chemin/absolu/vers/libgammaengine_native.so`
2. n'importe où sur `java.library.path`
3. `gammaengine/native/libgammaengine_native.so` à côté du jar du serveur

## Métriques

`/autothread native` indique si la bibliothèque est chargée et d'où. Le profileur enregistre
`native.compress`, `native.decompress`, `native.hash`, leurs équivalents Java sous `native.*.java`,
et les compteurs `native.jni.calls`, `native.jni.bytes` et `native.fallback.*`. Un compteur de repli
qui monte signifie que le chemin natif refuse du travail : cela s'analyse, cela ne s'ignore pas.
