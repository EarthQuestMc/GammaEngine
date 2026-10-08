# Carte du code — chargement des mods et des plugins

Conventions : `E/` = `eclipse/cauldron/src/main/java/` (sources patchées, hors git) ; `S/` = `src/main/java/` ; `P/` = `patches/`.
JVM du dépôt de test : Java HotSpot 1.8.0_491, 5 « mods » (aucun mod utilisateur : `run/mods` vide, `run/plugins` ne contient que `PluginMetrics` et `bStats`) — source : `run/boot5.log:11,50`. Aucune mesure sur de vrais mods : tout ce qui suit sur le coût est lu dans le code, pas chronométré.

## Vue d'ensemble (du `java -jar` au premier tick)

| # | Étape | Thread | Classloader | Preuve |
|---|-------|--------|-------------|--------|
| 1 | `java -jar server.jar` : le manifeste donne `Main-Class: cpw.mods.fml.relauncher.ServerLaunchWrapper`, `TweakClass: …FMLTweaker` et un `Class-Path` généré = tous les jars de `libraries/` ; aucun `Add-Opens`, aucun `Launcher-Agent` | `main` | chargeur d'application (système) | `build.gradle:165-175`, `build.gradle:267-274` |
| 2 | `ServerLaunchWrapper.main` appelle d'abord `CrucibleServerMainHook.relaunchMain(args)`. Malgré son nom, **aucun processus n'est relancé** (pas de `ProcessBuilder` dans `bootstrap/`) : bannière, `inject.properties`, `checkJava`, vérification des bibliothèques | `main` | système | `E/cpw/mods/fml/relauncher/ServerLaunchWrapper.java:14` ; `S/io/github/crucible/bootstrap/CrucibleServerMainHook.java:30-75` |
| 2a | `inject.properties` (créé s'il manque à partir de la ressource, vide par défaut) : chaque entrée devient une propriété système | `main` | système | `CrucibleServerMainHook.java:38-58` ; `src/main/resources/inject.properties` = 0 octet |
| 2b | `Lwjgl3ifyGlue.checkJava` : messages, refus si Java 17 < 17.0.6 (`lwjgl3ify.skipjavacheck` le coupe) | `main` | système | `S/io/github/crucible/bootstrap/Lwjgl3ifyGlue.java:59-103` |
| 2c | `verifyLibraries` → `LibraryManager.checkIntegrity` : MD5 de chaque jar de `libraries/` comparé au `.md5` voisin, en parallèle (pool de `physicalCores` threads `GammaEngine-LibraryCheck`) ; si échec : `setupLibraries` télécharge puis `System.exit(0)` (« redémarrage requis ») | `main` + pool | système | `CrucibleServerMainHook.java:65-72,83-111` ; `S/io/github/crucible/bootstrap/LibraryManager.java:174-250` |
| 3 | `ServerLaunchWrapper.run` charge par réflexion `net.minecraft.launchwrapper.Launch` (jar externe `io.github.cruciblemc:launchwrapper:1.13`, `run/libraries/io/github/cruciblemc/launchwrapper/1.13/`) et appelle `main` avec `--tweakClass …FMLServerTweaker` | `main` | système | `ServerLaunchWrapper.java:28,41-46` |
| 4 | `Launch.main` : construit `LaunchClassLoader` avec les URL tirées de la propriété `java.class.path` (sous `java -jar` : le jar serveur seul), instancie les tweakers (`FMLTweaker()` installe `FMLSecurityManager` ; `FMLServerTweaker`) | `main` | `LaunchClassLoader` créé ; `Launch.classLoader` | `javap` de `Launch.getURLs` (jar launchwrapper-1.13) ; `E/cpw/mods/fml/common/launcher/FMLTweaker.java:30-43` |
| 5 | `FMLServerTweaker.injectIntoClassLoader` : exclusions, `FMLLaunchHandler.configureForServerLaunch` → `setupHome` → `CoreModManager.handleLaunch` | `main` | `LaunchClassLoader` | `E/cpw/mods/fml/common/launcher/FMLServerTweaker.java:14-25` ; `E/cpw/mods/fml/relauncher/FMLLaunchHandler.java:37,70-77,111-113` |
| 5a | `handleLaunch` : enregistre `PatchingTransformer`, charge les 3 plugins racine (FML, Forge, `pw.prok.imagine.ImagineLoadingPlugin`), puis **`CrucibleCoremodHook.coremodHandleLaunch`** (config Crucible + lwjgl3ify : 2 transformers), puis `discoverCoreMods` (jars de `mods/` ayant `FMLCorePlugin`/`TweakClass`) | `main` | `LaunchClassLoader` | `E/cpw/mods/fml/relauncher/CoreModManager.java:68,175,201,212,222,236,252` |
| 5b | Tweakers en cascade : `FMLInjectionAndSortingTweaker` (trie par `sortIndex`), `FMLDeobfTweaker`, `TerminalTweaker`; chaque `FMLPluginWrapper.injectIntoClassLoader` enregistre les transformers du coremod (log « Injecting coremod … ») | `main` | `LaunchClassLoader` | `E/cpw/mods/fml/relauncher/CoreModManager.java:108-120,644-650,659-720` ; `E/cpw/mods/fml/common/launcher/FMLInjectionAndSortingTweaker.java:21-39` ; `run/boot5.log:21-25` |
| 6 | `LaunchClassLoader` charge `net.minecraft.server.MinecraftServer` (`getLaunchTarget`) et appelle `main` par réflexion | `main` → `Server thread` | `LaunchClassLoader` | `FMLServerTweaker.java:8-11` ; `run/boot5.log:39` (le journal passe de `main` à `Server thread`) |
| 7 | `DedicatedServer.startServer` : `FMLCommonHandler.onServerStart` → `FMLServerHandler.beginServerLoading` → **`Loader.loadMods()`** (découverte des mods, tri, états CONSTRUCTING) puis `preinitializeMods()` (PREINIT) | `Server thread` | `LaunchClassLoader` (`ModClassLoader` n'est qu'un délégué) | `E/net/minecraft/server/dedicated/DedicatedServer.java:206` ; `E/cpw/mods/fml/server/FMLServerHandler.java:84-89` ; `E/cpw/mods/fml/common/Loader.java:482-548,549` |
| 8 | `new DedicatedPlayerList` → `new CraftServer` (`ServerConfigurationManager` ligne 132) ; `SpigotConfig.init`, `registerCommands` ; `PreYggdrasilConverter`, `AnvilSaveConverter` | `Server thread` | `LaunchClassLoader` | `E/net/minecraft/server/management/ServerConfigurationManager.java:132` ; `DedicatedServer.java:263-267,312,320` |
| 9 | `handleServerAboutToStart`, puis `loadAllWorlds`. Au premier `SaveHandler` de la dimension 0 : `injectBlockBukkitMaterials`, `injectItemBukkitMaterials`, puis **`CraftServer.loadPlugins()` et `enablePlugins(STARTUP)`** (les plugins se chargent donc après le gel des registres, dans le constructeur du monde) | `Server thread` | `PluginClassLoader` (un par plugin) | `DedicatedServer.java:365-367` ; `E/net/minecraft/world/storage/SaveHandler.java:455-465` |
| 10 | Fin de chargement des mondes : `initializeMods` (INIT puis POSTINIT, `GameData.freezeData`), `initialWorldChunkLoad` (préchargement parallèle du spawn par `ChunkPrefetcher`), `enablePlugins(POSTWORLD)` (commandes vanille, permissions, `helpMap.initializeCommands`) | `Server thread` | — | `Loader.java:735,748` ; `FMLCommonHandler.java:323` ; `E/net/minecraft/server/MinecraftServer.java:450,472,520` ; `CraftServer.java:328-357` |
| 11 | `Done (n s)`, puis `onServerStarted` (AutoThread), boucle de ticks | `Server thread` | — | `DedicatedServer.java:367-370,398` ; `run/boot5.log:113` (`Done (0,974s)`, test sans mod) ; `S/io/github/gammaengine/autothread/AutoThreadRuntime.java:70` |

Point structurant : **les mods et le moteur partagent un seul classloader** (`LaunchClassLoader`), car `ModClassLoader.addFile` fait `mainClassLoader.addURL` (`E/cpw/mods/fml/common/ModClassLoader.java:45-48`). Les plugins Bukkit ont chacun leur `PluginClassLoader`.

## Classes et méthodes clés

| Élément | `chemin:ligne` | Rôle | Patch Crucible |
|---|---|---|---|
| `ServerLaunchWrapper.main/run` | `E/cpw/mods/fml/relauncher/ServerLaunchWrapper.java:12-54` | point d'entrée, délègue à LaunchWrapper | oui : `P/cpw/mods/fml/relauncher/ServerLaunchWrapper.java.patch` (appel `relaunchMain`) |
| `CrucibleServerMainHook.relaunchMain/verifyLibraries/setupLibraries` | `S/io/github/crucible/bootstrap/CrucibleServerMainHook.java:30,83,94` | pré-lancement | ajout Crucible, instrumenté par GammaEngine (durée de vérification, ligne 62-66) |
| `LibraryManager.checkIntegrity/checkOne/digestOf` | `S/io/github/crucible/bootstrap/LibraryManager.java:174,230,253` | MD5 des libs, parallèle (GammaEngine) | Crucible |
| `CrucibleMetadata` (statique) | `S/io/github/crucible/bootstrap/CrucibleMetadata.java:21-58` | lit le manifeste (`Forge-Version`, `GammaEngine-Libs`) ; lève si build Forge = 0 | Crucible |
| `FMLTweaker()` / `FMLSecurityManager` | `E/cpw/mods/fml/common/launcher/FMLTweaker.java:30-43` ; `P/cpw/mods/fml/relauncher/FMLSecurityManager.java.patch` | installe un `SecurityManager` (autorisé par `-Djava.security.manager=allow`, `java9args.txt:2`) ; le patch ajoute `defman` | oui |
| `CoreModManager.handleLaunch` | `E/cpw/mods/fml/relauncher/CoreModManager.java:175-250` | coremods, tweakers, hook Crucible ligne 222 | oui (`P/cpw/mods/fml/relauncher/CoreModManager.java.patch`) |
| `CoreModManager.addUrlToClassloader` | `CoreModManager.java:431-461` | ajoute une URL au chargeur système : `URLClassLoader.addURL` ou, hors `URLClassLoader` (Java 9+), champ `ucp` par réflexion | oui (lwjgl3ify) |
| Contournement FastCraft | `CoreModManager.java:366,399` ; `P/cpw/mods/fml/common/discovery/ModDiscoverer.java.patch` | ignore le coremod, le tweaker et le mod FastCraft sauf `-Dthermos.fastcraft.disable=false` | oui (Thermos) |
| `CrucibleCoremodHook.coremodHandleLaunch` | `S/io/github/crucible/bootstrap/CrucibleCoremodHook.java:7-14` | exclusion `io.github.crucible.bootstrap.` du chargeur, charge `CrucibleConfigs`, appelle lwjgl3ify | Crucible |
| `Lwjgl3ifyGlue.doCoremodWork` | `Lwjgl3ifyGlue.java:105-125` | exclusions (`com.sun`, `javax`, `jdk`, `org.w3c.dom`…), précharge `javax.script.ScriptEngineManager`, enregistre `LwjglRedirectTransformer` et `UnfinalizeObjectHoldersTransformer` | Crucible/lwjgl3ify |
| `LwjglRedirectTransformer.transform` | `S/me/eigenraven/lwjgl3ify/core/LwjglRedirectTransformer.java:25-52,54-60` | **pour chaque classe** : `ClassReader` + `ClassRemapper` + `ClassWriter(0)` ; remplace `org/lwjgl/`→`org/lwjglx/`, `javax/xml/bind/`→`jakarta/xml/bind/`, `javax/servlet/`→`jakarta/servlet/` ; saute les classes annotées `@Lwjgl3Aware` | lwjgl3ify |
| `UnfinalizeObjectHoldersTransformer` | `S/me/eigenraven/lwjgl3ify/core/UnfinalizeObjectHoldersTransformer.java:21-40` | lit chaque classe en `ClassNode` : retire `final` des champs `@ObjectHolder`, enums extensibles (liste `CrucibleConfigs.java:212`), `FixConstantPoolInterfaceMethodRefHelper` | lwjgl3ify |
| `ImagineASMClassTransformer` (jar KImagine 0.2.0, hors dépôt) | `run/libraries/pw/prok/KImagine/0.2.0/KImagine-0.2.0.jar` (`javap`) | transformer appelé sur toute classe ; exécute les classes `@Transformer.RegisterTransformer` trouvées par `FastDiscoverer` (classpath + `mods/`) | racine ajoutée au patch (`CoreModManager.java:68`) |
| `StreamsTransformer`, `RecurrentComplexTransformer`, `AsmHooks` | `S/io/github/crucible/patches/*.java` | correctifs ciblés de mods (Streams, Recurrent Complex) par bytecode au chargement, via KImagine ; **seul précédent de « correction de mod par transformer »** | Crucible |
| `Loader.identifyMods/loadMods/preinitializeMods/initializeMods` | `E/cpw/mods/fml/common/Loader.java:331,482,549,735` | découverte → tri → états | oui : `P/cpw/mods/fml/common/Loader.java.patch` (`CrucibleModContainer` injecté, ligne 335) |
| `ModDiscoverer.findClasspathMods/findModDirMods/identifyMods` | `E/cpw/mods/fml/common/discovery/ModDiscoverer.java` ; `Loader.java:352-366` | candidats = jar serveur, `mods/`, `mods/1.7.10` | oui (FastCraft) |
| `JarDiscoverer.discover` | `E/cpw/mods/fml/common/discovery/JarDiscoverer.java:41-95` | ouvre **chaque jar**, lit `mcmod.info`, passe **chaque `.class`** dans `ASMModParser` (annotations) → `ASMDataTable` ; séquentiel, sans cache | oui : saute `META-INF/versions`, nashorn, `jakarta/servlet/` (`P/…/JarDiscoverer.java.patch`) |
| `ASMDataTable` / `ASMData.getCandidate()` | `E/cpw/mods/fml/common/discovery/ASMDataTable.java:17-60` ; `ModCandidate.java:54-64` | annotation → classe → jar (`getModContainer()`) | non |
| `LaunchClassLoader` (jar externe) | `run/libraries/io/github/cruciblemc/launchwrapper/1.13/…jar` | `defineClass` avec `CodeSource(url du jar)`, `ConcurrentHashMap` pour `cachedClasses`/`resourceCache`, `tryComputeFrames` (si `-Dlegacy.computeFrames`) | fork Crucible (jar) |
| `ModClassLoader.addFile/loadClass` | `E/cpw/mods/fml/common/ModClassLoader.java:45-48,50-52` | délègue au `LaunchClassLoader` | non |
| `LoadController.transition/propogateStateMessage` | `E/cpw/mods/fml/common/LoadController.java:123,180` | machine à états, appelle chaque mod sur le thread serveur (`ThreadContext.put("mod")`, ligne 210) | non |
| `CraftServer.loadPlugins/enablePlugins/reload` | `S/org/bukkit/craftbukkit/v1_7_R4/CraftServer.java:307,328,823-825` | enregistre `JavaPluginLoader`, `pluginManager.loadPlugins(plugins/)`, `onLoad`, puis `onEnable` par `PluginLoadOrder` | oui |
| `JavaPluginLoader.loadPlugin/createRegisteredListeners` | `E/org/bukkit/plugin/java/JavaPluginLoader.java:76,262-345` | lit `plugin.yml`, crée `PluginClassLoader`; fabrique les exécuteurs | oui |
| `PluginClassLoader` (ctor, `findClass`, `remappedFindClass`, `getJarMapping`) | `E/org/bukkit/plugin/java/PluginClassLoader.java:51,83-196,279,383-432,434-481` | un chargeur par plugin ; remapping SpecialSource **par classe, au chargement** | oui (`P/org/bukkit/plugin/java/PluginClassLoader.java.patch`) |
| `ASMEventHandler` (bus Forge) | `E/cpw/mods/fml/common/eventhandler/ASMEventHandler.java:22-23,73-129,141` | génère une classe wrapper par méthode `@SubscribeEvent` (`ClassWriter(0)`, `ASMClassLoader` dédié, cache `HashMap<Method,Class>` non synchronisé) | oui (`P/…/ASMEventHandler.java.patch`) |

## Classloaders et transformers

**Chargeurs.**
- Système (`jdk.internal.loader.ClassLoaders$AppClassLoader` sur Java 9+) : jar serveur + toutes les bibliothèques par `Class-Path` (`build.gradle:173`).
- `LaunchClassLoader` (fork 1.13) : sources = `java.class.path` (= jar serveur sous `-jar`) puis les jars de mods via `addURL`. Exclusions par défaut (donc chargées par le parent) : `java.`, `sun.`, `org.lwjgl.`, `org.apache.logging.`, `net.minecraft.launchwrapper.`, `javax.`, `argo.`, `org.objectweb.asm.`, `com.google.common.`, `org.bouncycastle.` (`javap` du constructeur). Ajouts : `org.apache.`, `com.google.common.`, `LZMA.`, `com.mojang.` (`FMLTweaker.java:122-127`, `FMLServerTweaker.java:18-22`), `cpw.mods.fml.relauncher.`, `net.minecraftforge.classloading.` (`FMLLaunchHandler.java:57-58`), `io.github.crucible.bootstrap.` (`CrucibleCoremodHook.java:8`), et `com.sun`, `com.oracle`, `javax`, `jdk`, `org.omg`, `org.w3c.dom`, `org.xml.sax`, `org.hotswap.agent`, `org.lwjglx.debug` (`Lwjgl3ifyGlue.java:106-115`).
- Propriété d'une classe de mod : `ProtectionDomain.getCodeSource().getLocation()` = URL du jar (le chargeur définit chaque classe avec un `CodeSource`, `javap` : `new CodeSource`/`defineClass`). Les classes du moteur et de Minecraft ont pour source le jar serveur.
- `PluginClassLoader` (`public`, non plus `final`, patch Cauldron) : parent = chargeur de `JavaPluginLoader` (donc `LaunchClassLoader`), cache `ConcurrentMap` de classes, verrou `synchronized (name.intern())` (`PluginClassLoader.java:383-432`), `CodeSource` reconstruit (`PluginClassLoader.java:479`) pour les plugins qui lisent leur propre jar.

**Ordre d'enregistrement des `IClassTransformer`** (l'ordre d'appel est l'ordre de `registerTransformer`) :
1. `PatchingTransformer` (`CoreModManager.java:201`) ;
2. `LwjglRedirectTransformer`, `UnfinalizeObjectHoldersTransformer` (pendant `handleLaunch`, `CoreModManager.java:222` → `Lwjgl3ifyGlue.java:123-124`) ;
3. transformers des coremods dans l'ordre trié des tweakers : FML (`MarkerTransformer`, `SideTransformer`, `EventSubscriptionTransformer`, `FMLCorePlugin.java:23-25`), Forge (`FMLForgePlugin` → notamment `FluidIdTransformer`), KImagine (`ImagineASMClassTransformer`), puis les coremods de `mods/` ; `FMLDeobfTweaker` (poids 1000) ; `ModAPITransformer` ajouté plus tard par `ModClassLoader.addModAPITransformer` (lignes 89-95) ;
4. access transformers : classe nommée par chaque coremod (`CoreModManager.java:580`, liste `getAccessTransformers` ligne 723) ; le `AccessTransformer` de FML lit `*_at.cfg`.
Chaque classe chargée par le `LaunchClassLoader` traverse **toute** la liste (sauf préfixes en `addTransformerExclusion`), donc ajouter un transformer coûte une passe par classe de Minecraft et de tous les mods.

