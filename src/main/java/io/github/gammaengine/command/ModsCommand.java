package io.github.gammaengine.command;

import cpw.mods.fml.common.Loader;
import cpw.mods.fml.common.ModContainer;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;

import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/** {@code /mods}: the Forge mods on this server, active ones in green, the others in red. */
public class ModsCommand extends Command {
    public ModsCommand() {
        super("mods");
        setDescription("Lists the Forge mods loaded on the server");
        setUsage("/mods");
        setPermission("gamma.mods");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        List<ModContainer> mods = new ArrayList<ModContainer>(Loader.instance().getModList());
        Set<ModContainer> active = new HashSet<ModContainer>(Loader.instance().getActiveModList());
        Collections.sort(mods, new Comparator<ModContainer>() {
            @Override
            public int compare(ModContainer a, ModContainer b) {
                return a.getName().compareToIgnoreCase(b.getName());
            }
        });

        StringBuilder list = new StringBuilder();
        for (ModContainer mod : mods) {
            if (list.length() > 0) {
                list.append(ChatColor.GRAY).append(", ");
            }
            list.append(active.contains(mod) ? ChatColor.GREEN : ChatColor.RED).append(mod.getName())
                    .append(ChatColor.DARK_GRAY).append(' ').append(mod.getVersion());
        }
        sender.sendMessage(ChatColor.GOLD + "Mods (" + active.size() + " active of " + mods.size() + "): " + list);
        return true;
    }
}
