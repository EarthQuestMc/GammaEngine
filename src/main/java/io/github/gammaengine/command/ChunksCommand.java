package io.github.gammaengine.command;

import net.minecraft.server.MinecraftServer;
import net.minecraft.world.WorldServer;
import net.minecraftforge.cauldron.CauldronHooks;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

import java.io.File;
import java.text.SimpleDateFormat;
import java.util.Date;

/** {@code /chunks [dump [all]]}: loaded content per world, and a dump of every loaded chunk on request. */
public class ChunksCommand extends Command {
    public ChunksCommand() {
        super("chunks");
        setDescription("Shows loaded chunks, entities and tile entities per world");
        setUsage("/chunks [dump [all]]");
        setPermission("gamma.chunks");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        for (WorldServer world : MinecraftServer.getServer().worlds) {
            sender.sendMessage(ChatColor.GOLD + "Dimension " + ChatColor.WHITE + world.provider.dimensionId
                    + ChatColor.GRAY + " (" + world.provider.getDimensionName() + ")"
                    + ChatColor.GOLD + "  loaded chunks: " + ChatColor.WHITE + world.theChunkProviderServer.loadedChunkHashMap_KC.size()
                    + ChatColor.GOLD + "  active: " + ChatColor.WHITE + world.activeChunkSet.size()
                    + ChatColor.GOLD + "  entities: " + ChatColor.WHITE + world.loadedEntityList.size()
                    + ChatColor.GOLD + "  tile entities: " + ChatColor.WHITE + world.loadedTileEntityList.size());
            sender.sendMessage(ChatColor.GRAY + "  ticked last tick: " + world.entitiesTicked + " entities, "
                    + world.tilesTicked + " tile entities; waiting removal: " + world.unloadedEntityList.size()
                    + " entities, " + world.field_147483_b.size() + " tile entities");
        }

        if (args.length < 1 || !"dump".equalsIgnoreCase(args[0])) {
            return true;
        }
        boolean dumpAll = args.length > 1 && "all".equalsIgnoreCase(args[1]);
        File file = new File(new File(new File("."), "chunk-dumps"),
                "chunk-info-" + new SimpleDateFormat("yyyy-MM-dd_HH.mm.ss").format(new Date()) + "-server.txt");
        sender.sendMessage(ChatColor.GRAY + "Writing chunk info to " + file);
        CauldronHooks.writeChunks(file, dumpAll);
        sender.sendMessage(ChatColor.GREEN + "Chunk info written.");
        return true;
    }
}
