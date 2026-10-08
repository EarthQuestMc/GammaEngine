package io.github.gammaengine.util;

import java.io.PrintStream;

/**
 * The startup banner and the coloured console helpers.
 *
 * <p>Console output is the only interface a server operator has during boot, and a wall of
 * undifferentiated white text hides the two things they actually look for: which build is running
 * and whether anything went wrong. The banner names the build, and the colour helpers let the rest
 * of the engine mark a value, a warning and a heading differently.
 *
 * <p>Colour is optional by design. It is disabled by {@code -Dgammaengine.noColor=true}, and the
 * box-drawing banner falls back to plain ASCII when the console encoding cannot carry it, because a
 * banner rendered as question marks is worse than no banner.
 */
public final class ConsoleBanner {
    private static final char ESC = (char) 27;
    public static final String RESET = ESC + "[0m";
    public static final String BOLD = ESC + "[1m";
    public static final String DIM = ESC + "[2m";
    public static final String RED = ESC + "[31m";
    public static final String GREEN = ESC + "[32m";
    public static final String YELLOW = ESC + "[33m";
    public static final String BLUE = ESC + "[34m";
    public static final String MAGENTA = ESC + "[35m";
    public static final String CYAN = ESC + "[36m";
    public static final String WHITE = ESC + "[37m";
    public static final String BRIGHT_CYAN = ESC + "[96m";
    public static final String BRIGHT_BLUE = ESC + "[94m";
    public static final String BRIGHT_MAGENTA = ESC + "[95m";

    private static final String[] ART_UNICODE = {
            " \u2588\u2588\u2588\u2588\u2588\u2588\u2557  \u2588\u2588\u2588\u2588\u2588\u2557 \u2588\u2588\u2588\u2557   \u2588\u2588\u2588\u2557\u2588\u2588\u2588\u2557   \u2588\u2588\u2588\u2557 \u2588\u2588\u2588\u2588\u2588\u2557     \u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2557\u2588\u2588\u2588\u2557   \u2588\u2588\u2557 \u2588\u2588\u2588\u2588\u2588\u2588\u2557 \u2588\u2588\u2557\u2588\u2588\u2588\u2557   \u2588\u2588\u2557\u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2557",
            "\u2588\u2588\u2554\u2550\u2550\u2550\u2550\u255d \u2588\u2588\u2554\u2550\u2550\u2588\u2588\u2557\u2588\u2588\u2588\u2588\u2557 \u2588\u2588\u2588\u2588\u2551\u2588\u2588\u2588\u2588\u2557 \u2588\u2588\u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u2588\u2588\u2557    \u2588\u2588\u2554\u2550\u2550\u2550\u2550\u255d\u2588\u2588\u2588\u2588\u2557  \u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u2550\u2550\u255d \u2588\u2588\u2551\u2588\u2588\u2588\u2588\u2557  \u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u2550\u2550\u255d",
            "\u2588\u2588\u2551  \u2588\u2588\u2588\u2557\u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2551\u2588\u2588\u2554\u2588\u2588\u2588\u2588\u2554\u2588\u2588\u2551\u2588\u2588\u2554\u2588\u2588\u2588\u2588\u2554\u2588\u2588\u2551\u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2551    \u2588\u2588\u2588\u2588\u2588\u2557  \u2588\u2588\u2554\u2588\u2588\u2557 \u2588\u2588\u2551\u2588\u2588\u2551  \u2588\u2588\u2588\u2557\u2588\u2588\u2551\u2588\u2588\u2554\u2588\u2588\u2557 \u2588\u2588\u2551\u2588\u2588\u2588\u2588\u2588\u2557  ",
            "\u2588\u2588\u2551   \u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u2588\u2588\u2551\u2588\u2588\u2551\u255a\u2588\u2588\u2554\u255d\u2588\u2588\u2551\u2588\u2588\u2551\u255a\u2588\u2588\u2554\u255d\u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u2588\u2588\u2551    \u2588\u2588\u2554\u2550\u2550\u255d  \u2588\u2588\u2551\u255a\u2588\u2588\u2557\u2588\u2588\u2551\u2588\u2588\u2551   \u2588\u2588\u2551\u2588\u2588\u2551\u2588\u2588\u2551\u255a\u2588\u2588\u2557\u2588\u2588\u2551\u2588\u2588\u2554\u2550\u2550\u255d  ",
            "\u255a\u2588\u2588\u2588\u2588\u2588\u2588\u2554\u255d\u2588\u2588\u2551  \u2588\u2588\u2551\u2588\u2588\u2551 \u255a\u2550\u255d \u2588\u2588\u2551\u2588\u2588\u2551 \u255a\u2550\u255d \u2588\u2588\u2551\u2588\u2588\u2551  \u2588\u2588\u2551    \u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2557\u2588\u2588\u2551 \u255a\u2588\u2588\u2588\u2588\u2551\u255a\u2588\u2588\u2588\u2588\u2588\u2588\u2554\u255d\u2588\u2588\u2551\u2588\u2588\u2551 \u255a\u2588\u2588\u2588\u2588\u2551\u2588\u2588\u2588\u2588\u2588\u2588\u2588\u2557",
            " \u255a\u2550\u2550\u2550\u2550\u2550\u255d \u255a\u2550\u255d  \u255a\u2550\u255d\u255a\u2550\u255d     \u255a\u2550\u255d\u255a\u2550\u255d     \u255a\u2550\u255d\u255a\u2550\u255d  \u255a\u2550\u255d    \u255a\u2550\u2550\u2550\u2550\u2550\u2550\u255d\u255a\u2550\u255d  \u255a\u2550\u2550\u2550\u255d \u255a\u2550\u2550\u2550\u2550\u2550\u255d \u255a\u2550\u255d\u255a\u2550\u255d  \u255a\u2550\u2550\u2550\u255d\u255a\u2550\u2550\u2550\u2550\u2550\u2550\u255d"
    };

