package io.github.gammaengine.platform;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.Charset;
import java.util.HashSet;
import java.util.Set;

/**
 * CPU topology detection.
 *
 * <p>The scheduler must never confuse SMT siblings with physical cores: running eight heavy
 * region-tick workers on an 8c/16t machine is fast, running sixteen is slower because the two
 * threads of a core fight for the same execution ports and the same L1/L2. Everything that sizes
 * a pool for CPU-bound work therefore uses {@link #physicalCores()}, never
 * {@link #logicalProcessors()}.
 *
 * <p>Detection order:
 * <ol>
 *   <li>Linux {@code /sys/devices/system/cpu/cpuN/topology} (exact, the production target),</li>
 *   <li>Linux {@code /proc/cpuinfo} physical id / core id pairs (exact),</li>
 *   <li>a conservative heuristic (logical / 2 when the count looks like an SMT count).</li>
 * </ol>
 * The result can always be overridden by the administrator through the configuration file, and by
 * the system property {@code gammaengine.physicalCores} which is mostly meant for tests.
 */
public final class CpuTopology {
    private static final CpuTopology INSTANCE = detect();

    private final int logicalProcessors;
    private final int physicalCores;
    private final int packages;
    private final String source;

    CpuTopology(int logicalProcessors, int physicalCores, int packages, String source) {
        this.logicalProcessors = Math.max(1, logicalProcessors);
        this.physicalCores = Math.max(1, Math.min(physicalCores, this.logicalProcessors));
        this.packages = Math.max(1, packages);
        this.source = source;
    }

    public static CpuTopology get() {
        return INSTANCE;
    }

    /** Number of hardware threads the JVM can run on, SMT siblings included. */
    public int logicalProcessors() {
        return logicalProcessors;
    }

    /** Number of physical cores; the budget for CPU-bound simulation work. */
    public int physicalCores() {
        return physicalCores;
    }

    /** Number of CPU packages (sockets). Used as a NUMA hint. */
    public int packages() {
        return packages;
    }

    /** True when the machine exposes more hardware threads than physical cores. */
    public boolean hasSmt() {
        return logicalProcessors > physicalCores;
    }

    /** Where the numbers come from, for diagnostics. */
    public String source() {
        return source;
    }

    @Override
    public String toString() {
        return physicalCores + " physical core(s) / " + logicalProcessors + " logical processor(s), "
                + packages + " package(s) [" + source + "]";
    }

    private static CpuTopology detect() {
        int logical = Runtime.getRuntime().availableProcessors();

        String override = System.getProperty("gammaengine.physicalCores");
        if (override != null) {
            try {
                return new CpuTopology(logical, Integer.parseInt(override.trim()), 1, "system property");
            } catch (NumberFormatException ignored) {
                // fall through to real detection
            }
        }

        CpuTopology sysfs = fromSysfs(logical);
        if (sysfs != null) {
            return sysfs;
        }
        CpuTopology procinfo = fromProcCpuinfo(logical);
        if (procinfo != null) {
            return procinfo;
        }
        return heuristic(logical);
    }

    /**
     * Reads the per-CPU topology directory and counts the distinct (package, core) pairs. Offline
     * CPUs simply have no directory, which is the behaviour we want: a core the OS took away is
     * not a core we can schedule on.
     */
    private static CpuTopology fromSysfs(int logical) {
        File cpuDir = new File("/sys/devices/system/cpu");
        File[] cpus = cpuDir.listFiles();
        if (cpus == null) {
            return null;
        }
        Set<String> cores = new HashSet<String>();
        Set<String> packages = new HashSet<String>();
        for (File cpu : cpus) {
            String name = cpu.getName();
            if (!name.startsWith("cpu") || name.length() < 4 || !Character.isDigit(name.charAt(3))) {
                continue;
            }
            String pkg = readFirstLine(new File(cpu, "topology/physical_package_id"));
            String core = readFirstLine(new File(cpu, "topology/core_id"));
            if (pkg == null || core == null) {
                continue;
            }
            packages.add(pkg);
            cores.add(pkg + ":" + core);
        }
        if (cores.isEmpty()) {
            return null;
        }
        return new CpuTopology(logical, cores.size(), packages.size(), "sysfs");
    }

    /**
     * Fallback for kernels or containers without the sysfs topology tree. Note that inside a
     * container the procfs file describes the host, so the value is clamped to the number of
     * processors the JVM actually sees.
     */
    private static CpuTopology fromProcCpuinfo(int logical) {
        File file = new File("/proc/cpuinfo");
        if (!file.isFile()) {
            return null;
        }
        Set<String> cores = new HashSet<String>();
        Set<String> packages = new HashSet<String>();
        BufferedReader reader = null;
        try {
            reader = new BufferedReader(new InputStreamReader(new FileInputStream(file), Charset.forName("UTF-8")));
            String physicalId = "0";
            String coreId = null;
            String line;
            while ((line = reader.readLine()) != null) {
                int colon = line.indexOf(':');
                if (colon < 0) {
                    if (line.trim().isEmpty() && coreId != null) {
                        packages.add(physicalId);
                        cores.add(physicalId + ":" + coreId);
                        physicalId = "0";
                        coreId = null;
                    }
                    continue;
                }
                String key = line.substring(0, colon).trim();
                String value = line.substring(colon + 1).trim();
                if ("physical id".equals(key)) {
                    physicalId = value;
                } else if ("core id".equals(key)) {
                    coreId = value;
                }
            }
            if (coreId != null) {
                packages.add(physicalId);
                cores.add(physicalId + ":" + coreId);
            }
        } catch (IOException e) {
            return null;
        } finally {
            close(reader);
        }
        if (cores.isEmpty()) {
            return null;
        }
        return new CpuTopology(logical, Math.min(cores.size(), logical), packages.size(), "procfs");
    }

    /**
     * Last resort, used on Windows and on exotic platforms. Every server CPU sold in the last
     * fifteen years with an even hardware-thread count of four or more runs SMT, so assuming half
     * the logical processors are physical cores is safer than assuming no SMT at all: guessing too
     * few workers costs throughput, guessing too many costs latency on every tick.
     */
    private static CpuTopology heuristic(int logical) {
        int physical = (logical >= 4 && logical % 2 == 0) ? logical / 2 : logical;
        return new CpuTopology(logical, physical, 1, "heuristic");
    }

    private static String readFirstLine(File file) {
        if (!file.isFile()) {
            return null;
        }
        BufferedReader reader = null;
        try {
            reader = new BufferedReader(new InputStreamReader(new FileInputStream(file), Charset.forName("UTF-8")));
            String line = reader.readLine();
            return line == null ? null : line.trim();
        } catch (IOException e) {
            return null;
        } finally {
            close(reader);
        }
    }

    private static void close(BufferedReader reader) {
        if (reader != null) {
            try {
                reader.close();
            } catch (IOException ignored) {
                // nothing useful to do while closing a topology file
            }
        }
    }
}
