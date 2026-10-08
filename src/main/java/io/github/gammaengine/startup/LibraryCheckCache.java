package io.github.gammaengine.startup;

import java.io.IOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;

/**
 * Remembers which library jars passed the startup MD5 check, so that a jar that has not changed
 * since is not read and hashed again on every boot.
 *
 * <p>A jar is trusted without hashing only when all of these still hold since it was last hashed
 * and found correct: same size, same last-modified time, and the {@code .md5} file next to it
 * still expects the hash that was verified. Anything else (no entry, any metadata change, a new
 * expected hash, a missing, unreadable or damaged cache file) means the jar is hashed exactly as
 * it would be without this class. What this gives up: a jar whose bytes change while its size and
 * timestamp stay the same (silent disk corruption, or a deliberate edit that restores the
 * timestamp) is no longer caught. The switch {@code gamma.startup.libraryCheckCache} turns the
 * cache off for anyone who wants the full check on every boot.
 *
 * <p>Two more guards against trusting the wrong bytes. A jar whose size or timestamp changed while
 * it was being hashed is not recorded. A jar modified less than {@link #RACY_WINDOW_MILLIS} before
 * the check (or dated in the future) is not recorded either: on a file system with coarse
 * timestamps a second edit could land on the same timestamp ("racily clean" files, as git calls
 * them). Such a jar is simply hashed again next time.
 *
 * <p>Runs in the bootstrap, before the libraries are verified and put to use: JDK classes only.
 * The verification threads call {@link #isUnchanged} and {@link #recordHashed} concurrently.
 */
public final class LibraryCheckCache {
    /** Name of the cache file, kept inside the library folder it describes. */
    public static final String FILE_NAME = ".gammaengine-library-check";

    static final String HEADER = "# GammaEngine library check cache, format 1";
    static final String COLUMNS = "# library<TAB>size in bytes<TAB>last modified (ns since epoch)<TAB>verified md5";
    static final String FOOTER = "# end ";

    /** Coarsest timestamp resolution of a file system a server may run on (FAT, exFAT: 2 s). */
    static final long RACY_WINDOW_MILLIS = 2000L;

    private final Path file;
    private final Map<String, Entry> previous;
    private final String loadProblem;
    private final Map<String, Entry> verified = new ConcurrentHashMap<>();
    private final AtomicInteger hashed = new AtomicInteger();
    private final AtomicInteger trusted = new AtomicInteger();

    private LibraryCheckCache(Path file, Map<String, Entry> previous, String loadProblem) {
        this.file = file;
        this.previous = previous;
        this.loadProblem = loadProblem;
    }

    /** A cache that trusts nothing and writes nothing: every jar is hashed, as before. */
    public static LibraryCheckCache disabled() {
        return new LibraryCheckCache(null, Collections.<String, Entry>emptyMap(), null);
    }

    /**
     * Reads the cache file. Never throws: a missing file gives an empty cache, an unreadable or
     * damaged one gives an empty cache and a {@link #loadProblem()}.
     */
    public static LibraryCheckCache load(Path file) {
        if (!Files.exists(file)) {
            return new LibraryCheckCache(file, Collections.<String, Entry>emptyMap(), null);
        }
        try {
            return new LibraryCheckCache(file, parse(Files.readAllLines(file, StandardCharsets.UTF_8)), null);
        } catch (IOException | RuntimeException e) {
            return new LibraryCheckCache(file, Collections.<String, Entry>emptyMap(), String.valueOf(e));
        }
    }

    public boolean enabled() {
        return file != null;
    }

    /** Why the cache file could not be used, or null when it was read or simply absent. */
    public String loadProblem() {
        return loadProblem;
    }

    /** Number of jars hashed by this check. */
    public int hashed() {
        return hashed.get();
    }

    /** Number of jars trusted from the cache by this check. */
    public int trusted() {
        return trusted.get();
    }

    /**
     * True when {@code library} was verified by an earlier check and nothing visible changed since:
     * same size and timestamp, and {@code expectedMd5} (the content of its {@code .md5} file, compared
     * the same way the hash check compares it) is the hash that was verified then.
     */
    public boolean isUnchanged(String library, FileState current, String expectedMd5) {
        Entry entry = previous.get(library);
        if (entry == null || current == null || !entry.state.equals(current) || !entry.md5.equalsIgnoreCase(expectedMd5)) {
            return false;
        }
        verified.put(library, entry);
        trusted.incrementAndGet();
        return true;
    }

    /**
     * Counts a jar that was hashed and matched its {@code .md5}, and remembers it when that is safe:
     * the cache is on, the jar did not change during hashing ({@code before} equals {@code after}),
     * and its timestamp is not within the racy window of {@code nowMillis}.
     */
    public void recordHashed(String library, FileState before, FileState after, String md5, long nowMillis) {
        hashed.incrementAndGet();
        if (!enabled() || before == null || !before.equals(after) || isRacy(before, nowMillis)) {
            return;
        }
        verified.put(library, new Entry(before, md5.toLowerCase(Locale.ROOT)));
    }

    static boolean isRacy(FileState state, long nowMillis) {
        return state.modifiedNanos >= TimeUnit.MILLISECONDS.toNanos(nowMillis - RACY_WINDOW_MILLIS);
    }

