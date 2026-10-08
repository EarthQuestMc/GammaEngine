package io.github.gammaengine.metrics;

import java.io.IOException;
import java.io.Writer;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * What a recording with attribution writes when it ends: {@code mods.csv}, {@code chunks.csv} and
 * the {@code attribution} block of {@code summary.json}.
 *
 * <p>Built from a detached {@link AttributionTable}, on whatever thread stops the recording; it
 * never runs during a tick.
 */
final class AttributionReport {
    static final String MODS_HEADER = "owner,type,class,ticks,total_ms,mean_us";
    static final String CHUNKS_HEADER = "dimension,chunk_x,chunk_z,block_x,block_z,total_ms,entity_ticks,tile_entity_ticks";
    /** Rows of {@code chunks.csv}: the most expensive chunks only. */
    static final int CHUNK_ROWS = 200;
    /** Owners listed in {@code summary.json}. */
    static final int TOP_OWNERS = 10;

    /** Names the mod or plugin a class belongs to; {@link OwnerResolver} on a server. */
    interface Owners {
        String ownerOf(Class<?> type);
    }

    static final class ClassRow {
        final String owner;
        final boolean tile;
        final String className;
        final long ticks;
        final long nanos;

        ClassRow(String owner, boolean tile, String className, long ticks, long nanos) {
            this.owner = owner;
            this.tile = tile;
            this.className = className;
            this.ticks = ticks;
            this.nanos = nanos;
        }
    }

    static final class OwnerRow {
        final String owner;
        long ticks;
        long nanos;

        OwnerRow(String owner) {
            this.owner = owner;
        }
    }

    static final class ChunkRow {
        final int dimension;
        final int chunkX;
        final int chunkZ;
        final long nanos;
        final long entityTicks;
        final long tileTicks;

        ChunkRow(int dimension, int chunkX, int chunkZ, long nanos, long entityTicks, long tileTicks) {
            this.dimension = dimension;
            this.chunkX = chunkX;
            this.chunkZ = chunkZ;
            this.nanos = nanos;
            this.entityTicks = entityTicks;
            this.tileTicks = tileTicks;
        }
    }

    final List<ClassRow> classes;
    final List<OwnerRow> owners;
    final List<ChunkRow> chunks;
    final int chunkCount;
    final long entityNanos;
    final long tileNanos;
    final long objectTicks;

    private AttributionReport(List<ClassRow> classes, List<OwnerRow> owners, List<ChunkRow> chunks, int chunkCount,
                              long entityNanos, long tileNanos, long objectTicks) {
        this.classes = classes;
        this.owners = owners;
        this.chunks = chunks;
        this.chunkCount = chunkCount;
        this.entityNanos = entityNanos;
        this.tileNanos = tileNanos;
        this.objectTicks = objectTicks;
    }

    /** Sorts the table: classes and owners by total time, then the {@code chunkLimit} most expensive chunks. */
    static AttributionReport of(AttributionTable table, Owners resolver, int chunkLimit) {
        List<ClassRow> classes = new ArrayList<ClassRow>();
        Map<String, OwnerRow> byOwner = new HashMap<String, OwnerRow>();
        for (AttributionTable.ClassCost cost : table.classes()) {
            String owner = resolver.ownerOf(cost.type);
            classes.add(new ClassRow(owner, cost.tile, cost.type.getName(), cost.ticks, cost.nanos));
            OwnerRow row = byOwner.get(owner);
            if (row == null) {
                row = new OwnerRow(owner);
                byOwner.put(owner, row);
            }
            row.ticks += cost.ticks;
            row.nanos += cost.nanos;
        }
        Collections.sort(classes, new Comparator<ClassRow>() {
            @Override
            public int compare(ClassRow a, ClassRow b) {
                int byTime = Long.compare(b.nanos, a.nanos);
                if (byTime != 0) {
                    return byTime;
                }
                int byOwnerName = a.owner.compareTo(b.owner);
                return byOwnerName != 0 ? byOwnerName : a.className.compareTo(b.className);
            }
        });
        List<OwnerRow> owners = new ArrayList<OwnerRow>(byOwner.values());
        Collections.sort(owners, new Comparator<OwnerRow>() {
            @Override
            public int compare(OwnerRow a, OwnerRow b) {
                int byTime = Long.compare(b.nanos, a.nanos);
                return byTime != 0 ? byTime : a.owner.compareTo(b.owner);
            }
        });

        final List<ChunkRow> chunks = new ArrayList<ChunkRow>(table.chunkCount());
        table.forEachChunk(new AttributionTable.ChunkVisitor() {
            @Override
            public void chunk(int dimension, int chunkX, int chunkZ, long nanos, long entityTicks, long tileTicks) {
                chunks.add(new ChunkRow(dimension, chunkX, chunkZ, nanos, entityTicks, tileTicks));
            }
        });
        Collections.sort(chunks, new Comparator<ChunkRow>() {
            @Override
            public int compare(ChunkRow a, ChunkRow b) {
                int byTime = Long.compare(b.nanos, a.nanos);
                if (byTime != 0) {
                    return byTime;
                }
                if (a.dimension != b.dimension) {
                    return a.dimension < b.dimension ? -1 : 1;
                }
                if (a.chunkX != b.chunkX) {
                    return a.chunkX < b.chunkX ? -1 : 1;
                }
                return a.chunkZ < b.chunkZ ? -1 : (a.chunkZ == b.chunkZ ? 0 : 1);
            }
        });
        int chunkCount = chunks.size();
        List<ChunkRow> top = new ArrayList<ChunkRow>(chunks.subList(0, Math.min(chunkLimit, chunkCount)));
        return new AttributionReport(classes, owners, top, chunkCount, table.entityNanos(), table.tileNanos(),
                table.entityTicks() + table.tileTicks());
    }

