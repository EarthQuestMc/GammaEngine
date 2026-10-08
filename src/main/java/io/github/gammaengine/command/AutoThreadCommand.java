package io.github.gammaengine.command;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.autothread.AutoThreadRuntime;
import io.github.gammaengine.bench.BenchmarkSpec;
import io.github.gammaengine.bench.WorldBenchmark;
import io.github.gammaengine.config.GammaConfig;
import io.github.gammaengine.metrics.BenchRecorder;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.LatencyHistogram;
import io.github.gammaengine.profiler.TickStatistics;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

import java.io.File;
import java.util.List;
import java.util.Locale;

/**
 * {@code /autothread}: the operator-facing window into the runtime.
 *
 * <p>The command answers questions in the order an administrator actually asks them: is the server
 * healthy ({@code status}), where is the time going ({@code profile}), and where is the memory
 * ({@code memory}).
 */
public class AutoThreadCommand extends Command {
    public AutoThreadCommand() {
        super("autothread");
        setDescription("Inspect and control the " + GammaEngine.NAME + " AutoThread runtime");
        setUsage(ChatColor.translateAlternateColorCodes('&',
                "&7&m----------------&7[&bAutoThread&7]&m----------------\n"
                        + "&b  >&e /autothread status &7-&a runtime state, TPS and MSPT.\n"
                        + "&b  >&e /autothread native &7-&a native engine state.\n"
                        + "&b  >&e /autothread profile start|stop|report &7-&a profiling session.\n"
                        + "&b  >&e /autothread bench start|stop &7-&a synthetic load benchmark.\n"
                        + "&b  >&e /autothread record start [name] [attribution]|stop|status &7-&a record every tick to gammaengine/bench.\n"
                        + "&b  >&e /autothread memory [gc] &7-&a heap, off-heap and world memory."));
        setPermission("gammaengine.autothread");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        if (args.length == 0) {
            sender.sendMessage(usageMessage);
            return true;
        }

        String action = args[0].toLowerCase();
        if ("status".equals(action)) {
            sendLines(sender, AutoThreadRuntime.get().statusText());
        } else if ("native".equals(action)) {
            nativeEngine(sender);
        } else if ("profile".equals(action)) {
            profile(sender, args);
        } else if ("bench".equals(action)) {
            bench(sender, args);
        } else if ("record".equals(action)) {
            record(sender, args);
        } else if ("memory".equals(action) || "mem".equals(action)) {
            boolean collect = args.length > 1 && ("gc".equalsIgnoreCase(args[1]) || "collect".equalsIgnoreCase(args[1]));
            sendLines(sender, io.github.gammaengine.diag.MemoryReport.text(collect));
        } else {
            sender.sendMessage(usageMessage);
        }
        return true;
    }

    private void nativeEngine(CommandSender sender) {
        sender.sendMessage(ChatColor.AQUA + "Native engine:");
        sender.sendMessage(ChatColor.GRAY + "  " + io.github.gammaengine.nativeengine.NativeEngine.get().statusText());
    }

    private void profile(CommandSender sender, String[] args) {
        GammaProfiler profiler = GammaProfiler.get();
        String sub = args.length > 1 ? args[1].toLowerCase() : "";
        if ("start".equals(sub)) {
            if (profiler.startSession()) {
                sender.sendMessage(ChatColor.GREEN + "Profiling session started, every metric was reset.");
            } else {
                sender.sendMessage(ChatColor.YELLOW + "A profiling session is already running.");
            }
        } else if ("stop".equals(sub)) {
            String report = profiler.stopSession();
            if (report == null) {
                sender.sendMessage(ChatColor.YELLOW + "No profiling session is running.");
                return;
            }
            File file = profiler.writeReport(report);
            sender.sendMessage(ChatColor.GREEN + "Profiling session stopped."
                    + (file == null ? "" : " Report written to " + file.getPath()));
            sendLines(sender, report);
        } else if ("report".equals(sub)) {
            String report = profiler.sessionActive() ? profiler.buildReport() : profiler.lastReport();
            if (report == null) {
                sender.sendMessage(ChatColor.YELLOW + "No report available yet, run /autothread profile start first.");
                return;
            }
            sendLines(sender, report);
        } else {
            sender.sendMessage(ChatColor.GRAY + "Usage: /autothread profile <start|stop|report>");
        }
    }

