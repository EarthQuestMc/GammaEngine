package io.github.gammaengine.metrics;

import org.junit.Test;

import java.io.IOException;
import java.io.StringWriter;
import java.util.HashMap;
import java.util.Map;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertTrue;

/** Sorting and file formats of mods.csv, chunks.csv and the summary block. */
public class AttributionReportTest {

    private static AttributionReport.Owners owners(final Map<Class<?>, String> table) {
        return new AttributionReport.Owners() {
            @Override
            public String ownerOf(Class<?> type) {
                String owner = table.get(type);
                return owner == null ? OwnerResolver.UNKNOWN : owner;
            }
        };
    }

    private static AttributionTable sample() {
        AttributionTable table = new AttributionTable();
        // String: entity of "minecraft", 3 ticks, 3 ms in total.
        table.record(String.class, false, 0, 0, 0, 1_000_000);
        table.record(String.class, false, 0, 0, 0, 1_000_000);
        table.record(String.class, false, 0, 1, 0, 1_000_000);
        // Integer: tile entity of "thaumcraft", 2 ticks, 5 ms.
        table.record(Integer.class, true, 0, 1, 0, 2_000_000);
        table.record(Integer.class, true, -1, -2, 5, 3_000_000);
        // Long: entity of "thaumcraft", 1 tick, 0.5 ms.
        table.record(Long.class, false, 0, 0, 0, 500_000);
        return table;
    }

    private static Map<Class<?>, String> ownerTable() {
        Map<Class<?>, String> owners = new HashMap<Class<?>, String>();
        owners.put(String.class, "minecraft");
        owners.put(Integer.class, "thaumcraft");
        owners.put(Long.class, "thaumcraft");
        return owners;
    }

    @Test
    public void modsCsvIsSortedByTotalTime() throws IOException {
        AttributionReport report = AttributionReport.of(sample(), owners(ownerTable()), AttributionReport.CHUNK_ROWS);
        StringWriter out = new StringWriter();
        report.writeMods(out);
        assertEquals("owner,type,class,ticks,total_ms,mean_us\n"
                + "thaumcraft,tile_entity,java.lang.Integer,2,5.000,2500.000\n"
                + "minecraft,entity,java.lang.String,3,3.000,1000.000\n"
                + "thaumcraft,entity,java.lang.Long,1,0.500,500.000\n", out.toString());
    }

    @Test
    public void chunksCsvListsTheMostExpensiveChunksWithTheirCentre() throws IOException {
        AttributionReport report = AttributionReport.of(sample(), owners(ownerTable()), 2);
        assertEquals(3, report.chunkCount);
        StringWriter out = new StringWriter();
        report.writeChunks(out);
        // Chunk (0,0,0) holds 2.5 ms, (-1,-2,5) 3 ms, (0,1,0) 3 ms: ties are ordered by dimension.
        assertEquals("dimension,chunk_x,chunk_z,block_x,block_z,total_ms,entity_ticks,tile_entity_ticks\n"
                + "-1,-2,5,-24,88,3.000,0,1\n"
                + "0,1,0,24,8,3.000,1,1\n", out.toString());
    }

    @Test
    public void ownersAreAggregatedAcrossClasses() {
        AttributionReport report = AttributionReport.of(sample(), owners(ownerTable()), AttributionReport.CHUNK_ROWS);
        assertEquals(2, report.owners.size());
        assertEquals("thaumcraft", report.owners.get(0).owner);
        assertEquals(5_500_000, report.owners.get(0).nanos);
        assertEquals(3, report.owners.get(0).ticks);
        assertEquals("minecraft", report.owners.get(1).owner);
        assertEquals(8_500_000, report.totalNanos());
        assertEquals(6, report.objectTicks);
    }

    @Test
    public void summaryBlockNamesTheTopOwners() {
        AttributionReport report = AttributionReport.of(sample(), owners(ownerTable()), AttributionReport.CHUNK_ROWS);
        String json = report.json(17_000_000);
        assertTrue(json, json.startsWith("{\"enabled\": true, \"attributed_ms\": 8.5000, \"entity_ms\": 3.5000, "
                + "\"tile_entity_ms\": 5.0000, \"share_of_tick_time\": 0.5000, \"object_ticks\": 6, \"classes\": 3, "
                + "\"chunks\": 3, \"top_owners\": [\n"));
        assertTrue(json, json.contains("{\"owner\": \"thaumcraft\", \"total_ms\": 5.5000, \"ticks\": 3, \"share\": 0.6471}"));
        assertTrue(json, json.endsWith("\"share\": 0.3529}\n  ]}"));
        assertTrue(report.text(17_000_000), report.text(17_000_000).contains("(50.0% of tick time), top thaumcraft 64.7%"));
    }

    @Test
    public void emptyTableStillProducesValidFiles() throws IOException {
        AttributionReport report = AttributionReport.of(new AttributionTable(), owners(ownerTable()), 200);
        StringWriter mods = new StringWriter();
        report.writeMods(mods);
        assertEquals(AttributionReport.MODS_HEADER + "\n", mods.toString());
        assertTrue(report.json(0), report.json(0).contains("\"share_of_tick_time\": null"));
        assertTrue(report.json(0), report.json(0).endsWith("\"top_owners\": []}"));
    }

    @Test
    public void onlyFieldsThatNeedItAreQuoted() {
        assertEquals("BuildCraft|Core", AttributionReport.csvField("BuildCraft|Core"));
        assertEquals("plugin:My Plugin", AttributionReport.csvField("plugin:My Plugin"));
        assertEquals("\"a,b\"", AttributionReport.csvField("a,b"));
        assertEquals("\"say \"\"hi\"\"\"", AttributionReport.csvField("say \"hi\""));
    }
}
