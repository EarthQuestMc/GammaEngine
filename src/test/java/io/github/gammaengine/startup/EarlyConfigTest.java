package io.github.gammaengine.startup;

import org.junit.After;
import org.junit.Before;
import org.junit.Test;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;
import java.util.List;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertNull;
import static org.junit.Assert.assertTrue;

public class EarlyConfigTest {
    private static final String PATH = EarlyConfig.LIBRARY_CHECK_CACHE_PATH;
    private static final String PROPERTY = EarlyConfig.LIBRARY_CHECK_CACHE_PROPERTY;

    private Path file;
    private ByteArrayOutputStream output;
    private PrintStream out;

    @Before
    public void setUp() throws IOException {
        file = Files.createTempFile("gammaengine", ".yml");
        output = new ByteArrayOutputStream();
        out = new PrintStream(output, true);
        System.clearProperty(PROPERTY);
    }

    @After
    public void tearDown() throws IOException {
        System.clearProperty(PROPERTY);
        Files.deleteIfExists(file);
    }

    /** The shape GammaConfig writes: header comments, nested sections, comments above each key. */
    private static List<String> generated(String value) {
        return Arrays.asList(
                "# GammaEngine configuration",
                "",
                "gamma:",
                "  network:",
                "    # Deflate level used for chunk packets sent to clients, 1 to 9.",
                "    chunkCompressionLevel: 4",
                "  startup:",
                "    # Radius, in chunks",
                "    spawnRadiusChunks: 0",
                "    # Startup library check: remember the size, date and MD5 of every library jar: with colons",
                "    libraryCheckCache: " + value,
                "  native:",
                "    enabled: true");
    }

    @Test
    public void findsTheNestedKey() {
        assertEquals("false", EarlyConfig.find(generated("false"), PATH));
        assertEquals("4", EarlyConfig.find(generated("false"), "gamma.network.chunkCompressionLevel"));
        assertEquals("true", EarlyConfig.find(generated("false"), "gamma.native.enabled"));
    }

    @Test
    public void missingKeyIsNull() {
        assertNull(EarlyConfig.find(generated("false"), "gamma.startup.other"));
        assertNull(EarlyConfig.find(Arrays.asList("gamma:", "  native:", "    enabled: true"), PATH));
        assertNull(EarlyConfig.find(Arrays.<String>asList(), PATH));
    }

    @Test
    public void sameNameUnderAnotherSectionIsNotTaken() {
        List<String> lines = Arrays.asList(
                "gamma:",
                "  other:",
                "    libraryCheckCache: false",
                "  startup:",
                "    spawnRadiusChunks: 0",
                "libraryCheckCache: false",
                "startup:",
                "  libraryCheckCache: false");
        assertNull(EarlyConfig.find(lines, PATH));
    }

    @Test
    public void quotesCommentsAndOddIndentationAreHandled() {
        assertEquals("off", EarlyConfig.find(Arrays.asList(
                "ï»¿gamma:", "   startup:", "         libraryCheckCache: off   # turned off"), PATH));
        assertEquals("no", EarlyConfig.find(Arrays.asList(
                "gamma:", "  'startup':", "    \"libraryCheckCache\": \"no\" # quoted"), PATH));
        assertEquals("flat dotted key", "false", EarlyConfig.find(Arrays.asList("gamma.startup.libraryCheckCache: false"), PATH));
        assertEquals("last occurrence wins", "true", EarlyConfig.find(Arrays.asList(
                "gamma:", "  startup:", "    libraryCheckCache: false", "    libraryCheckCache: true"), PATH));
    }

    @Test
    public void booleansAreTheYamlOnes() {
        for (String yes : Arrays.asList("true", "True", "TRUE", "yes", "on", " true ")) {
            assertEquals(yes, Boolean.TRUE, EarlyConfig.parseBoolean(yes));
        }
        for (String no : Arrays.asList("false", "False", "no", "OFF")) {
            assertEquals(no, Boolean.FALSE, EarlyConfig.parseBoolean(no));
        }
        for (String neither : Arrays.asList("", "0", "1", "maybe", "flase")) {
            assertNull(neither, EarlyConfig.parseBoolean(neither));
        }
    }

    @Test
    public void defaultsToOnWithoutFile() throws IOException {
        Files.delete(file);
        assertTrue(EarlyConfig.libraryCheckCache(file, out));
        assertEquals("", output.toString());
    }

    @Test
    public void fileTurnsItOff() throws IOException {
        Files.write(file, generated("false"), StandardCharsets.UTF_8);
        assertFalse(EarlyConfig.libraryCheckCache(file, out));
    }

    @Test
    public void invalidValueKeepsTheDefaultAndSaysSo() throws IOException {
        Files.write(file, generated("sometimes"), StandardCharsets.UTF_8);
        assertTrue(EarlyConfig.libraryCheckCache(file, out));
        assertTrue(output.toString().contains("sometimes"));
    }

    @Test
    public void unreadableFileKeepsTheDefault() throws IOException {
        Files.write(file, new byte[]{0, (byte) 0xFF, (byte) 0xFE, ':', '\n', '\t', '-'});
        assertTrue(EarlyConfig.libraryCheckCache(file, out));
    }

    @Test
    public void propertyOverridesTheFile() throws IOException {
        Files.write(file, generated("true"), StandardCharsets.UTF_8);
        System.setProperty(PROPERTY, "false");
        assertFalse(EarlyConfig.libraryCheckCache(file, out));

        Files.write(file, generated("false"), StandardCharsets.UTF_8);
        System.setProperty(PROPERTY, "true");
        assertTrue(EarlyConfig.libraryCheckCache(file, out));

        System.setProperty(PROPERTY, "garbage");
        assertFalse("an unusable property falls through to the file", EarlyConfig.libraryCheckCache(file, out));
        assertTrue(output.toString().contains("garbage"));
    }
}