    long totalNanos() {
        return entityNanos + tileNanos;
    }

    /** One row per class, by total time. */
    void writeMods(Writer out) throws IOException {
        out.write(MODS_HEADER);
        out.write('\n');
        for (ClassRow row : classes) {
            out.write(csvField(row.owner) + ',' + (row.tile ? "tile_entity" : "entity") + ',' + csvField(row.className)
                    + ',' + row.ticks + ',' + millis(row.nanos) + ','
                    + String.format(Locale.ROOT, "%.3f", row.ticks == 0 ? 0.0 : row.nanos / 1.0e3 / row.ticks) + '\n');
        }
    }

    /** The most expensive chunks; {@code block_x} and {@code block_z} are the centre of the chunk. */
    void writeChunks(Writer out) throws IOException {
        out.write(CHUNKS_HEADER);
        out.write('\n');
        for (ChunkRow row : chunks) {
            out.write(row.dimension + "," + row.chunkX + ',' + row.chunkZ + ',' + (row.chunkX * 16 + 8) + ','
                    + (row.chunkZ * 16 + 8) + ',' + millis(row.nanos) + ',' + row.entityTicks + ',' + row.tileTicks + '\n');
        }
    }

    /**
     * The {@code attribution} object of {@code summary.json}.
     *
     * @param tickNanos total duration of the recorded ticks, for the share of tick time attributed
     */
    String json(long tickNanos) {
        StringBuilder json = new StringBuilder(512);
        json.append("{\"enabled\": true")
                .append(", \"attributed_ms\": ").append(number(totalNanos() / 1.0e6))
                .append(", \"entity_ms\": ").append(number(entityNanos / 1.0e6))
                .append(", \"tile_entity_ms\": ").append(number(tileNanos / 1.0e6))
                .append(", \"share_of_tick_time\": ").append(tickNanos > 0 ? number((double) totalNanos() / tickNanos) : "null")
                .append(", \"object_ticks\": ").append(objectTicks)
                .append(", \"classes\": ").append(classes.size())
                .append(", \"chunks\": ").append(chunkCount)
                .append(", \"top_owners\": [");
        int listed = Math.min(TOP_OWNERS, owners.size());
        for (int i = 0; i < listed; i++) {
            OwnerRow owner = owners.get(i);
            json.append(i == 0 ? "\n" : ",\n").append("    {\"owner\": ").append(Recording.quote(owner.owner))
                    .append(", \"total_ms\": ").append(number(owner.nanos / 1.0e6))
                    .append(", \"ticks\": ").append(owner.ticks)
                    .append(", \"share\": ").append(number(totalNanos() > 0 ? (double) owner.nanos / totalNanos() : 0.0))
                    .append('}');
        }
        return json.append(listed > 0 ? "\n  ]}" : "]}").toString();
    }

    /** One line for the console. */
    String text(long tickNanos) {
        StringBuilder text = new StringBuilder(128);
        text.append(String.format(Locale.ROOT, "attribution %.1f ms over %d object ticks", totalNanos() / 1.0e6, objectTicks));
        if (tickNanos > 0) {
            text.append(String.format(Locale.ROOT, " (%.1f%% of tick time)", 100.0 * totalNanos() / tickNanos));
        }
        if (!owners.isEmpty() && totalNanos() > 0) {
            OwnerRow first = owners.get(0);
            text.append(String.format(Locale.ROOT, ", top %s %.1f%%", first.owner, 100.0 * first.nanos / totalNanos()));
        }
        return text.toString();
    }

    static String millis(long nanos) {
        return String.format(Locale.ROOT, "%.3f", nanos / 1.0e6);
    }

    private static String number(double value) {
        return String.format(Locale.ROOT, "%.4f", value);
    }

    /** RFC 4180: quoted only when the value holds a comma, a quote or a line break. */
    static String csvField(String value) {
        if (value.indexOf(',') < 0 && value.indexOf('"') < 0 && value.indexOf('\n') < 0 && value.indexOf('\r') < 0) {
            return value;
        }
        return '"' + value.replace("\"", "\"\"") + '"';
    }
}
