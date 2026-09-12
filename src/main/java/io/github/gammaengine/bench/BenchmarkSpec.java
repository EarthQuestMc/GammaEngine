package io.github.gammaengine.bench;

/**
 * Parameters of one benchmark run.
 *
 * <p>Phase 1 cannot be done honestly without a load: an empty world ticks in 0.2 ms and every
 * optimisation looks like noise. This describes a reproducible synthetic load, built from the same
 * seed and the same commands every time, so two runs of the server can be compared.
 */
public final class BenchmarkSpec {
    private int chunkRadius = 8;
    private int entities = 500;
    private int tileEntities = 500;
    private int ticks = 600;
    private String name = "default";

    /** Radius, in chunks, of the square kept loaded around the world spawn. */
    public int chunkRadius() {
        return chunkRadius;
    }

    /** Number of living entities spawned across the loaded area. */
    public int entities() {
        return entities;
    }

    /** Number of ticking tile entities placed across the loaded area. */
    public int tileEntities() {
        return tileEntities;
    }

    /** Duration of the measured run, in server ticks. */
    public int ticks() {
        return ticks;
    }

    public String name() {
        return name;
    }

    /** Number of chunks the spec keeps loaded. */
    public int chunkCount() {
        int side = chunkRadius * 2 + 1;
        return side * side;
    }

    /**
     * Parses {@code key=value} arguments, ignoring anything unrecognised so a typo degrades into a
     * default rather than into a failed benchmark.
     */
    public static BenchmarkSpec parse(String[] args, int from) {
        BenchmarkSpec spec = new BenchmarkSpec();
        for (int i = from; i < args.length; i++) {
            String argument = args[i];
            int equals = argument.indexOf('=');
            if (equals <= 0) {
                continue;
            }
            String key = argument.substring(0, equals).toLowerCase();
            String value = argument.substring(equals + 1);
            if ("chunks".equals(key) || "radius".equals(key)) {
                spec.chunkRadius = clamp(parseInt(value, spec.chunkRadius), 0, 32);
            } else if ("entities".equals(key)) {
                spec.entities = clamp(parseInt(value, spec.entities), 0, 100_000);
            } else if ("tiles".equals(key)) {
                spec.tileEntities = clamp(parseInt(value, spec.tileEntities), 0, 100_000);
            } else if ("ticks".equals(key)) {
                spec.ticks = clamp(parseInt(value, spec.ticks), 20, 200_000);
            } else if ("name".equals(key)) {
                spec.name = value;
            }
        }
        return spec;
    }

    private static int parseInt(String value, int fallback) {
        try {
            return Integer.parseInt(value.trim());
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    private static int clamp(int value, int min, int max) {
        return Math.max(min, Math.min(max, value));
    }

    @Override
    public String toString() {
        return String.format("%s: %d chunks (radius %d), %d entities, %d tile entities, %d ticks",
                name, chunkCount(), chunkRadius, entities, tileEntities, ticks);
    }
}
