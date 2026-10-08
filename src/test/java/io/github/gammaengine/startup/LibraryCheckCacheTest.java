package io.github.gammaengine.startup;

import io.github.gammaengine.startup.LibraryCheckCache.Entry;
import io.github.gammaengine.startup.LibraryCheckCache.FileState;
import org.junit.After;
import org.junit.Before;
import org.junit.Test;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.FileTime;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;
import static org.junit.Assert.fail;

public class LibraryCheckCacheTest {
    private static final String MD5_A = "0123456789abcdef0123456789abcdef";
    private static final String MD5_B = "fedcba9876543210fedcba9876543210";
    private static final long HOUR_AGO_MILLIS = System.currentTimeMillis() - TimeUnit.HOURS.toMillis(1);

    private Path directory;
    private Path cacheFile;

    @Before
    public void createDirectory() throws IOException {
        directory = Files.createTempDirectory("gamma-libcache");
        cacheFile = directory.resolve(LibraryCheckCache.FILE_NAME);
    }

    @After
    public void deleteDirectory() {
        File[] files = directory.toFile().listFiles();
        if (files != null) {
            for (File file : files) {
                file.delete();
            }
        }
        directory.toFile().delete();
    }

    private static Map<String, Entry> sample() {
        Map<String, Entry> entries = new HashMap<>();
        entries.put("org.scala-lang:scala-library:2.11.1", new Entry(new FileState(5538130L, 1700000000123456700L), MD5_A));
        entries.put("com.google.guava:guava:17.0", new Entry(new FileState(2243036L, 1600000000000000000L), MD5_B));
        return entries;
    }

    private Path jar(String name, byte[] content, long modifiedMillis) throws IOException {
        Path jar = directory.resolve(name);
        Files.write(jar, content);
        Files.setLastModifiedTime(jar, FileTime.fromMillis(modifiedMillis));
        return jar;
    }

    @Test
    public void formatThenParseGivesTheSameEntries() {
        Map<String, Entry> entries = sample();
        assertEquals(entries, LibraryCheckCache.parse(LibraryCheckCache.format(entries)));
    }

    @Test
    public void emptyCacheRoundTrips() {
        Map<String, Entry> entries = new HashMap<>();
        assertEquals(entries, LibraryCheckCache.parse(LibraryCheckCache.format(entries)));
    }

    @Test
    public void damagedContentIsRejected() {
        List<String> good = LibraryCheckCache.format(sample());
        List<List<String>> damaged = Arrays.asList(
                good.subList(1, good.size()),                                   // no header
                good.subList(0, good.size() - 1),                               // truncated: no end line
                replace(good, good.size() - 1, LibraryCheckCache.FOOTER + "3"), // wrong count
                replace(good, 2, "a:b:1\t12\t34"),                              // missing field
                replace(good, 2, "a:b:1\t-1\t34\t" + MD5_A),                    // negative size
                replace(good, 2, "a:b:1\t12\tsoon\t" + MD5_A),                  // not a number
                replace(good, 2, "a:b:1\t12\t34\t" + MD5_A.substring(1)),       // short md5
                replace(good, 2, "a:b:1\t12\t34\t" + MD5_A.toUpperCase()),      // not as written
                replace(good, 3, good.get(2)),                                  // duplicate library
                Arrays.asList("garbage"));
        for (List<String> lines : damaged) {
            try {
                LibraryCheckCache.parse(lines);
                fail("accepted " + lines);
            } catch (IllegalArgumentException expected) {
                // NumberFormatException included
            }
        }
    }

    private static List<String> replace(List<String> lines, int index, String line) {
        List<String> copy = new java.util.ArrayList<>(lines);
        copy.set(index, line);
        return copy;
    }

    @Test
    public void missingFileMeansEmptyCacheWithoutProblem() {
        LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
        assertTrue(cache.enabled());
        assertNull(cache.loadProblem());
        assertFalse(cache.isUnchanged("a:b:1", new FileState(1, 1), MD5_A));
    }

    @Test
    public void corruptFileMeansEmptyCacheAndIsRewritten() throws IOException {
        Files.write(cacheFile, new byte[]{(byte) 0xC3, 0x28, 0x00, (byte) 0xFF, '\n', 'x'}); // invalid UTF-8
        LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
        assertNotNull(cache.loadProblem());
        assertFalse(cache.isUnchanged("a:b:1", new FileState(1, 1), MD5_A));
        assertTrue("a damaged file is replaced even when nothing was verified", cache.saveIfChanged());
        assertNull(LibraryCheckCache.load(cacheFile).loadProblem());
    }

    @Test
    public void unchangedJarIsTrustedOnTheNextCheck() throws IOException {
        Path jar = jar("lib.jar", new byte[]{1, 2, 3}, HOUR_AGO_MILLIS);
        LibraryCheckCache first = LibraryCheckCache.load(cacheFile);
        FileState state = FileState.of(jar);
        assertFalse(first.isUnchanged("a:b:1", state, MD5_A));
        first.recordHashed("a:b:1", state, FileState.of(jar), MD5_A, System.currentTimeMillis());
        assertEquals(1, first.hashed());
        assertTrue(first.saveIfChanged());

        LibraryCheckCache second = LibraryCheckCache.load(cacheFile);
        assertTrue(second.isUnchanged("a:b:1", FileState.of(jar), MD5_A));
        assertTrue("the .md5 file is compared ignoring case, like the hash check",
                second.isUnchanged("a:b:1", FileState.of(jar), MD5_A.toUpperCase()));
        assertEquals(2, second.trusted());
        assertEquals(0, second.hashed());
        assertFalse("nothing changed, nothing to write", second.saveIfChanged());
    }

