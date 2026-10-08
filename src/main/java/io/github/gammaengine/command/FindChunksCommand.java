package io.github.gammaengine.command;

import net.minecraft.server.MinecraftServer;
import net.minecraft.world.WorldServer;
import net.minecraft.world.chunk.Chunk;
import net.minecraftforge.common.DimensionManager;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.entity.Player;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;

/** {@code /findchunks [dimension]}: the twenty loaded chunks holding the most tile entities. */
public class FindChunksCommand extends Command {
    private static final int SHOWN = 20;

    public FindChunksCommand() {
        super("findchunks");
        setDescription("Lists the loaded chunks with the most tile entities");
        setUsage("/findchunks [dimension]");
        setPermission("gamma.findChunks");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        WorldServer world;
        if (args.length >= 1) {
            int id;
            try {
                id = Integer.parseInt(args[0]);
            } catch (NumberFormatException e) {
                sender.sendMessage(ChatColor.RED + "Not a dimension number: " + args[0]);
                return true;
            }
            world = DimensionManager.getWorld(id);
            if (world == null) {
                sender.sendMessage(ChatColor.RED + "Dimension " + id + " is not loaded.");
                return true;
            }
        } else if (sender instanceof Player) {
            world = ((Player) sender).getWorld().getWorldServer();
        } else {
            world = MinecraftServer.getServer().worldServers[0];
        }

        List<Chunk> chunks = new ArrayList<Chunk>(world.theChunkProviderServer.loadedChunkHashMap_KC.rawVanilla().values());
        Collections.sort(chunks, new Comparator<Chunk>() {
            @Override
            public int compare(Chunk a, Chunk b) {
                return Integer.compare(b.chunkTileEntityMap.size(), a.chunkTileEntityMap.size());
            }
        });
        sender.sendMessage(ChatColor.GOLD + "Chunks with the most tile entities in dimension "
                + world.provider.dimensionId + ":");
        for (int i = 0; i < Math.min(chunks.size(), SHOWN); i++) {
            Chunk chunk = chunks.get(i);
            sender.sendMessage(ChatColor.GRAY + "  [" + ChatColor.YELLOW + (chunk.xPosition << 4) + ChatColor.GRAY + ", "
                    + ChatColor.YELLOW + (chunk.zPosition << 4) + ChatColor.GRAY + "] " + ChatColor.WHITE
                    + chunk.chunkTileEntityMap.size() + ChatColor.GRAY + " tile entities, "
                    + countEntities(chunk) + " entities");
        }
        return true;
    }

    private static int countEntities(Chunk chunk) {
        int count = 0;
        for (List<?> section : chunk.entityLists) {
            count += section.size();
        }
        return count;
    }
}
