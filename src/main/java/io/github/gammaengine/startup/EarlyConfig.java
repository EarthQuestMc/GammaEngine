package io.github.gammaengine.startup;

import java.io.IOException;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;

/**
 * Reads the few {@code gammaengine.yml} switches needed before the libraries are verified.
 *
 * <p>{@link io.github.gammaengine.config.GammaConfig} cannot be used that early: it goes through
 * SnakeYAML and logs through log4j, and both live in the library jars that the bootstrap is about to
 * check. Loading them first would run code from jars not yet verified, and a missing jar would end
 * the boot with a {@code NoClassDefFoundError} before the libraries could be installed again. So
 * this class understands just the block-style subset of YAML that {@code GammaConfig} writes:
 * nested keys by indentation, comments, optional quotes. Anything it does not understand reads as
 * "absent", which means the default.
 *
 * <p>The keys are still declared in {@code GammaConfig}, so they appear, with their comments, in the
 * file it generates. A system property overrides the file, for when the file itself is broken.
 */
public final class EarlyConfig {
    public static final String LIBRARY_CHECK_CACHE_PATH = "gamma.startup.libraryCheckCache";
    public static final String LIBRARY_CHECK_CACHE_PROPERTY = "gammaengine.libraryCheckCache";

    private EarlyConfig() {
    }

    /**
     * Whether the library check may trust jars unchanged since the last check
     * ({@code -Dgammaengine.libraryCheckCache}, else {@code gamma.startup.libraryCheckCache}, else true).
     * Never throws: problems are reported on {@code out} and the default is kept.
     */
    public static boolean libraryCheckCache(Path configFile, PrintStream out) {
        return readSwitch(configFile, LIBRARY_CHECK_CACHE_PATH, LIBRARY_CHECK_CACHE_PROPERTY, true, out);
    }

    static boolean readSwitch(Path configFile, String path, String property, boolean fallback, PrintStream out) {
        String fromProperty = System.getProperty(property);
        if (fromProperty != null) {
            Boolean value = parseBoolean(fromProperty);
            if (value != null) {
                return value;
            }
            out.println("[GammaEngine] Ignoring -D" + property + "=" + fromProperty + ": expected true or false");
        }
        String raw;
        try {
            // ISO-8859-1 never fails to decode, and the keys and values read here are plain ASCII.
            raw = Files.isRegularFile(configFile)
                    ? find(Files.readAllLines(configFile, StandardCharsets.ISO_8859_1), path) : null;
        } catch (IOException | RuntimeException e) {
            out.println("[GammaEngine] Could not read " + configFile.getFileName() + " (" + e + "), "
                    + path + " stays " + fallback);
            return fallback;
        }
        if (raw == null) {
            return fallback;
        }
        Boolean value = parseBoolean(raw);
        if (value == null) {
            out.println("[GammaEngine] " + configFile.getFileName() + ": " + path + " is '" + raw
                    + "', expected true or false; using " + fallback);
            return fallback;
        }
        return value;
    }

    /** The YAML 1.1 booleans SnakeYAML accepts, in any case; null for anything else. */
    static Boolean parseBoolean(String value) {
        String lower = value.trim().toLowerCase(Locale.ROOT);
        if (lower.equals("true") || lower.equals("yes") || lower.equals("on")) {
            return Boolean.TRUE;
        }
        if (lower.equals("false") || lower.equals("no") || lower.equals("off")) {
            return Boolean.FALSE;
        }
        return null;
    }

    /**
     * Returns the raw scalar stored at {@code dottedPath} ("a.b.c" for {@code a:} / {@code b:} /
     * {@code c: value} nested by indentation), unquoted and without its comment, or null when absent.
     * The last occurrence wins, like SnakeYAML.
     */
    static String find(List<String> lines, String dottedPath) {
        List<Integer> indents = new ArrayList<>();
        List<String> keys = new ArrayList<>();
        String found = null;
        for (int n = 0; n < lines.size(); n++) {
            String line = lines.get(n);
            if (n == 0 && line.startsWith("ï»¿")) {
                line = line.substring(3); // UTF-8 byte order mark seen through ISO-8859-1
            }
            String trimmed = line.trim();
            if (trimmed.isEmpty() || trimmed.startsWith("#") || trimmed.startsWith("-") || trimmed.equals("...")) {
                continue;
            }
            int indent = 0;
            while (indent < line.length() && line.charAt(indent) == ' ') {
                indent++;
            }
            String key;
            String rest;
            char first = trimmed.charAt(0);
            if (first == '"' || first == '\'') {
                int close = trimmed.indexOf(first, 1);
                if (close < 0 || close + 1 >= trimmed.length() || trimmed.charAt(close + 1) != ':') {
                    continue;
                }
                key = trimmed.substring(1, close);
                rest = trimmed.substring(close + 2);
            } else {
                int colon = keyColon(trimmed);
                if (colon < 0) {
                    continue; // continuation of a multi-line value, not a key
                }
                key = trimmed.substring(0, colon).trim();
                rest = trimmed.substring(colon + 1);
            }
            while (!indents.isEmpty() && indents.get(indents.size() - 1) >= indent) {
                indents.remove(indents.size() - 1);
                keys.remove(keys.size() - 1);
            }
            indents.add(indent);
            keys.add(key);
            if (String.join(".", keys).equals(dottedPath)) {
                found = scalar(rest);
            }
        }
        return found;
    }

    /** Index of the colon that ends a plain key: followed by a space, a tab or the end of the line. */
    private static int keyColon(String trimmed) {
        for (int i = 0; i < trimmed.length(); i++) {
            if (trimmed.charAt(i) == ':' && (i + 1 == trimmed.length()
                    || trimmed.charAt(i + 1) == ' ' || trimmed.charAt(i + 1) == '\t')) {
                return i;
            }
        }
        return -1;
    }

    private static String scalar(String rest) {
        String value = rest.trim();
        if (!value.isEmpty() && (value.charAt(0) == '"' || value.charAt(0) == '\'')) {
            int close = value.indexOf(value.charAt(0), 1);
            return close < 0 ? value.substring(1) : value.substring(1, close);
        }
        int comment = value.indexOf(" #");
        if (comment < 0) {
            comment = value.indexOf("\t#");
        }
        return (comment < 0 ? value : value.substring(0, comment)).trim();
    }
}