    /**
     * Runs the synthetic-load benchmark. The load is built and measured on the server thread, so
     * the command returns immediately and the report arrives when the run ends.
     */
    private void bench(final CommandSender sender, String[] args) {
        String sub = args.length > 1 ? args[1].toLowerCase() : "";
        if ("stop".equals(sub) || "cancel".equals(sub)) {
            WorldBenchmark.get().cancel();
            sender.sendMessage(ChatColor.GREEN + "Benchmark cancelled.");
            return;
        }
        if (!"start".equals(sub)) {
            sender.sendMessage(ChatColor.GRAY
                    + "Usage: /autothread bench start [chunks=8] [entities=500] [tiles=500] [ticks=600] [name=run]");
            sender.sendMessage(ChatColor.GRAY + "       /autothread bench stop");
            return;
        }

        BenchmarkSpec spec = BenchmarkSpec.parse(args, 2);
        String error = WorldBenchmark.get().start(spec, new WorldBenchmark.Listener() {
            @Override
            public void message(String line) {
                sender.sendMessage(ChatColor.GRAY + line);
            }
        });
        if (error != null) {
            sender.sendMessage(ChatColor.RED + error);
        }
    }

    /**
     * Tick and GC recording for the bench: the orchestrator drives it from the server console.
     * {@code attribution} anywhere after {@code start} turns on level 2 for this recording; the first
     * other argument is the name.
     */
    private void record(CommandSender sender, String[] args) {
        BenchRecorder recorder = BenchRecorder.get();
        String sub = args.length > 1 ? args[1].toLowerCase() : "status";
        if ("start".equals(sub)) {
            String name = null;
            boolean attribution = GammaConfig.configs.gamma_bench_attribution;
            for (int i = 2; i < args.length; i++) {
                if ("attribution".equalsIgnoreCase(args[i])) {
                    attribution = true;
                } else if (name == null) {
                    name = args[i];
                }
            }
            String error = recorder.start(name == null ? "run" : name, attribution);
            if (error == null) {
                sender.sendMessage(ChatColor.GREEN + "Recording every tick to " + recorder.directory()
                        + (attribution ? ", with time per class, mod and chunk" : ""));
            } else {
                sender.sendMessage(ChatColor.RED + error);
            }
        } else if ("stop".equals(sub)) {
            String summary = recorder.stop();
            sender.sendMessage(summary == null
                    ? ChatColor.YELLOW + "No recording is running."
                    : ChatColor.GREEN + "Recording stopped: " + ChatColor.WHITE + summary);
        } else if ("status".equals(sub)) {
            sender.sendMessage(recorder.isRecording()
                    ? ChatColor.GREEN + "Recording to " + recorder.directory()
                    + (recorder.isAttributing() ? ", with attribution" : "")
                    : ChatColor.GRAY + "No recording is running.");
        } else {
            sender.sendMessage(ChatColor.GRAY + "Usage: /autothread record start [name] [attribution]|stop|status");
        }
    }

    private void sendLines(CommandSender sender, String text) {
        for (String line : text.split("\n")) {
            sender.sendMessage(line);
        }
    }

    /** Summary lines used by other diagnostics, kept here so formatting stays in one place. */
    public static String shortSummary() {
        TickStatistics ticks = GammaProfiler.get().ticks();
        TickStatistics.Mspt mspt = ticks.mspt();
        List<LatencyHistogram.Snapshot> top = GammaProfiler.get().registry().snapshotsByCost();
        StringBuilder out = new StringBuilder();
        out.append(String.format(Locale.ROOT, "TPS %.2f, MSPT %s", ticks.tps1m(), mspt == null ? "n/a" : mspt.toString()));
        if (!top.isEmpty()) {
            out.append(", hottest: ").append(top.get(0).name());
        }
        return out.toString();
    }
}
