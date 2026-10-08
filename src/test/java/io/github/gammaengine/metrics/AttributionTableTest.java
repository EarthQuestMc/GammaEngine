package io.github.gammaengine.metrics;

import org.junit.Test;

import java.util.HashMap;
import java.util.Map;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/** Sums of the level 2 probe, without a server: classes stand in for entity and tile entity types. */
public class AttributionTableTest {

    @Test
    public void ticksAreSummedByClass() {
        AttributionTable table = new AttributionTable();
        table.record(String.class, false, 0, 1, 1, 100);
        table.record(String.class, false, 0, 2, 2, 300);
        table.record(Integer.class, true, 0, 1, 1, 50);

        Map<Class<?>, AttributionTable.ClassCost> byClass = new HashMap<Class<?>, AttributionTable.ClassCost>();
        for (AttributionTable.ClassCost cost : table.classes()) {
            byClass.put(cost.type, cost);
        }
        assertEquals(2, byClass.size());
        assertEquals(400, byClass.get(String.class).nanos);
        assertEquals(2, byClass.get(String.class).ticks);
        assertFalse(byClass.get(String.class).tile);
        assertEquals(50, byClass.get(Integer.class).nanos);
        assertTrue(byClass.get(Integer.class).tile);

        assertEquals(400, table.entityNanos());
        assertEquals(50, table.tileNanos());
        assertEquals(2, table.entityTicks());
        assertEquals(1, table.tileTicks());
    }

    @Test
    public void chunksAreKeptApartByDimension() {
        AttributionTable table = new AttributionTable();
        table.record(String.class, false, 0, 3, -4, 10);
        table.record(String.class, true, 0, 3, -4, 20);
        table.record(String.class, false, -1, 3, -4, 40);
        table.record(String.class, false, 0, 3, -4, 5);

        final Map<String, long[]> chunks = new HashMap<String, long[]>();
        table.forEachChunk(new AttributionTable.ChunkVisitor() {
            @Override
            public void chunk(int dimension, int chunkX, int chunkZ, long nanos, long entityTicks, long tileTicks) {
                chunks.put(dimension + ":" + chunkX + ":" + chunkZ, new long[]{nanos, entityTicks, tileTicks});
            }
        });
        assertEquals(2, chunks.size());
        assertEquals(2, table.chunkCount());
        long[] overworld = chunks.get("0:3:-4");
        assertEquals(35, overworld[0]);
        assertEquals(2, overworld[1]);
        assertEquals(1, overworld[2]);
        long[] nether = chunks.get("-1:3:-4");
        assertEquals(40, nether[0]);
        assertEquals(1, nether[1]);
        assertEquals(0, nether[2]);
    }

    @Test
    public void chunkKeysRoundTripNegativeAndExtremeCoordinates() {
        int[] values = {0, 1, -1, 1875000, -1875000, Integer.MAX_VALUE, Integer.MIN_VALUE};
        for (int x : values) {
            for (int z : values) {
                long key = AttributionTable.chunkKey(x, z);
                assertEquals(x, AttributionTable.chunkX(key));
                assertEquals(z, AttributionTable.chunkZ(key));
            }
        }
        assertFalse(AttributionTable.chunkKey(1, -1) == AttributionTable.chunkKey(-1, 1));
    }

    @Test
    public void tablesGrowPastTheirInitialSize() {
        AttributionTable table = new AttributionTable();
        for (int pass = 0; pass < 2; pass++) {
            for (int x = -500; x < 500; x++) {
                table.record(Long.class, false, 7, x, x * 3, 2);
            }
        }
        assertEquals(1000, table.chunkCount());
        final long[] total = new long[2];
        table.forEachChunk(new AttributionTable.ChunkVisitor() {
            @Override
            public void chunk(int dimension, int chunkX, int chunkZ, long nanos, long entityTicks, long tileTicks) {
                assertEquals(7, dimension);
                assertEquals(chunkX * 3, chunkZ);
                assertEquals(4, nanos);
                total[0] += nanos;
                total[1] += entityTicks;
            }
        });
        assertEquals(4000, total[0]);
        assertEquals(2000, total[1]);
    }
}
