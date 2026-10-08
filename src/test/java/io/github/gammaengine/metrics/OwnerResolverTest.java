package io.github.gammaengine.metrics;

import org.junit.Test;

import java.io.File;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;

/** Class to mod resolution from a jar table given by the test, as the server builds it from FML. */
public class OwnerResolverTest {
    private static final File ROOT = new File(System.getProperty("java.io.tmpdir"), "gamma owner test");
    private static final File SERVER_JAR = new File(ROOT, "server.jar");
    private static final File LIBRARIES = new File(ROOT, "libraries");
    private static final File ALPHA_JAR = new File(ROOT, "mods/alpha-1.0.jar");
    private static final File BUILDCRAFT_JAR = new File(ROOT, "mods/buildcraft-7.1.jar");

    private static String key(File file) {
        return OwnerResolver.canonical(file);
    }

    private static OwnerResolver resolver() {
        Set<String> server = new HashSet<String>(Collections.singleton(key(SERVER_JAR)));
        Map<String, List<OwnerResolver.ModEntry>> mods = new HashMap<String, List<OwnerResolver.ModEntry>>();
        mods.put(key(ALPHA_JAR), Collections.singletonList(new OwnerResolver.ModEntry("alpha", "com.example.alpha")));
        mods.put(key(BUILDCRAFT_JAR), Arrays.asList(
                new OwnerResolver.ModEntry("BuildCraft|Core", "buildcraft"),
                new OwnerResolver.ModEntry("BuildCraft|Transport", "buildcraft.transport")));
        return new OwnerResolver(server, key(LIBRARIES) + File.separator, mods);
    }

    @Test
    public void launchClassLoaderUrlsPointAtTheJar() {
        // LaunchClassLoader gives the URL of the class file inside the jar, escaped.
        String url = "jar:" + ALPHA_JAR.toURI() + "!/com/example/alpha/EntityThing.class";
        assertEquals(key(ALPHA_JAR), OwnerResolver.normalize(url, "com.example.alpha.EntityThing"));
    }

    @Test
    public void unescapedSpacesAreAccepted() {
        String path = ALPHA_JAR.getAbsolutePath().replace(File.separatorChar, '/');
        String url = "file:" + (path.startsWith("/") ? "" : "/") + path;
        assertEquals(key(ALPHA_JAR), OwnerResolver.normalize(url, "com.example.alpha.EntityThing"));
    }

    @Test
    public void classFilesInADirectoryResolveToTheDirectory() {
        File classes = new File(ROOT, "classes");
        String base = classes.toURI().toString();
        base = base.endsWith("/") ? base : base + "/";
        assertEquals(key(classes), OwnerResolver.normalize(base + "com/example/alpha/EntityThing.class",
                "com.example.alpha.EntityThing"));
        assertEquals(key(classes), OwnerResolver.normalize(base, "com.example.alpha.EntityThing"));
    }

    @Test
    public void modJarsResolveToTheirMod() {
        assertEquals("alpha", resolver().resolve("com.example.alpha.EntityThing", key(ALPHA_JAR)));
        assertEquals("alpha", resolver().resolve("com.example.alpha.TileThing$Inner", key(ALPHA_JAR)));
    }

    @Test
    public void aJarWithSeveralModsPicksTheClosestPackage() {
        OwnerResolver resolver = resolver();
        assertEquals("BuildCraft|Transport", resolver.resolve("buildcraft.transport.TileGenericPipe", key(BUILDCRAFT_JAR)));
        // Same distance to both mods: the first one loaded wins.
        assertEquals("BuildCraft|Core", resolver.resolve("buildcraft.energy.TileEngine", key(BUILDCRAFT_JAR)));
        assertEquals("BuildCraft|Core", resolver.resolve("Unpackaged", key(BUILDCRAFT_JAR)));
    }

    @Test
    public void serverJarsAndLibrariesAreMinecraft() {
        OwnerResolver resolver = resolver();
        assertEquals(OwnerResolver.SERVER, resolver.resolve("net.minecraft.entity.passive.EntityPig", key(SERVER_JAR)));
        File vanilla = new File(LIBRARIES, "net/minecraft/server/1.7.10/server-1.7.10.jar");
        assertEquals(OwnerResolver.SERVER, resolver.resolve("net.minecraft.tileentity.TileEntityHopper", key(vanilla)));
    }

    @Test
    public void otherJarsAndMissingSourcesAreNamedAsSuch() {
        OwnerResolver resolver = resolver();
        assertEquals("jar:coremod.jar", resolver.resolve("org.other.Thing", key(new File(ROOT, "mods/coremod.jar"))));
        assertEquals(OwnerResolver.SERVER, resolver.resolve("net.minecraft.entity.Generated", null));
        assertEquals(OwnerResolver.UNKNOWN, resolver.resolve("com.example.Generated", null));
    }

    @Test
    public void realClassesAreResolvedFromTheirCodeSourceAndCached() {
        String here = OwnerResolver.location(OwnerResolverTest.class);
        assertNotNull(here);
        Map<String, List<OwnerResolver.ModEntry>> mods = new HashMap<String, List<OwnerResolver.ModEntry>>();
        mods.put(here, Collections.singletonList(new OwnerResolver.ModEntry("testmod", "io.github.gammaengine")));
        OwnerResolver resolver = new OwnerResolver(Collections.<String>emptySet(), null, mods);
        assertEquals("testmod", resolver.ownerOf(OwnerResolverTest.class));
        assertEquals("testmod", resolver.ownerOf(OwnerResolverTest.class));
        // The JDK has no code source and is not the server.
        assertEquals(OwnerResolver.UNKNOWN, resolver.ownerOf(String.class));
    }

    @Test
    public void sharedSegmentsCountsWholePackageNames() {
        assertEquals(2, OwnerResolver.sharedSegments("buildcraft.transport.pipes", "buildcraft.transport"));
        assertEquals(1, OwnerResolver.sharedSegments("buildcraft.transportation", "buildcraft.transport"));
        assertEquals(0, OwnerResolver.sharedSegments("", "buildcraft"));
    }
}
