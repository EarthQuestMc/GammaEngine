package org.spigotmc;

import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

public class TicksPerSecondCommand extends Command {

    public TicksPerSecondCommand(String name) {
        super(name);
        this.description = "Gets the current ticks per second for the server";
        this.usageMessage = "/tps";
        this.setPermission("bukkit.command.tps");
    }

    @Override
    public boolean execute(CommandSender sender, String currentAlias, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }

        // GammaEngine - TPS over five windows, tick durations, CPU, memory, GC and load, instead of
        // three smoothed TPS figures; the first line keeps the "TPS from last ..." shape.
        for (String line : io.github.gammaengine.diag.ServerHealth.get().report()) {
            sender.sendMessage(line);
        }

        return true;
    }
}
