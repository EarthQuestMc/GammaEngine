package io.github.gammaengine;

import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;

/**
 * Entry point constants for the GammaEngine fork.
 *
 * <p>GammaEngine is a fork of Crucible (Minecraft 1.7.10, Forge 10.13.4.1614,
 * Bukkit 1.7.10-R0.1-SNAPSHOT) whose goal is to use several physical CPU cores for world
 * simulation while keeping vanilla/Forge/Bukkit behaviour byte-for-byte compatible.
 *
 * <p>Everything added by the fork lives under {@code io.github.gammaengine} so that upstream
 * Crucible changes can still be merged with minimal conflicts. Patches applied to Minecraft,
 * Forge or Bukkit classes are kept as small as possible: they should delegate to this package
 * instead of embedding logic.
 */
public final class GammaEngine {
    /** Logger shared by every GammaEngine subsystem. */
    public static final Logger LOGGER = LogManager.getLogger("GammaEngine");

    /** Human readable fork name, used in logs and in the {@code /autothread} command output. */
    public static final String NAME = "GammaEngine";

    private GammaEngine() {
    }
}