    private static final String[] ART_ASCII = {
            "  _____                                 ______             _            ",
            " / ____|                               |  ____|           (_)           ",
            "| |  __  __ _ _ __ ___  _ __ ___   __ _| |__   _ __   __ _ _ _ __   ___ ",
            "| | |_ |/ _` | '_ ` _ \\| '_ ` _ \\ / _` |  __| | '_ \\ / _` | | '_ \\ / _ \\",
            "| |__| | (_| | | | | | | | | | | | (_| | |____| | | | (_| | | | | |  __/",
            " \\_____|\\__,_|_| |_| |_|_| |_| |_|\\__,_|______|_| |_|\\__, |_|_| |_|\\___|",
            "                                                      __/ |             ",
            "                                                     |___/              "
    };

    private static final Boolean COLOR = detectColor();

    private ConsoleBanner() {
    }

    /** Wraps text in a colour, or returns it unchanged when colour is off. */
    public static String colour(String colour, String text) {
        return COLOR ? colour + text + RESET : text;
    }

    public static boolean colourEnabled() {
        return COLOR;
    }

    /**
     * Prints the startup banner.
     *
     * @param version    build version, shown under the art
     * @param subtitle   one line describing what this build is
     */
    public static void print(PrintStream out, String version, String subtitle) {
        String[] art = supportsUnicode() ? ART_UNICODE : ART_ASCII;
        String[] gradient = {BRIGHT_CYAN, BRIGHT_CYAN, CYAN, CYAN, BRIGHT_BLUE, BRIGHT_BLUE, BLUE, BLUE};

        out.println();
        for (int i = 0; i < art.length; i++) {
            out.println(colour(gradient[Math.min(i, gradient.length - 1)], art[i]));
        }
        out.println(colour(BRIGHT_MAGENTA, "   " + subtitle));
        out.println(colour(DIM, "   " + version));
        out.println();
    }

    /**
     * Prints a framed summary. Used once the server is ready, so the numbers an operator needs are
     * in one block instead of scattered over two hundred log lines.
     */
    public static void printBox(PrintStream out, String title, String[] lines) {
        int width = title.length();
        for (String line : lines) {
            width = Math.max(width, visibleLength(line));
        }
        width += 2;

        String horizontal = repeat('-', width + 2);
        out.println(colour(CYAN, "+" + horizontal + "+"));
        out.println(colour(CYAN, "| ") + colour(BOLD + WHITE, pad(title, width)) + colour(CYAN, " |"));
        out.println(colour(CYAN, "+" + horizontal + "+"));
        for (String line : lines) {
            out.println(colour(CYAN, "| ") + pad(line, width) + colour(CYAN, " |"));
        }
        out.println(colour(CYAN, "+" + horizontal + "+"));
    }

    private static String pad(String text, int width) {
        int padding = width - visibleLength(text);
        return padding <= 0 ? text : text + repeat(' ', padding);
    }

    /** Length ignoring ANSI escapes, so coloured values still line up inside the frame. */
    private static int visibleLength(String text) {
        int length = 0;
        boolean inEscape = false;
        for (int i = 0; i < text.length(); i++) {
            char character = text.charAt(i);
            if (inEscape) {
                if (character == 'm') {
                    inEscape = false;
                }
            } else if (character == ESC) {
                inEscape = true;
            } else {
                length++;
            }
        }
        return length;
    }

    private static String repeat(char character, int count) {
        StringBuilder builder = new StringBuilder(Math.max(0, count));
        for (int i = 0; i < count; i++) {
            builder.append(character);
        }
        return builder.toString();
    }

    private static boolean supportsUnicode() {
        return stdoutEncoding().toUpperCase().contains("UTF");
    }

    /**
     * The encoding {@code System.out} writes with. From Java 19 it is {@code stdout.encoding}, which
     * ignores {@code file.encoding}; before, {@code sun.stdout.encoding} when a Windows console is
     * attached, {@code file.encoding} otherwise.
     */
    private static String stdoutEncoding() {
        String encoding = System.getProperty(javaMajor() >= 19 ? "stdout.encoding" : "sun.stdout.encoding");
        return encoding != null ? encoding : System.getProperty("file.encoding", "");
    }

    private static int javaMajor() {
        String version = System.getProperty("java.specification.version", "1.8");
        try {
            return Integer.parseInt(version.startsWith("1.") ? version.substring(2) : version);
        } catch (NumberFormatException e) {
            return 8;
        }
    }

    private static Boolean detectColor() {
        if (Boolean.getBoolean("gammaengine.noColor")) {
            return Boolean.FALSE;
        }
        // Windows consoles older than Windows 10 do not understand ANSI at all, and a log file that
        // captured escape codes is unreadable. Everything else, including every hosting panel that
        // renders a console, handles them.
        String os = System.getProperty("os.name", "").toLowerCase();
        if (os.contains("windows")) {
            String version = System.getProperty("os.version", "0");
            try {
                if (Double.parseDouble(version.split("\\.")[0]) < 10) {
                    return Boolean.FALSE;
                }
            } catch (NumberFormatException ignored) {
                // Unknown Windows version: assume it is recent enough.
            }
        }
        return Boolean.TRUE;
    }
}
