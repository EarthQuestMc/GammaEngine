package io.github.gammaengine.util;

import io.github.gammaengine.GammaEngine;

/**
 * Decides what the console reader does with each line it reads.
 *
 * <p>A blank line is not a command. And on Windows, a server started with no console attached
 * (a service, a background job) gets an input stream whose every read fails; the console reader
 * turns each failure into an empty line, in a busy loop. Forwarded as commands, those lines
 * flooded the server thread with "Unknown command" and the first tick never finished.
 *
 * <p>Thread invariant: one instance per console reader thread, used by that thread only.
 */
public final class ConsoleInput {
    /** Blank lines in one second that no person can type: the input is dead. */
    private static final int DEAD_INPUT_BLANK_LINES = 1000;
    private static final long WINDOW_NANOS = 1_000_000_000L;

    private int blankLines;
    private long windowStart = System.nanoTime();

    /** True when the line should be dispatched as a command. */
    public boolean isCommand(String line) {
        return line != null && !line.trim().isEmpty();
    }

    /**
     * Called for every line that is not a command. True once the input has ended or produces blank
     * lines faster than anyone can type; the reader should then stop.
     */
    public boolean isDead(String line) {
        if (line == null) {
            GammaEngine.LOGGER.info("Console input ended: console commands are disabled until the next start.");
            return true;
        }
        long now = System.nanoTime();
        if (now - windowStart > WINDOW_NANOS) {
            windowStart = now;
            blankLines = 0;
        }
        if (++blankLines < DEAD_INPUT_BLANK_LINES) {
            return false;
        }
        // On Windows a closed pipe also reads as endless blank lines, so this is not always an error.
        GammaEngine.LOGGER.warn("Console input closed or unreadable: console commands are disabled until the next start. "
                + "Start the server with --noconsole to skip the console entirely.");
        return true;
    }
}