    @Test
    public void anyVisibleChangeMeansHashingAgain() throws IOException {
        Path jar = jar("lib.jar", new byte[]{1, 2, 3}, HOUR_AGO_MILLIS);
        LibraryCheckCache first = LibraryCheckCache.load(cacheFile);
        first.recordHashed("a:b:1", FileState.of(jar), FileState.of(jar), MD5_A, System.currentTimeMillis());
        first.saveIfChanged();
        LibraryCheckCache second = LibraryCheckCache.load(cacheFile);

        assertFalse("other expected hash", second.isUnchanged("a:b:1", FileState.of(jar), MD5_B));
        assertFalse("other expected text", second.isUnchanged("a:b:1", FileState.of(jar), MD5_A + " "));
        assertFalse("other library", second.isUnchanged("a:b:2", FileState.of(jar), MD5_A));

        Files.setLastModifiedTime(jar, FileTime.fromMillis(HOUR_AGO_MILLIS + 1000));
        assertFalse("new timestamp", second.isUnchanged("a:b:1", FileState.of(jar), MD5_A));

        jar("lib.jar", new byte[]{1, 2, 3, 4}, HOUR_AGO_MILLIS);
        assertFalse("new size", second.isUnchanged("a:b:1", FileState.of(jar), MD5_A));
        assertEquals(0, second.trusted());
    }

    @Test
    public void recentlyModifiedJarIsNotRemembered() throws IOException {
        long now = System.currentTimeMillis();
        Path jar = jar("lib.jar", new byte[]{1}, now - 500);
        LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
        cache.recordHashed("a:b:1", FileState.of(jar), FileState.of(jar), MD5_A, now);
        assertEquals(1, cache.hashed());
        assertFalse("nothing safe to remember", cache.saveIfChanged());
        assertFalse(Files.exists(cacheFile));

        assertTrue(LibraryCheckCache.isRacy(new FileState(1, TimeUnit.MILLISECONDS.toNanos(now + 60000)), now));
        assertFalse(LibraryCheckCache.isRacy(new FileState(1, TimeUnit.MILLISECONDS.toNanos(now - 2001)), now));
    }

    @Test
    public void jarChangedDuringHashingIsNotRemembered() throws IOException {
        LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
        FileState before = new FileState(3, TimeUnit.MILLISECONDS.toNanos(HOUR_AGO_MILLIS));
        FileState after = new FileState(4, TimeUnit.MILLISECONDS.toNanos(HOUR_AGO_MILLIS));
        cache.recordHashed("a:b:1", before, after, MD5_A, System.currentTimeMillis());
        assertFalse(cache.saveIfChanged());
    }

    @Test
    public void librariesNoLongerNeededDropOut() throws IOException {
        Path jar = jar("lib.jar", new byte[]{1, 2, 3}, HOUR_AGO_MILLIS);
        LibraryCheckCache first = LibraryCheckCache.load(cacheFile);
        first.recordHashed("a:b:1", FileState.of(jar), FileState.of(jar), MD5_A, System.currentTimeMillis());
        first.recordHashed("a:b:old", FileState.of(jar), FileState.of(jar), MD5_A, System.currentTimeMillis());
        first.saveIfChanged();

        LibraryCheckCache second = LibraryCheckCache.load(cacheFile);
        assertTrue(second.isUnchanged("a:b:1", FileState.of(jar), MD5_A));
        assertTrue("one library fewer than the file holds", second.saveIfChanged());
        List<String> lines = Files.readAllLines(cacheFile, StandardCharsets.UTF_8);
        assertEquals(LibraryCheckCache.FOOTER + "1", lines.get(lines.size() - 1));
    }

    @Test
    public void saveLeavesNoTemporaryFile() throws IOException {
        Path jar = jar("lib.jar", new byte[]{1, 2, 3}, HOUR_AGO_MILLIS);
        for (int round = 0; round < 2; round++) {
            LibraryCheckCache cache = LibraryCheckCache.load(cacheFile);
            cache.recordHashed("a:b:" + round, FileState.of(jar), FileState.of(jar), MD5_A, System.currentTimeMillis());
            assertTrue(cache.saveIfChanged());
        }
        String[] names = directory.toFile().list();
        assertNotNull(names);
        Arrays.sort(names);
        assertEquals(Arrays.asList(LibraryCheckCache.FILE_NAME, "lib.jar"), Arrays.asList(names));
    }

    @Test
    public void disabledCacheTrustsNothingAndWritesNothing() throws IOException {
        Path jar = jar("lib.jar", new byte[]{1, 2, 3}, HOUR_AGO_MILLIS);
        LibraryCheckCache cache = LibraryCheckCache.disabled();
        assertFalse(cache.enabled());
        cache.recordHashed("a:b:1", null, null, MD5_A, System.currentTimeMillis());
        assertFalse(cache.isUnchanged("a:b:1", FileState.of(jar), MD5_A));
        assertEquals(1, cache.hashed());
        assertFalse(cache.saveIfChanged());
    }
}
