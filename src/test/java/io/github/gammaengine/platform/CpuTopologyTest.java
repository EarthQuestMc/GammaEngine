package io.github.gammaengine.platform;

import org.junit.Test;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

public class CpuTopologyTest {

    @Test
    public void detectionProducesUsableNumbers() {
        CpuTopology topology = CpuTopology.get();
        assertTrue(topology.logicalProcessors() >= 1);
        assertTrue(topology.physicalCores() >= 1);
        assertTrue("physical cores cannot exceed logical processors",
                topology.physicalCores() <= topology.logicalProcessors());
        assertTrue(topology.packages() >= 1);
        assertFalse(topology.source().isEmpty());
    }

    @Test
    public void physicalCoresAreNeverClampedAboveLogicalProcessors() {
        // A bad override must degrade to something safe rather than oversubscribe the machine.
        CpuTopology topology = new CpuTopology(8, 64, 1, "test");
        assertEquals(8, topology.physicalCores());
        assertFalse(topology.hasSmt());
    }

    @Test
    public void smtIsReportedWhenLogicalExceedsPhysical() {
        CpuTopology topology = new CpuTopology(16, 8, 1, "test");
        assertTrue(topology.hasSmt());
        assertEquals(8, topology.physicalCores());
        assertEquals(16, topology.logicalProcessors());
    }

    @Test
    public void degenerateValuesAreRaisedToOne() {
        CpuTopology topology = new CpuTopology(0, 0, 0, "test");
        assertEquals(1, topology.logicalProcessors());
        assertEquals(1, topology.physicalCores());
        assertEquals(1, topology.packages());
    }
}
