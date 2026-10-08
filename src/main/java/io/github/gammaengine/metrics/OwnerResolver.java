package io.github.gammaengine.metrics;

import cpw.mods.fml.common.Loader;
import cpw.mods.fml.common.ModContainer;
import io.github.gammaengine.GammaEngine;

import java.io.File;
import java.io.IOException;
import java.net.URI;
import java.net.URL;
import java.security.CodeSource;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.WeakHashMap;

/**
 * Finds which mod or plugin a class belongs to, from the jar it was loaded from.
 *
 * <p>Owners are named {@code minecraft} for the server itself (Minecraft, Forge, Bukkit, Crucible,
 * GammaEngine and their libraries), the mod id for a mod, {@code plugin:<name>} for a Bukkit plugin,
 * {@code jar:<file>} for a jar that matches neither (a coremod without a mod container, for
 * instance) and {@code unknown} for a class with no code source.
 *
 * <p>Resolution happens when a recording ends, never during a tick: it touches the file system to
 * canonicalise paths. Results are cached per class, weakly so a reloaded plugin can be unloaded.
 *
 * <p>Thread invariant: any thread may call {@link #ownerOf}; the cache is guarded by the instance
 * lock, and the jar table is immutable once built.
 */
final class OwnerResolver implements AttributionReport.Owners {
    static final String SERVER = "minecraft";
    static final String UNKNOWN = "unknown";

    /** A mod, and the package of its {@code @Mod} class, used to split a jar that holds several mods. */
    static final class ModEntry {
        final String modId;
        final String modPackage;

        ModEntry(String modId, String modPackage) {
            this.modId = modId;
            this.modPackage = modPackage;
        }
    }

    // Classes with no code source in these packages are still the server's own.
    private static final String[] SERVER_PACKAGES = {
            "net.minecraft.", "net.minecraftforge.", "cpw.mods.fml.", "org.bukkit.", "org.spigotmc.",
            "io.github.crucible.", "io.github.gammaengine.", "co.aikar.", "thermos.", "kcauldron.",
    };

    private static OwnerResolver server;

    private final Set<String> serverLocations;
    private final String serverDirectory;
    private final Map<String, List<ModEntry>> modsByLocation;
    private final Map<Class<?>, String> cache = new WeakHashMap<Class<?>, String>();

    /**
     * @param serverLocations normalised locations of the server's own jars
     * @param serverDirectory normalised directory whose jars all belong to the server (the
     *                        libraries folder), ending with a separator, or {@code null}
     * @param modsByLocation  normalised jar location to the mods it holds, in load order
     */
    OwnerResolver(Set<String> serverLocations, String serverDirectory, Map<String, List<ModEntry>> modsByLocation) {
        this.serverLocations = serverLocations;
        this.serverDirectory = serverDirectory;
        this.modsByLocation = modsByLocation;
    }

    /** The resolver of the running server, built once from the FML mod list. */
    static synchronized OwnerResolver forServer() {
        if (server == null) {
            server = build();
        }
        return server;
    }

    private static OwnerResolver build() {
        Set<String> serverLocations = new HashSet<String>();
        for (Class<?> anchor : new Class<?>[]{net.minecraft.server.MinecraftServer.class, Loader.class,
                org.bukkit.Bukkit.class, GammaEngine.class}) {
            String location = location(anchor);
            if (location != null) {
                serverLocations.add(location);
            }
        }
        String libraries = canonical(new File("libraries")) + File.separator;
        Map<String, List<ModEntry>> mods = new HashMap<String, List<ModEntry>>();
        List<ModContainer> modList;
        try {
            modList = Loader.instance().getModList();
        } catch (RuntimeException e) {
            GammaEngine.LOGGER.warn("Cannot read the mod list, classes will be attributed by jar name only", e);
            modList = new ArrayList<ModContainer>();
        }
        for (ModContainer mod : modList) {
            try {
                File source = mod.getSource();
                if (source == null) {
                    continue;
                }
                String location = canonical(source);
                // mcp, FML, Forge and Crucible live in the server jars: those classes stay "minecraft".
                if (serverLocations.contains(location) || location.startsWith(libraries)) {
                    continue;
                }
                Object instance = mod.getMod();
                List<ModEntry> entries = mods.get(location);
                if (entries == null) {
                    entries = new ArrayList<ModEntry>(1);
                    mods.put(location, entries);
                }
                entries.add(new ModEntry(mod.getModId(), instance == null ? "" : packageOf(instance.getClass().getName())));
            } catch (RuntimeException e) {
                GammaEngine.LOGGER.warn("Cannot locate the jar of mod {}, its classes will be attributed by jar name", mod, e);
            }
        }
        return new OwnerResolver(serverLocations, libraries, mods);
    }

