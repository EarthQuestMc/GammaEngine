package io.github.crucible.bootstrap;

import cpw.mods.fml.common.launcher.FMLTweaker;
import io.github.gammaengine.startup.EarlyConfig;
import io.github.gammaengine.startup.LibraryCheckCache;
import net.minecraft.launchwrapper.LaunchClassLoader;

import java.io.File;
import java.io.FileReader;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.NoSuchAlgorithmException;
import java.util.*;

// DO NOT TRY TO LOAD ANY MINECRAFT CLASS FROM HERE, THIS CLASS IS LOADED BEFORE EVERYTHING ON THE SERVER ENTRYPOINT
// Also avoid using streams here
public class CrucibleServerMainHook {
    private static final String[] REPOS = new String[]{
      "https://github.com/juanmuscaria/maven/raw/master/",
      "https://github.com/juanmuscaria/maven/raw/master/ThermosLibs/",
      "https://maven.minecraftforge.net/",
      "https://libraries.minecraft.net/",
      "https://repo.maven.apache.org/maven2/"
    };
    private static final Path LIBRARY_ROOT = Paths.get("libraries").toAbsolutePath();
    public static final PrintStream originalOut = System.out;
    public static final PrintStream originalErr = System.err;
    private static String libraryCheckSummary = "";

    public static void relaunchMain(String[] args) throws Exception {
        // GammaEngine - the banner is the first thing on the console, before any log framework is
        // configured, so an operator always knows which build they are looking at.
        io.github.gammaengine.util.ConsoleBanner.print(System.out,
                "Minecraft 1.7.10  |  Forge 10.13.4.1614  |  Bukkit 1.7.10-R0.1-SNAPSHOT",
                "Forge + Bukkit server built for player count and fast startup");
        System.out.println("[GammaEngine] Running pre-launch tweaks");

        File injectFile = new File("inject.properties");
        if (!injectFile.exists()) {
            Properties internalProperties = new Properties();
            try {
                internalProperties.load(CrucibleServerMainHook.class.getClassLoader().getResourceAsStream("inject.properties"));
                internalProperties.store(Files.newOutputStream(injectFile.toPath()),
                  "All properties in this file will be injected into System Properties before the server starts. Useful for shared hostings");
            } catch (IOException e) {
                throw new RuntimeException(e);
            }
        }
        Properties propertiesToInject = new Properties();
        try {
            propertiesToInject.load(new FileReader(injectFile));
        } catch (IOException e) {
            throw new RuntimeException(e);
        }

        for (Map.Entry<Object, Object> entry : propertiesToInject.entrySet()) {
            System.setProperty((String) entry.getKey(), (String) entry.getValue());
        }

        Lwjgl3ifyGlue.checkJava();

        // GammaEngine - time the verification: it runs on every boot, even when nothing changed. Only
        // the jars that changed since the last successful check are read and hashed (see below).
        long verificationStart = System.nanoTime();
        boolean librariesReady = verifyLibraries();
        long verificationMillis = (System.nanoTime() - verificationStart) / 1_000_000L;

        if (!librariesReady) {
            setupLibraries();
            System.out.println("[GammaEngine] GammaEngine installed! A restart is required to be able to boot.");
            System.exit(0);
        } else {
            System.out.println("[GammaEngine] Libraries verified in " + verificationMillis + " ms"
                    + libraryCheckSummary + ", booting the server");
        }
    }


    public static void serverMain(String[] args) {
        if (Boolean.parseBoolean(System.getProperty("crucible.i.know.what.i.am.doing.please.crash.the.server")))
            throw new RuntimeException("crucible.i.know.what.i.am.doing.please.crash.the.server=true! If you say so, crashing the server as you asked!");
    }

    private static boolean verifyLibraries() throws IOException, NoSuchAlgorithmException {
        if (Boolean.parseBoolean(System.getProperty("crucible.skipLibraryVerification"))) {
            System.out.println("[GammaEngine] Skipping library integrity verification");
            return true;
        }
        if (!Files.isDirectory(LIBRARY_ROOT)) {
            return false;
        }
        // GammaEngine - jars unchanged since the last successful check (same size, timestamp and
        // expected MD5) are not hashed again. The switch is read without GammaConfig: SnakeYAML and
        // log4j are in the very jars being checked (see EarlyConfig).
        boolean useCache = EarlyConfig.libraryCheckCache(Paths.get("gammaengine.yml"), System.out);
        LibraryCheckCache cache = useCache
                ? LibraryCheckCache.load(LIBRARY_ROOT.resolve(LibraryCheckCache.FILE_NAME))
                : LibraryCheckCache.disabled();
        if (cache.loadProblem() != null) {
            System.out.println("[GammaEngine] Library check cache unusable (" + cache.loadProblem()
                    + "), hashing every library");
        }
        boolean verified = LibraryManager.checkIntegrity(LIBRARY_ROOT, CrucibleMetadata.NEEDED_LIBRARIES, cache);
        if (verified) {
            try {
                cache.saveIfChanged();
            } catch (IOException e) {
                System.out.println("[GammaEngine] Could not save the library check cache (" + e
                        + "), changed libraries will be hashed again next boot");
            }
            libraryCheckSummary = " (" + cache.hashed() + " hashed, "
                    + (useCache ? cache.trusted() + " unchanged since the last check" : "check cache off") + ")";
        }
        return verified;
    }

    private static void setupLibraries() throws InterruptedException {
        System.out.println("[GammaEngine] Setting up server libraries, it may take a few minutes");
        if (Files.exists(LIBRARY_ROOT) && !Files.isDirectory(LIBRARY_ROOT)) {
            throw new IllegalStateException(String.format("Library root '%s' is a file, aborting startup!", LIBRARY_ROOT.toAbsolutePath()));
        }

        String[] userDefinedRepos = System.getProperty("crucible.libraryRepos", "").split(" ");

        List<String> list = new ArrayList<>();
        for (String[] strings : Arrays.asList(REPOS, userDefinedRepos)) {
            for (String s : strings) {
                if (!s.isEmpty()) {
                    list.add(s);
                }
            }
        }
        LibraryManager.downloadMavenLibraries(LIBRARY_ROOT, list.toArray(new String[0]), CrucibleMetadata.NEEDED_LIBRARIES);
    }

    public static void restoreStreams() {
        System.setOut(originalOut);
        System.setErr(originalErr);
    }
}