**Où brancher les transformers du fork.** Trois options, de la plus tôt à la plus tard :
- (a) `CrucibleCoremodHook.coremodHandleLaunch` / `Lwjgl3ifyGlue.doCoremodWork` : même mécanisme que lwjgl3ify, appelé à `CoreModManager.java:222`, **avant** les transformers FML/Forge et avant les coremods des mods : le transformer voit les classes dans leur état d'origine ;
- (b) `@Transformer.RegisterTransformer` (KImagine) : pratique, mais ne s'exécute qu'à la position de `ImagineASMClassTransformer` (après FML, avant les coremods de `mods/` selon l'ordre de tri) ;
- (c) un coremod `IFMLLoadingPlugin` interne ajouté à `rootPlugins` (`CoreModManager.java:68`) : ordre contrôlé par `sortIndex`/`@SortingIndex`.
La liste d'exclusions de transformer (`addTransformerExclusion`) évite de réécrire `io.github.gammaengine.*` (à ajouter : aujourd'hui seuls les préfixes FML sont exclus, `FMLServerTweaker.java:19-21`).

## Compatibilité Java moderne

| Sujet | État dans le dépôt | Preuve |
|---|---|---|
| `--add-opens` | uniquement dans un fichier de documentation `java9args.txt` à ajouter à la main : `jdk.internal.loader`, `java.net`, `java.nio`, `java.io`, `java.lang`, `java.lang.reflect`, `java.text`, `java.util`, `jdk.internal.reflect`, `sun.nio.ch`, `com.sun.jndi.dns`, `sun.awt`, `sun.awt.image`, `com.sun.imageio.plugins.png`, `jdk.dynalink.beans`, `javax.sql.rowset.serial` ; plus `--add-modules jdk.dynalink` et `java.sql.rowset`, `--illegal-access=warn`, `-Djava.security.manager=allow`, `-Dcrucible.weAreJava9=true` | `java9args.txt:1-34` ; `README.md:44` ; `docs/build.md:96` |
| Manifeste exécutable | pas de `Add-Opens` / `Add-Exports` (qui s'appliquent pourtant au jar lancé par `-jar`) | `build.gradle:158-175` |
| Cast du chargeur système | patché dans le moteur : `addUrlToClassloader` (voir tableau). Aucun transformer ne réécrit les `(URLClassLoader) getSystemClassLoader()` des mods ou plugins | `CoreModManager.java:431-461` ; recherche `URLClassLoader` dans `S/me/` et `S/io/github/crucible/patches/` : aucune réécriture |
| Réflexion sur les internes | `UnsafeHacks` (`sun.misc.Unsafe.theUnsafe`) pour les enums extensibles ; `ReflectionHelper` FML | `S/me/eigenraven/lwjgl3ify/UnsafeHacks.java:18-24` |
| Package `javax.xml.bind`, `javax.servlet` | réécrits par `LwjglRedirectTransformer` vers `jakarta.*` ; bibliothèques fournies : `jakarta.xml.bind-api 3.0.1`, `jaxb-impl 3.0.2`, **`javax.servlet-api 4.0.1`** (paquet `javax.servlet`, pas `jakarta.servlet`) | `LwjglRedirectTransformer.java:54-58` ; `build.gradle:92-94` |
| Nashorn / JAXB embarqués | `org.openjdk.nashorn:nashorn-core:15.4`, `jaxb-impl`, `jakarta.xml.bind-api`, `javax.persistence:persistence-api:1.0.2` ; `--add-modules jdk.dynalink` ; `JarDiscoverer` ignore leurs entrées | `build.gradle:92-95,129` ; `java9args.txt:25-27` ; `P/…/JarDiscoverer.java.patch` |
| ASM | `org.ow2.asm` 9.5 (asm, commons, tree, analysis, util) + `asm-deprecated 7.1` ; le launchwrapper embarque aussi `shaded/org/mutabilitydetector/asm` pour le calcul de frames | `build.gradle:86-91` ; contenu du jar launchwrapper |
| lwjgl3ify | `me.eigenraven.lwjgl3ify.*` (775 lignes) : remap des paquets, enums extensibles, dé-finalisation des `@ObjectHolder`, `UnsafeHacks` | `S/me/eigenraven/lwjgl3ify/` |
| Correctifs de mods par transformer | Streams, Recurrent Complex (voir plus haut) ; FastCraft désactivé | `S/io/github/crucible/patches/` |

## Points d'accroche pour l'analyse automatique des jars

| Besoin | Point d'accroche | Preuve |
|---|---|---|
| Lister les jars à analyser | `mods/`, `mods/1.7.10`, `-Dfml.coreMods.load`, `ModListHelper.additionalMods` pour les mods ; dossier `plugins/` (option `plugins` d'OptionSet) pour Bukkit | `Loader.java:353-363` ; `CoreModManager.java:254,324,494` ; `CraftServer.java:310-313` |
| Hash par jar, avant tout chargement | `CrucibleCoremodHook.coremodHandleLaunch` (avant `discoverCoreMods`) ; `XxHash64` est déjà dans le fork (`S/io/github/gammaengine/util/XxHash64.java`) | `CoreModManager.java:222` |
| Lire tous les `.class` d'un jar mod | `JarDiscoverer.discover` les parse déjà tous (`ASMModParser`) : on peut y brancher une seconde passe ou un cache | `JarDiscoverer.java:62-85` |
| TileEntities / entités / blocs enregistrés | `GameRegistry.registerTileEntity`, `EntityRegistry.registerModEntity`, `GameData` ; patches Crucible présents | `P/cpw/mods/fml/common/registry/{GameRegistry,EntityRegistry,GameData}.java.patch` |
| Handlers Forge | `EventBus.register` → `ASMEventHandler` (cible = objet, méthode `@SubscribeEvent`) ; donne la classe du mod, donc le jar | `E/cpw/mods/fml/common/eventhandler/ASMEventHandler.java:33-34` |
| Listeners Bukkit | `JavaPluginLoader.createRegisteredListeners` : `Listener`, `Method`, `Plugin` connus | `JavaPluginLoader.java:262-345` |
| `IWorldGenerator` | `GameRegistry.registerWorldGenerator` (patch présent) | `P/cpw/mods/fml/common/registry/GameRegistry.java.patch` |
| Propriétaire d'une classe | mod : `ModCandidate`/`ASMData.getCandidate().getModContainer()` (File) ; `ModContainer.getSource()` ; `getCodeSource()` ; plugin : `PluginClassLoader.getPlugin()` (Spigot) puis `getDescription()` | `ASMDataTable.java:34` ; `ModContainer.java:64` ; `PluginClassLoader.java:36` |
| Garde-fou de thread | **`AsyncCatcher` de Spigot : non trouvé.** Recherche `AsyncCatcher` / `async.catch` dans `src`, `patches`, `eclipse/cauldron/src`, `bukkit`, `forge` : 0 occurrence. Seul garde existant : `TimedEventExecutor` qui désactive le chronométrage hors thread principal, ce n'est pas un garde-fou | `S/co/aikar/timings/TimedEventExecutor.java:75` |

## Configuration existante

| Clé | Fichier | Défaut | Effet |
|---|---|---|---|
| (propriétés libres) | `inject.properties` (racine du serveur) | vide | injectées dans `System` avant tout (`CrucibleServerMainHook.java:49-58`) |
| `crucible.skipLibraryVerification` | propriété système | faux | saute le MD5 des libs (`CrucibleServerMainHook.java:84`) |
| `crucible.libraryRepos` | propriété système | vide | dépôts Maven supplémentaires séparés par espaces (`:100`) |
| `crucible.weAreJava9` | propriété système (posée dans `java9args.txt:4`) | faux | masque l'avertissement « arguments Java 9 absents » (`Lwjgl3ifyGlue.java:63`) |
| `lwjgl3ify.skipjavacheck` | propriété système | faux | saute le refus Java 17 < 17.0.6 (`:74`) |
| `crucible.i.know.what.i.am.doing.please.crash.the.server` | propriété système | faux | plante volontairement `serverMain` (`CrucibleServerMainHook.java:79`) |
| `thermos.fastcraft.disable` | propriété système | `true` | ignore FastCraft (`CoreModManager.java:366,399`) |
| `thermos.forgeRevision` | propriété système | `0` (puis `fmlversion.properties`) | numéro de build Forge (`CrucibleMetadata.java:38`) |
| `fml.coreMods.load` | propriété système | vide | coremods supplémentaires (`CoreModManager.java:226`) |
| `legacy.debugClassLoading`, `…Finer`, `…Save`, `legacy.computeFrames` | propriétés système (lues par le jar launchwrapper) | faux | traces / dump des classes transformées dans un dossier temporaire / recalcul des frames ASM |
| `plugin-settings.default.custom-class-loader` | `cauldron.yml` | `true` | `false` : plugin sans remapping (`PluginClassLoader.java:85,107`) |
| `plugin-settings.default.remap-nms-v1_7_R4` … `v1_5_R3`, `remap-obc-v1_7_R4` … `v1_5_R3` | `cauldron.yml` | `true` | quelles versions de NMS/OBC remapper (`:87-103`) |
| `plugin-settings.default.remap-nms-pre`, `remap-obc-pre`, `remap-allow-future` | `cauldron.yml` | `false` | NMS « non versionné » (`:92`) |
| `plugin-settings.default.global-inheritance`, `plugin-inheritance`, `remap-reflect-field`, `remap-reflect-class`, `remap-guava` | `cauldron.yml` | `true` | carte d'héritage globale ; remapping de la réflexion ; `com.google.common` → `guava10` (`:98-104`) |
| `plugin-settings.default.debug` | `cauldron.yml` | `false` | écrit chaque classe remappée dans `remapped-plugin-classes/` (`:461`) |
| `plugin-settings.<nom>.<clé>` | `cauldron.yml` | hérite de `default` | surcharge par plugin (`:109-125`) |
| `plugin-settings.allow-reload` | `cauldron.yml` | `false` (`run/cauldron.yml:14`) | autorise `/reload` |
| `lwjgl3ify_extensibleEnums` | `Gamma.yml` (migré depuis `Crucible.yml`) | liste `DEFAULT_EXTENSIBLE_ENUMS` (≈ 50 enums) | enums rendus extensibles (`CrucibleConfigs.java:212,215-244`) |
| `gamma.native.enabled`, `gamma.profiling.enabledAtStartup`, `gamma.network.chunkCompressionLevel` | `gammaengine.yml` (ancien nom `GammaAutoThread.yml`, renommé au démarrage) | `true`, `false`, `4` | chargement de la bibliothèque native, profilage dès le démarrage, niveau deflate des paquets de chunks (`S/io/github/gammaengine/config/GammaConfig.java`) |

## Pièges pour la suite

1. **`relaunchMain` ne relance rien** : tout argument JVM (GC, `--add-opens`, CDS) doit venir de la ligne de commande ou du manifeste ; il n'existe aucun processus lanceur dans le code. Un lanceur serait un ajout (`io.github.gammaengine`).
2. `java.class.path` sous `-jar` ne contient que le jar serveur : `Launch` ne voit pas les bibliothèques dans `LaunchClassLoader.getSources()` ; elles passent par les exclusions. Toute analyse qui énumère `getSources()` pour trouver des jars de libs ne les trouvera pas.
3. Les mods sont dans le **même** chargeur que Minecraft : on ne peut pas isoler ni décharger un mod ; « rétrogradation à chaud » ne peut qu'agir par état (bascule d'un drapeau lu par un transformer déjà inséré), pas par rechargement de classe.
4. Deux transformers lwjgl3ify s'exécutent sur toutes les classes (une lecture/écriture ASM complète + une lecture `ClassNode`) avant les transformers FML : c'est un coût de démarrage fixe, proportionnel au nombre de classes chargées, et ils ne cachent rien (`LwjglRedirectTransformer.java:25-52`). Le cache des classes transformées doit donc être posé **autour** de la chaîne entière, pas dans un seul transformer.
5. Le remapping des plugins est paresseux et par classe : le coût est étalé sur le chargement et sur les premières utilisations (classes chargées tard, au premier événement), y compris sur le thread serveur en cours de partie. `getGlobalInheritanceMap` n'est pas synchronisé (`JavaPluginLoader.java:410-440`).
6. `LwjglRedirectTransformer` réécrit `javax/servlet/` en `jakarta/servlet/` alors que seule `javax.servlet-api` est fournie (`build.gradle:93`) : un mod qui référence `javax.servlet` verrait une classe absente. Non testé ici.
7. `ASMEventHandler.cache` est un `HashMap` statique non synchronisé (`ASMEventHandler.java:23`) : tout enregistrement concurrent de handlers (parallélisation du chargement) exige de le protéger.
8. `JavaPluginLoader` enregistre les écouteurs par réflexion, enveloppés dans `TimedEventExecutor` qui crée **un `Timing` par méthode** à l'enregistrement (`Timings.ofSafe`, `TimedEventExecutor.java:69`), même si Timings est coupé.
9. Les plugins se chargent à la création du premier `SaveHandler` (`SaveHandler.java:462`) : tout code d'analyse de plugins doit tourner avant ce point, ou dans `CraftServer.loadPlugins` (`CraftServer.java:313`).
10. `FMLTweaker` installe un `SecurityManager` : sur Java 18+ il faut `-Djava.security.manager=allow`, et il est déprécié pour suppression ; cible Java 21 : vérifier que `System.setSecurityManager` reste utilisable (non testé, JVM de test = Java 8).
11. Aucune mesure ici sur de vrais mods : le temps de démarrage de `run/boot5.log` (0,974 s) vient d'un serveur sans mod.

## État des éléments de la feuille de route

| Élément | État | Preuve | Interrupteur |
|---|---|---|---|
| Hash par jar | absent (seul MD5 des **bibliothèques** moteur ; `XxHash64` existe mais n'est utilisé que par `NativeEngine`) | `LibraryManager.java:253-262` ; `grep XxHash64` : `NativeEngine.java`, `XxHash64.java` | — |
| Analyse de bytecode par classe | absent (aucun `ClassReader`/`ClassNode` dans `io.github.gammaengine` ; seul le scan d'annotations FML) | recherche dans `S/io/github/gammaengine/` : 0 | — |
| Classement série / parallèle | absent (aucune classification de mod ; `AutoThreadRuntime` ne porte que le cycle de vie et la mesure du tick) | `AutoThreadRuntime.java` | — |
| Garde-fou de thread | absent (`AsyncCatcher` non trouvé) | cf. tableau des accroches | — |
| Rétrogradation à chaud | absent | pas de bascule d'état par classe | — |
| Cache des décisions | absent | aucun fichier de cache dans `run/gammaengine` hors `native`, `reports` | — |
| Rapport au démarrage | partiel : durée de vérification des libs, rapport mémoire `/autothread memory`, bannière ; rien par mod ni par plugin | `CrucibleServerMainHook.java:62-73` ; `S/io/github/gammaengine/diag/MemoryReport.java` ; `run/gammaengine/reports` | — |
| Exécuteurs Bukkit en bytecode | absent (réflexion `method.invoke`) | `JavaPluginLoader.java:324-337` | — |
| Cache des classes transformées | absent (seuls des caches mémoire du `LaunchClassLoader` ; dump disque en débogage) | `javap` : champs `cachedClasses`, `resourceCache` ; `legacy.debugClassLoadingSave` | `legacy.debugClassLoadingSave` (débogage seulement) |
| Cache du scan d'annotations | absent (`JarDiscoverer` relit tous les jars à chaque démarrage) | `JarDiscoverer.java:62-85` | — |
| Cache du remapping des plugins | partiel : table `JarMapping` en mémoire par combinaison de drapeaux ; aucune persistance des classes remappées | `PluginClassLoader.java:51,279-290` ; dump seulement si `debug` (`:461`) | `plugin-settings.<nom>.custom-class-loader=false` coupe le remapping |
| AppCDS | absent | `-Xshare`/`SharedArchiveFile` : 0 occurrence dans `java9args.txt`, `build.gradle`, `README.md`, `docs/build.md` | — |
| CRaC | absent | 0 occurrence | — |
| `--add-opens` dans le lanceur | partiel : liste dans `java9args.txt`, à copier à la main ; pas de lanceur, pas d'`Add-Opens` au manifeste | `java9args.txt` ; `build.gradle:158-175` ; `docs/roadmap.md:104` | — |
| Transformers pour la réflexion sur les internes / cast du chargeur | absent comme transformers ; contournement dans le moteur seulement (`addUrlToClassloader`) | `CoreModManager.java:431-461` | — |
| Bibliothèques retirées du JDK embarquées | partiel : JAXB (jakarta), Nashorn 15.4, servlet 4.0.1, persistence 1.0.2 ; reste non vérifié (`javax.annotation`, `javax.activation`, CORBA…) | `build.gradle:92-95,129` | — |
| Script de lancement | absent (ni `.sh` ni `.bat` hors `gradlew`) | recherche de fichiers `start*`, `launch*`, `*.sh`, `*.bat` : seulement `gradlew*` | — |
| ZGC | absent | aucun `-XX:` dans le dépôt ; `docs/roadmap.md:67,105` le prévoit | — |
