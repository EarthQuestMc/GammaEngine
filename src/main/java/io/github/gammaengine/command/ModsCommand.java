package io.github.gammaengine.command;

import cpw.mods.fml.common.Loader;
import cpw.mods.fml.common.ModContainer;
import io.github.crucible.CrucibleModContainer;
import org.bukkit.Bukkit;
import org.bukkit.ChatColor;
import org.bukkit.command.Command;
import org.bukkit.command.CommandSender;
import org.bukkit.plugin.Plugin;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * {@code /mods}: the Forge mods and the Bukkit plugins of this server, in one answer. Active mods and
 * enabled plugins in green, the others in red, plugins provided by a mod in aqua.
 */
public class ModsCommand extends Command {
    public ModsCommand() {
        super("mods");
        setDescription("Lists the Forge mods and the Bukkit plugins loaded on the server");
        setUsage("/mods");
        setPermission("gamma.mods");
    }

    @Override
    public boolean execute(CommandSender sender, String label, String[] args) {
        if (!testPermission(sender)) {
            return true;
        }
        sender.sendMessage(mods());
        sender.sendMessage(plugins());
        return true;
    }

    private static String mods() {
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
            separate(list);
            list.append(active.contains(mod) ? ChatColor.GREEN : ChatColor.RED).append(mod.getName())
                    .append(ChatColor.DARK_GRAY).append(' ').append(mod.getVersion());
        }
        return ChatColor.GOLD + "Mods (" + active.size() + " active of " + mods.size() + "): " + list;
    }

    private static String plugins() {
        List<Plugin> plugins = new ArrayList<Plugin>(Arrays.asList(Bukkit.getPluginManager().getPlugins()));
        Collections.sort(plugins, new Comparator<Plugin>() {
            @Override
            public int compare(Plugin a, Plugin b) {
                return a.getName().compareToIgnoreCase(b.getName());
            }
        });

        int enabled = 0;
        StringBuilder list = new StringBuilder();
        for (Plugin plugin : plugins) {
            separate(list);
            if (plugin.isEnabled()) {
                enabled++;
            }
            ChatColor colour = CrucibleModContainer.isModPlugin(plugin) ? ChatColor.AQUA
                    : plugin.isEnabled() ? ChatColor.GREEN : ChatColor.RED;
            list.append(colour).append(plugin.getName())
                    .append(ChatColor.DARK_GRAY).append(' ').append(plugin.getDescription().getVersion());
        }
        return ChatColor.GOLD + "Plugins (" + enabled + " enabled of " + plugins.size() + "): "
                + (plugins.isEmpty() ? ChatColor.GRAY + "none" : list.toString());
    }

    private static void separate(StringBuilder list) {
        if (list.length() > 0) {
            list.append(ChatColor.GRAY).append(", ");
        }
    }
}