    /**
     * Writes what this check verified, replacing the file atomically, unless it already holds exactly
     * that. Call only after every library passed. Libraries that are no longer needed drop out.
     *
     * @return true when the file was written
     */
    public boolean saveIfChanged() throws IOException {
        if (!enabled() || (loadProblem == null && verified.equals(previous))) {
            return false;
        }
        // Not Files.createTempFile: its random name seeds a SecureRandom, measured at 250 ms on the
        // first call, more than the whole check it saves. CREATE_NEW still refuses an existing file.
        Path temporary = file.toAbsolutePath().resolveSibling(
                FILE_NAME + "." + Long.toHexString(System.nanoTime()) + ".tmp");
        Writer writer = Files.newBufferedWriter(temporary, StandardCharsets.UTF_8,
                StandardOpenOption.CREATE_NEW, StandardOpenOption.WRITE);
        try {
            try {
                for (String line : format(verified)) {
                    writer.write(line);
                    writer.write('\n');
                }
            } finally {
                writer.close();
            }
            try {
                Files.move(temporary, file, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(temporary, file, StandardCopyOption.REPLACE_EXISTING);
            }
        } finally {
            Files.deleteIfExists(temporary);
        }
        return true;
    }

    static List<String> format(Map<String, Entry> entries) {
        List<String> lines = new ArrayList<>(entries.size() + 3);
        lines.add(HEADER);
        lines.add(COLUMNS);
        for (Map.Entry<String, Entry> entry : new TreeMap<>(entries).entrySet()) {
            Entry value = entry.getValue();
            lines.add(entry.getKey() + '\t' + value.state.size + '\t' + value.state.modifiedNanos + '\t' + value.md5);
        }
        lines.add(FOOTER + entries.size());
        return lines;
    }

    /**
     * Parses the file written by {@link #format}. Strict on purpose: any line it does not understand
     * rejects the whole file, which only costs one full hash.
     *
     * @throws IllegalArgumentException when the content is not a complete cache file
     */
    static Map<String, Entry> parse(List<String> lines) {
        int last = lines.size() - 1;
        while (last >= 0 && lines.get(last).trim().isEmpty()) {
            last--;
        }
        if (last < 1 || !HEADER.equals(lines.get(0))) {
            throw new IllegalArgumentException("not a library check cache");
        }
        String footer = lines.get(last);
        if (!footer.startsWith(FOOTER)) {
            throw new IllegalArgumentException("truncated: no end line");
        }
        int expected = Integer.parseInt(footer.substring(FOOTER.length()).trim());
        Map<String, Entry> entries = new HashMap<>();
        for (int i = 1; i < last; i++) {
            String line = lines.get(i);
            if (line.startsWith("#")) {
                continue;
            }
            String[] fields = line.split("\t", -1);
            if (fields.length != 4 || fields[0].isEmpty()) {
                throw new IllegalArgumentException("bad line " + (i + 1));
            }
            long size = Long.parseLong(fields[1]);
            long modified = Long.parseLong(fields[2]);
            String md5 = fields[3];
            if (size < 0 || !isMd5(md5)) {
                throw new IllegalArgumentException("bad line " + (i + 1));
            }
            if (entries.put(fields[0], new Entry(new FileState(size, modified), md5)) != null) {
                throw new IllegalArgumentException("duplicate " + fields[0]);
            }
        }
        if (entries.size() != expected) {
            throw new IllegalArgumentException("expected " + expected + " entries, found " + entries.size());
        }
        return entries;
    }

    private static boolean isMd5(String value) {
        if (value.length() != 32) {
            return false;
        }
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (!((c >= '0' && c <= '9') || (c >= 'a' && c <= 'f'))) {
                return false;
            }
        }
        return true;
    }

    /** Size and last-modified time of a jar, the two things compared to decide it did not change. */
    public static final class FileState {
        final long size;
        final long modifiedNanos;

        FileState(long size, long modifiedNanos) {
            this.size = size;
            this.modifiedNanos = modifiedNanos;
        }

        public static FileState of(Path file) throws IOException {
            BasicFileAttributes attributes = Files.readAttributes(file, BasicFileAttributes.class);
            return new FileState(attributes.size(), attributes.lastModifiedTime().to(TimeUnit.NANOSECONDS));
        }

        @Override
        public boolean equals(Object other) {
            if (!(other instanceof FileState)) {
                return false;
            }
            FileState that = (FileState) other;
            return size == that.size && modifiedNanos == that.modifiedNanos;
        }

        @Override
        public int hashCode() {
            return Long.hashCode(size) * 31 + Long.hashCode(modifiedNanos);
        }

        @Override
        public String toString() {
            return size + " bytes, modified " + modifiedNanos + " ns";
        }
    }

    static final class Entry {
        final FileState state;
        final String md5;

        Entry(FileState state, String md5) {
            this.state = state;
            this.md5 = md5;
        }

        @Override
        public boolean equals(Object other) {
            if (!(other instanceof Entry)) {
                return false;
            }
            Entry that = (Entry) other;
            return state.equals(that.state) && md5.equals(that.md5);
        }

        @Override
        public int hashCode() {
            return state.hashCode() * 31 + md5.hashCode();
        }
    }
}
