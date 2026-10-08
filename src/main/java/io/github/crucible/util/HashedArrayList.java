package io.github.crucible.util;

import java.util.*;

/**
 * An {@link ArrayList} that refuses duplicates and answers {@link #contains} from a hash set.
 *
 * <p>This is the concrete type of {@code World.loadedEntityList}, {@code unloadedEntityList},
 * {@code loadedTileEntityList} and {@code addedTileEntityList}, so mods see it through the
 * {@code List} interface: every mutating method, including the ones reached through iterators,
 * must keep the list and the set holding exactly the same elements.
 *
 * <p>Thread invariant: written and iterated by the thread that owns the world only. The set is
 * synchronized for historical reasons; the list itself is not thread-safe.
 */
public class HashedArrayList<TileEntity> extends ArrayList<TileEntity> {

    private final Set<TileEntity> hashed = Collections.synchronizedSet(new LinkedHashSet<TileEntity>());

    public HashedArrayList() {
        super();
    }

    @Override
    public boolean add(TileEntity arg0) {
        boolean flag = hashed.add(arg0);

        if (flag)
            super.add(arg0);

        return flag;
    }

    @Override
    public void add(int arg0, TileEntity arg1) {
        boolean flag = hashed.add(arg1);

        if (flag)
            super.add(arg0, arg1);
    }

    // GammaEngine - add element by element: the set used to accept only the new elements while
    // the list appended all of them, duplicates included.
    @Override
    public boolean addAll(Collection arg0) {
        boolean changed = false;
        for (Object element : arg0) {
            changed |= add((TileEntity) element);
        }
        return changed;
    }

    @Override
    public boolean addAll(int arg0, Collection arg1) {
        List<TileEntity> fresh = new ArrayList<TileEntity>(arg1.size());
        for (Object element : arg1) {
            if (hashed.add((TileEntity) element))
                fresh.add((TileEntity) element);
        }

        if (!fresh.isEmpty())
            super.addAll(arg0, fresh);

        return !fresh.isEmpty();
    }

    @Override
    public void clear() {
        this.hashed.clear();
        super.clear();
    }

    @Override
    public boolean contains(Object arg0) {
        return hashed.contains(arg0);
    }

    @Override
    public boolean containsAll(Collection arg0) {
        return hashed.containsAll(arg0);
    }

    @Override
    public TileEntity get(int arg0) {
        return super.get(arg0);
    }

    @Override
    public int indexOf(Object arg0) {
        return super.indexOf(arg0);
    }

    @Override
    public boolean isEmpty() {
        return super.isEmpty();
    }

    @Override
    public Iterator<TileEntity> iterator() {
        return new HashedArrayIterator(super.iterator(), this.hashed);
    }

    @Override
    public int lastIndexOf(Object arg0) {
        if (this.hashed.contains(arg0)) {
            return super.lastIndexOf(arg0);
        } else {
            return -1;
        }
    }

    // GammaEngine - listIterator() used to call itself and overflow the stack, and
    // listIterator(int) returned an iterator whose mutations bypassed the set.
    @Override
    public ListIterator listIterator() {
        return listIterator(0);
    }

    @Override
    public ListIterator listIterator(int arg0) {
        return new HashedArrayListIterator(super.listIterator(arg0));
    }

    @Override
    public boolean remove(Object arg0) {
        boolean flag = this.hashed.remove(arg0);
        if (flag)
            super.remove(arg0);
        return flag;
    }

    @Override
    public TileEntity remove(int arg0) {
        TileEntity te = super.remove(arg0);

        if (te != null)
            hashed.remove(te);

        return te;
    }

    // GammaEngine - remove in place: rebuilding the list from the set reordered it.
    @Override
    public boolean removeAll(Collection arg0) {
        boolean flag = this.hashed.removeAll(arg0);

        if (flag)
            super.removeAll(arg0);

        return flag;
    }

    @Override
    public boolean retainAll(Collection arg0) {
        boolean flag = this.hashed.retainAll(arg0);

        if (flag)
            super.retainAll(arg0);

        return flag;
    }

    // GammaEngine - the set used to lose the new element instead of the replaced one. A replaced
    // element only leaves the set once no slot holds it any more, which keeps swaps (two sets in a
    // row, as Collections.swap and shuffle do) consistent.
    @Override
    public TileEntity set(int arg0, TileEntity arg1) {
        TileEntity te = super.set(arg0, arg1);
        replaced(te, arg1);
        return te;
    }

    @Override
    public boolean removeIf(java.util.function.Predicate<? super TileEntity> filter) {
        boolean flag = super.removeIf(filter);

        if (flag)
            resynchronize();

        return flag;
    }

    @Override
    public void replaceAll(java.util.function.UnaryOperator<TileEntity> operator) {
        super.replaceAll(operator);
        resynchronize();
    }

    @Override
    public int size() {
        return super.size();
    }

    @Override
    public List<TileEntity> subList(int arg0, int arg1) {
        return super.subList(arg0, arg1);
    }

    @Override
    public Object[] toArray() {
        return super.toArray();
    }

    @Override
    public Object[] toArray(Object[] arg0) {
        return super.toArray(arg0);
    }

    private void replaced(TileEntity previous, TileEntity current) {
        if (previous == current)
            return;

        hashed.add(current);

        if (super.indexOf(previous) < 0)
            hashed.remove(previous);
    }

    /** Rebuilds the set from the list after a bulk operation that ArrayList performs in place. */
    private void resynchronize() {
        hashed.clear();
        hashed.addAll(this);
    }

    class HashedArrayIterator<TileEntity> implements Iterator<TileEntity> {
        Iterator<TileEntity> aritr;
        Set<TileEntity> teset;
        private TileEntity last = null;

        public HashedArrayIterator(Iterator aritr, Set<TileEntity> teset) {
            this.aritr = aritr;
            this.teset = teset;
        }

        @Override
        public boolean hasNext() {
            return aritr.hasNext();
        }

        @Override
        public TileEntity next() {
            last = aritr.next();
            return last;
        }

        @Override
        public void remove() {
            aritr.remove();
            teset.remove(last);
        }
    }

    // GammaEngine - the set was never assigned, so add, remove and set threw a NullPointerException.
    // ArrayList's own list iterator already routes add, set and remove through the overridden
    // methods of this list, which keep the set in step; this wrapper only has to refuse duplicates
    // before ArrayList moves its cursor.
    class HashedArrayListIterator implements ListIterator<TileEntity> {
        ListIterator<TileEntity> aritr;

        public HashedArrayListIterator(ListIterator<TileEntity> aritr) {
            this.aritr = aritr;
        }

        @Override
        public void add(TileEntity arg0) {
            if (!hashed.contains(arg0))
                this.aritr.add(arg0);
        }

        @Override
        public boolean hasNext() {
            return aritr.hasNext();
        }

        @Override
        public boolean hasPrevious() {
            return aritr.hasPrevious();
        }

        @Override
        public TileEntity next() {
            return aritr.next();
        }

        @Override
        public int nextIndex() {
            return aritr.nextIndex();
        }

        @Override
        public TileEntity previous() {
            return aritr.previous();
        }

        @Override
        public int previousIndex() {
            return aritr.previousIndex();
        }

        @Override
        public void remove() {
            aritr.remove();
        }

        @Override
        public void set(TileEntity arg0) {
            aritr.set(arg0);
        }
    }
}
