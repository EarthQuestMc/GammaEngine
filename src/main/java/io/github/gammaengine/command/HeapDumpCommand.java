package io.github.gammaengine.command;

import net.minecraftforge.cauldron.CauldronHooks;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

import java.io.File;
import java.text.SimpleDateFormat;
import java.util.Date;

/** {@code /heapdump}: writes the server heap to dumps/. Freezes the server for as long as the dump takes. */
public class HeapDumpCommand extends Command {
    public HeapDumpCommand() {
        super("heapdump");
        setDescription("Writes a heap dump of the server to dumps/ (the server pauses while it writes)");
        setUsage("/heapdump");
        setPermission("gamma.heap");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        File file = new File(new File(new File("."), "dumps"),
                "heap-dump-" + new SimpleDateFormat("yyyy-MM-dd_HH.mm.ss").format(new Date()) + "-server.hprof");
        sender.sendMessage(ChatColor.GRAY + "Writing heap dump to " + file);
        CauldronHooks.dumpHeap(file, true);
        sender.sendMessage(ChatColor.GREEN + "Heap dump written.");
        return true;
    }
}
