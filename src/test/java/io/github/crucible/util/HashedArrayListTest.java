package io.github.crucible.util;

import org.junit.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.ListIterator;
import java.util.Random;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

/**
 * The list behind {@code World.loadedEntityList}: membership answered by the hash set must always
 * match what the list holds, whichever {@code List} method a mod uses.
 */
public class HashedArrayListTest {

    private static HashedArrayList<String> of(String... values) {
        HashedArrayList<String> list = new HashedArrayList<String>();
        for (String value : values) {
            list.add(value);
        }
        return list;
    }

    private static void assertConsistent(List<String> list) {
        for (String value : list) {
            assertTrue("contains() lost " + value, list.contains(value));
        }
        assertEquals("duplicate in list", new java.util.HashSet<String>(list).size(), list.size());
    }

    @Test
    public void listIteratorDoesNotRecurse() {
        HashedArrayList<String> list = of("a", "b", "c");
        ListIterator<String> iterator = list.listIterator();
        assertEquals("a", iterator.next());
    }

    @Test
    public void listIteratorMutationsKeepTheSetInStep() {
        HashedArrayList<String> list = of("a", "b", "c");
        ListIterator<String> iterator = list.listIterator();
        iterator.next();
        iterator.remove();
        assertFalse(list.contains("a"));

        iterator.next();
        iterator.set("x");
        assertFalse(list.contains("b"));
        assertTrue(list.contains("x"));

        iterator.add("y");
        assertTrue(list.contains("y"));
        assertEquals(Arrays.asList("x", "y", "c"), new ArrayList<String>(list));
        assertConsistent(list);
    }

    @Test
    public void setReplacesMembership() {
        HashedArrayList<String> list = of("a", "b");
        assertEquals("a", list.set(0, "z"));
        assertFalse(list.contains("a"));
        assertTrue(list.contains("z"));
        assertConsistent(list);
    }

    @Test
    public void swapAndShuffleKeepEveryElement() {
        HashedArrayList<String> list = of("a", "b", "c", "d", "e", "f");
        Collections.swap(list, 0, 5);
        Collections.shuffle(list, new Random(42));
        assertEquals(6, list.size());
        for (String value : Arrays.asList("a", "b", "c", "d", "e", "f")) {
            assertTrue(list.contains(value));
        }
        assertConsistent(list);
    }

    @Test
    public void addAllSkipsDuplicates() {
        HashedArrayList<String> list = of("a", "b");
        assertTrue(list.addAll(Arrays.asList("b", "c", "c")));
        assertEquals(Arrays.asList("a", "b", "c"), new ArrayList<String>(list));

        assertTrue(list.addAll(1, Arrays.asList("a", "x")));
        assertEquals(Arrays.asList("a", "x", "b", "c"), new ArrayList<String>(list));
        assertFalse(list.addAll(Arrays.asList("a", "x")));
    }

    @Test
    public void removeAllKeepsListOrder() {
        HashedArrayList<String> list = of("a", "b", "c", "d");
        list.set(0, "z");
        assertTrue(list.removeAll(of("b")));
        assertEquals(Arrays.asList("z", "c", "d"), new ArrayList<String>(list));
        assertConsistent(list);
    }

    @Test
    public void removeIfUpdatesMembership() {
        HashedArrayList<String> list = of("a", "bb", "c");
        assertTrue(list.removeIf(value -> value.length() > 1));
        assertFalse(list.contains("bb"));
        assertConsistent(list);
    }
}
