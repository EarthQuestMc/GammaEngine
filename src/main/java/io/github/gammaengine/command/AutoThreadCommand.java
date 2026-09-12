package io.github.gammaengine.command;

import io.github.gammaengine.GammaEngine;
import io.github.gammaengine.autothread.AutoThreadRuntime;
import io.github.gammaengine.concurrent.ManagedPool;
import io.github.gammaengine.concurrent.ThreadPools;
import io.github.gammaengine.profiler.GammaProfiler;
import io.github.gammaengine.profiler.LatencyHistogram;
import io.github.gammaengine.profiler.TickStatistics;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

import java.io.File;
import java.util.List;
import java.util.Map;

/**
 * {@code /autothread}: the operator-facing window into the runtime.
 *
 * <p>The command answers questions in the order an administrator actually asks them: is the server
 * healthy ({@code status}), where is the time going ({@code profile}), what is the runtime fighting
 * with ({@code conflicts}), and are the workers saturated ({@code workers}).
 */
public class AutoThreadCommand extends Command {
    public AutoThreadCommand() {
        super("autothread");
        setDescription("Inspect and control the " + GammaEngine.NAME + " AutoThread runtime");
        setUsage(ChatColor.translateAlternateColorCodes('&',
                "&7&m----------------&7[&bAutoThread&7]&m----------------\n"
                        + "&b  >&e /autothread status &7-&a runtime state, TPS and MSPT.\n"
                        + "&b  >&e /autothread workers &7-&a thread pool occupancy.\n"
                        + "&b  >&e /autothread regions &7-&a active regions and their owners.\n"
                        + "&b  >&e /autothread conflicts &7-&a observed access conflicts.\n"
                        + "&b  >&e /autothread native &7-&a native engine state.\n"
                        + "&b  >&e /autothread profile start|stop|report &7-&a profiling session."));
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
        } else if ("workers".equals(action)) {
            workers(sender);
        } else if ("regions".equals(action)) {
            regions(sender);
        } else if ("conflicts".equals(action)) {
            conflicts(sender);
        } else if ("native".equals(action)) {
            nativeEngine(sender);
        } else if ("profile".equals(action)) {
            profile(sender, args);
        } else {
            sender.sendMessage(usageMessage);
        }
        return true;
    }

    private void workers(CommandSender sender) {
        ThreadPools pools = ThreadPools.get();
        if (pools == null) {
            sender.sendMessage(ChatColor.RED + "AutoThread runtime has not booted yet.");
            return;
        }
        sender.sendMessage(ChatColor.AQUA + "Thread pools (" + pools.physicalCores() + " physical cores):");
        for (ManagedPool pool : pools.all()) {
            LatencyHistogram.Snapshot wait = pool.queueWaitHistogram().snapshot();
            LatencyHistogram.Snapshot exec = pool.executionHistogram().snapshot();
            sender.sendMessage(String.format(ChatColor.GRAY + "  %-12s " + ChatColor.WHITE
                            + "%d thr, %d active, %d queued, %d done" + ChatColor.GRAY
                            + " | wait p95 %.2fms | exec p95 %.2fms",
                    pool.name(), pool.threads(), pool.activeThreads(), pool.queueDepth(),
                    pool.completedTasks(), wait.p95Millis(), exec.p95Millis()));
        }
    }

    private void regions(CommandSender sender) {
        // Regions arrive with the region manager; until then the server runs one implicit region
        // per world, owned by the server thread, and saying so is more useful than an empty list.
        sender.sendMessage(ChatColor.AQUA + "Regions:");
        sender.sendMessage(ChatColor.GRAY + "  Region partitioning is not active yet on this build: "
                + "world simulation runs in the single implicit region owned by the server thread.");
    }

    private void conflicts(CommandSender sender) {
        Map<String, Long> counters = GammaProfiler.get().registry().counterValues();
        boolean any = false;
        sender.sendMessage(ChatColor.AQUA + "Access conflicts:");
        for (Map.Entry<String, Long> entry : counters.entrySet()) {
            if (entry.getKey().startsWith("conflict.") && entry.getValue() > 0) {
                sender.sendMessage(ChatColor.GRAY + "  " + entry.getKey() + ": " + ChatColor.WHITE + entry.getValue());
                any = true;
            }
        }
        if (!any) {
            sender.sendMessage(ChatColor.GRAY + "  None recorded.");
        }
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
        out.append(String.format("TPS %.2f, MSPT %s", ticks.tps1m(), mspt == null ? "n/a" : mspt.toString()));
        if (!top.isEmpty()) {
            out.append(", hottest: ").append(top.get(0).name());
        }
        return out.toString();
    }
}
