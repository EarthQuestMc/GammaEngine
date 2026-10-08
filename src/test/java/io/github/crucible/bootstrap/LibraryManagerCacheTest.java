package io.github.crucible.bootstrap;

import io.github.gammaengine.startup.LibraryCheckCache;
import org.junit.After;
import org.junit.Before;
import org.junit.Test;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.FileTime;
import java.security.MessageDigest;
import java.util.Random;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** The startup check with its cache, on a fake library folder: no server involved. */
public class LibraryManagerCacheTest {
    private static final String[] LIBRARIES = {"t.g:a:1", "t.g:b:1", "t.g:c:1", "t.g:d:1", "t.g:e:1"};
    private static final long HOUR_AGO_MILLIS = System.currentTimeMillis() - TimeUnit.HOURS.toMillis(1);

    private Path root;
    private Path cacheFile;

    @Before
    public void createLibraries() throws Exception {
        root = Files.createTempDirectory("gamma-libraries");
        cacheFile = root.resolve(LibraryCheckCache.FILE_NAME);
        Random random = new Random(42);
        for (String library : LIBRARIES) {
            byte[] content = new byte[4096 + random.nextInt(4096)];
            random.nextBytes(content);
            writeJar(library, content, HOUR_AGO_MILLIS);
            Files.write(md5File(library), LibraryManager.encodeHex(
                    MessageDigest.getInstance("MD5").digest(content)).getBytes(StandardCharsets.UTF_8));
        }
    }

    @After
    public void deleteLibraries() {
        delete(root.toFile());
    }

    private static void delete(File file) {
        File[] children = file.listFiles();
        if (children != null) {
            for (File child : children) {
                delete(child);
            }
        }
        file.delete();
    }

    private Path jar(String library) {
        String[] parts = library.split(":");
        return root.resolve(parts[0].replace('.', '/')).resolve(parts[1]).resolve(parts[2])
                .resolve(parts[1] + "-" + parts[2] + ".jar");
    }

    private Path md5File(String library) {
        return jar(library).resolveSibling(jar(library).getFileName() + ".md5");
    }

    private void writeJar(String library, byte[] content, long modifiedMillis) throws IOException {
        Path jar = jar(library);
        Files.createDirectories(jar.getParent());
        Files.write(jar, content);
        Files.setLastModifiedTime(jar, FileTime.fromMillis(modifiedMillis));
    }

    private LibraryCheckCache check(boolean expected, int hashed, int trusted) throws Exception {
        LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
        assertEquals(expected, LibraryManager.checkIntegrity(root, LIBRARIES, cache));
        if (expected) {
            assertEquals("hashed", hashed, cache.hashed());
            assertEquals("trusted", trusted, cache.trusted());
            cache.saveIfChanged();
        }
        return cache;
    }

    @Test
    public void secondBootHashesNothing() throws Exception {
        check(true, 5, 0);
        assertTrue(Files.isRegularFile(cacheFile));
        check(true, 0, 5);
    }

    @Test
    public void modifiedJarIsHashedAndRejectedAsBefore() throws Exception {
        check(true, 5, 0);
        byte[] content = Files.readAllBytes(jar("t.g:c:1"));
        content[100] ^= 1; // same size, new timestamp: what an edit or a bad copy looks like
        writeJar("t.g:c:1", content, HOUR_AGO_MILLIS + 5000);
        check(false, 0, 0);
        assertFalse("the plain check agrees", LibraryManager.checkIntegrity(root, LIBRARIES));

        content[100] ^= 1; // put the right bytes back, with yet another timestamp
        writeJar("t.g:c:1", content, HOUR_AGO_MILLIS + 10000);
        check(true, 1, 4);
        check(true, 0, 5);
    }

    @Test
    public void newExpectedHashMeansHashingAgain() throws Exception {
        check(true, 5, 0);
        Files.write(md5File("t.g:b:1"), "00000000000000000000000000000000".getBytes(StandardCharsets.UTF_8));
        check(false, 0, 0);
    }

    @Test
    public void missingOrDamagedCacheMeansFullHash() throws Exception {
        check(true, 5, 0);
        Files.delete(cacheFile);
        check(true, 5, 0);
        assertTrue("written again", Files.isRegularFile(cacheFile));

        Files.write(cacheFile, "# GammaEngine library check cache, format 1\nnonsense\n".getBytes(StandardCharsets.UTF_8));
        LibraryCheckCache cache = check(true, 5, 0);
        assertTrue(cache.loadProblem() != null);
        check(true, 0, 5);
    }

    @Test
    public void disabledCacheHashesEverything() throws Exception {
        check(true, 5, 0);
        LibraryCheckCache disabled = LibraryCheckCache.disabled();
        assertTrue(LibraryManager.checkIntegrity(root, LIBRARIES, disabled));
        assertEquals(5, disabled.hashed());
        assertEquals(0, disabled.trusted());
    }

    /**
     * The accepted limit, written down: bytes that change while size and timestamp are restored are
     * not seen while the cache is on. Turning the switch off brings the full check back.
     */
    @Test
    public void corruptionKeepingSizeAndTimestampIsOnlySeenWithTheCacheOff() throws Exception {
        check(true, 5, 0);
        byte[] content = Files.readAllBytes(jar("t.g:d:1"));
        content[7] ^= 1;
        writeJar("t.g:d:1", content, HOUR_AGO_MILLIS);
        check(true, 0, 5);
        assertFalse(LibraryManager.checkIntegrity(root, LIBRARIES, LibraryCheckCache.disabled()));
    }
}