    @Override
    public synchronized String ownerOf(Class<?> type) {
        String owner = cache.get(type);
        if (owner == null) {
            String plugin = pluginName(type.getClassLoader());
            owner = plugin != null ? "plugin:" + plugin : resolve(type.getName(), location(type));
            cache.put(type, owner);
        }
        return owner;
    }

    /** Owner of a class that no plugin class loader claimed, from its normalised code location. */
    String resolve(String className, String location) {
        if (location == null) {
            return isServerPackage(className) ? SERVER : UNKNOWN;
        }
        List<ModEntry> mods = modsByLocation.get(location);
        if (mods != null && !mods.isEmpty()) {
            return pick(mods, className);
        }
        if (serverLocations.contains(location) || (serverDirectory != null && location.startsWith(serverDirectory))) {
            return SERVER;
        }
        return "jar:" + new File(location).getName();
    }

    /**
     * One jar can hold several mods (BuildCraft ships six). The mod whose {@code @Mod} class shares
     * the longest package prefix with the class wins; on a tie, the first one loaded.
     */
    static String pick(List<ModEntry> mods, String className) {
        ModEntry best = mods.get(0);
        int bestScore = -1;
        for (ModEntry mod : mods) {
            int score = sharedSegments(packageOf(className), mod.modPackage);
            if (score > bestScore) {
                best = mod;
                bestScore = score;
            }
        }
        return best.modId;
    }

    /** Number of leading package segments two packages have in common. */
    static int sharedSegments(String a, String b) {
        if (a.isEmpty() || b.isEmpty()) {
            return 0;
        }
        String[] left = a.split("\\.");
        String[] right = b.split("\\.");
        int shared = 0;
        while (shared < left.length && shared < right.length && left[shared].equals(right[shared])) {
            shared++;
        }
        return shared;
    }

    static boolean isServerPackage(String className) {
        for (String prefix : SERVER_PACKAGES) {
            if (className.startsWith(prefix)) {
                return true;
            }
        }
        return false;
    }

    static String packageOf(String className) {
        int dot = className.lastIndexOf('.');
        return dot < 0 ? "" : className.substring(0, dot);
    }

    private static String pluginName(ClassLoader loader) {
        if (loader instanceof org.bukkit.plugin.java.PluginClassLoader) {
            org.bukkit.plugin.java.JavaPlugin plugin = ((org.bukkit.plugin.java.PluginClassLoader) loader).getPlugin();
            return plugin == null ? null : plugin.getName();
        }
        return null;
    }

    /** Normalised location of the jar or directory a class was loaded from, or {@code null}. */
    static String location(Class<?> type) {
        try {
            CodeSource source = type.getProtectionDomain().getCodeSource();
            URL url = source == null ? null : source.getLocation();
            return url == null ? null : normalize(url.toString(), type.getName());
        } catch (SecurityException e) {
            return null;
        }
    }

    /**
     * Turns a code source URL into the canonical path of its jar or directory. The launch class
     * loader gives the URL of the class file itself ({@code jar:file:/mods/a.jar!/a/B.class}),
     * a plain class loader the URL of the jar; both end up as {@code /mods/a.jar}.
     */
    static String normalize(String url, String className) {
        String path = url;
        if (path.startsWith("jar:")) {
            path = path.substring(4);
            int entry = path.indexOf("!/");
            if (entry >= 0) {
                path = path.substring(0, entry);
            }
        } else {
            String classFile = "/" + className.replace('.', '/') + ".class";
            if (path.endsWith(classFile)) {
                path = path.substring(0, path.length() - classFile.length() + 1);
            }
        }
        File file = toFile(path);
        return file == null ? path : canonical(file);
    }

    private static File toFile(String url) {
        if (!url.startsWith("file:")) {
            return null;
        }
        try {
            return new File(new URI(url.replace(" ", "%20")));
        } catch (Exception e) {
            // An authority component or a malformed escape: fall back to the raw path.
            return new File(url.substring("file:".length()));
        }
    }

    static String canonical(File file) {
        try {
            return file.getCanonicalPath();
        } catch (IOException e) {
            return file.getAbsolutePath();
        }
    }
}
