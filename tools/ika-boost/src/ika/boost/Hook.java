package ika.boost;

import java.io.*;
import java.nio.ByteBuffer;
import java.nio.MappedByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicLong;
import java.util.function.Function;
import java.util.zip.CRC32;

/**
 * Transformed-class cache. Entries are keyed by class name and validated by the
 * length and CRC32 of the untransformed bytes, so a changed jar only misses the
 * classes it changed. The launcher puts every other input (mod list, configs,
 * loader and Java versions) in the directory name.
 *
 * Called from the patched ClassTransformer with Object[]{in, name, reason, out, loader}:
 * out == null is a lookup (returns byte[] or null), out != null records a result.
 */
final class Hook implements Function<Object[], Object> {
    private static final int MAGIC = 0x494B4231; // "IKB1"
    private static final byte UNCHANGED = 0, BYTES = 1, SKIP = 2;

    private static final int CONFIRMED = 0x80;
    static final boolean PREWARM = Boolean.getBoolean("ika.boost.prewarm");

    /**
     * kind: UNCHANGED, BYTES or SKIP. confirmed: the same output came out of two
     * launches. Only confirmed entries are served, so transformers whose output
     * depends on timing or runtime state (seen with BiggerStacks) never get cached.
     */
    private record Entry(int inLen, int inCrc, byte kind, boolean confirmed, int offset, int outLen, byte[] out) {}

    private final Path dir, file, pending;
    private final boolean verify;
    private final Future<Map<String, Entry>> loaded;
    private volatile Map<String, Entry> base;
    private final ConcurrentHashMap<String, Entry> fresh = new ConcurrentHashMap<>();
    private volatile MappedByteBuffer data;
    private final AtomicLong hits = new AtomicLong(), misses = new AtomicLong(), mismatches = new AtomicLong();
    private volatile long lastMiss = System.nanoTime();
    private volatile boolean dirty;
    private volatile boolean disabled;

    /** Without a fixed Mixin session id, cached and fresh classes would disagree. */
    void disable() {
        disabled = true;
    }
    // Classes being transformed on this thread (cache misses only).
    private final ThreadLocal<ArrayDeque<String>> active = ThreadLocal.withInitial(ArrayDeque::new);
    // Every class that went through the patched ClassTransformer.
    private final Set<String> routed = ConcurrentHashMap.newKeySet();
    // Classes whose transformation defined other game classes as a side effect
    // (e.g. Jasione generates helper classes). They are never served from cache.
    private final Set<String> tainted = ConcurrentHashMap.newKeySet();
    private final AtomicLong skipped = new AtomicLong(), unstable = new AtomicLong();

    /** Called by the agent for every class the JVM defines in a TransformingClassLoader. */
    void onDefine(String internalName) {
        String name = internalName.replace('/', '.');
        // Classes loaded from jars go through ClassTransformer before they are defined.
        if (routed.contains(name)) return;
        ArrayDeque<String> stack = active.get();
        if (stack.isEmpty()) return;
        for (String s : stack) {
            if (tainted.add(s)) System.err.println("[IkaBoost] " + s + " defines " + name + " while transformed; not cached");
        }
    }

    Hook(String dir) throws IOException {
        this.dir = Paths.get(dir);
        Files.createDirectories(this.dir);
        this.file = this.dir.resolve("classes.ikc");
        this.pending = this.dir.resolve("classes.ikc.new");
        this.verify = Boolean.getBoolean("ika.boost.verify");
        if (Files.exists(pending)) Files.move(pending, file, StandardCopyOption.REPLACE_EXISTING);
        // Parse and pre-read the cache while the JVM and ModLauncher are still starting.
        FutureTask<Map<String, Entry>> task = new FutureTask<>(this::load);
        Thread t = new Thread(task, "IkaBoost-load");
        t.setDaemon(true);
        t.start();
        this.loaded = task;
        Thread flusher = new Thread(this::flushLoop, "IkaBoost-flush");
        flusher.setDaemon(true);
        flusher.start();
        Runtime.getRuntime().addShutdownHook(new Thread(this::flush, "IkaBoost-shutdown"));
    }

    private Map<String, Entry> load() {
        if (!Files.isRegularFile(file)) return Map.of();
        long start = System.nanoTime();
        try (FileChannel ch = FileChannel.open(file, StandardOpenOption.READ)) {
            MappedByteBuffer buf = ch.map(FileChannel.MapMode.READ_ONLY, 0, ch.size());
            buf.load(); // pull the whole file into the page cache in one sequential read
            if (buf.getInt() != MAGIC) return Map.of();
            int count = buf.getInt();
            HashMap<String, Entry> map = new HashMap<>(count * 2);
            byte[] nameBuf = new byte[1024];
            for (int i = 0; i < count; i++) {
                int nl = buf.getShort() & 0xFFFF;
                if (nl > nameBuf.length) nameBuf = new byte[nl];
                buf.get(nameBuf, 0, nl);
                String name = new String(nameBuf, 0, nl, java.nio.charset.StandardCharsets.UTF_8);
                int inLen = buf.getInt(), inCrc = buf.getInt();
                int raw = buf.get() & 0xFF;
                byte kind = (byte) (raw & 0x7F);
                boolean confirmed = (raw & CONFIRMED) != 0;
                int outLen = buf.getInt();
                int off = buf.position();
                buf.position(off + outLen);
                map.put(name, new Entry(inLen, inCrc, kind, confirmed, off, outLen, null));
            }
            data = buf;
            System.err.printf("[IkaBoost] cache: %d classes, %.1f MB, read in %d ms%n",
                    count, ch.size() / 1048576.0, (System.nanoTime() - start) / 1_000_000);
            return map;
        } catch (Throwable e) {
            System.err.println("[IkaBoost] cache unreadable, rebuilding: " + e);
            return Map.of();
        }
    }

    private Map<String, Entry> base() {
        Map<String, Entry> b = base;
        if (b != null) return b;
        try {
            b = loaded.get();
        } catch (Exception e) {
            b = Map.of();
        }
        base = b;
        return b;
    }

    /**
     * Transformers known to produce output that depends on runtime state
     * (BiggerStacks' transformerlib rewrites slot limits from its live config).
     */
    private static final byte[][] DYNAMIC = {
            "portb/transformerlib/".getBytes(java.nio.charset.StandardCharsets.UTF_8),
            // Jasione defines these helper classes while it transforms the class that
            // uses them; a cached user would skip that and fail with NoClassDefFoundError.
            "me/decce/jasione/cached/".getBytes(java.nio.charset.StandardCharsets.UTF_8),
    };

    private static boolean dynamic(byte[] out) {
        for (byte[] p : DYNAMIC) {
            search:
            for (int i = 0, n = out.length - p.length; i <= n; i++) {
                for (int j = 0; j < p.length; j++) if (out[i + j] != p[j]) continue search;
                return true;
            }
        }
        return false;
    }
    private static int crc(byte[] in) {
        CRC32 c = new CRC32();
        c.update(in, 0, in.length);
        return (int) c.getValue();
    }

    private byte[] bytesOf(Entry e, byte[] in) {
        if (e.kind == UNCHANGED) return in;
        if (e.out != null) return e.out;
        byte[] out = new byte[e.outLen];
        data.get(e.offset, out); // absolute get: thread-safe on a read-only buffer
        return out;
    }

    @Override
    public Object apply(Object[] a) {
        byte[] in = (byte[]) a[0];
        String name = (String) a[1];
        if (disabled) return null;
        // ModLauncher also transforms classes for plugins that only want to read them
        // (Mixin's getClassNode, reason "mixin"): cache those under "name@reason".
        if (!"classloading".equals(a[2])) {
            if (!(a[2] instanceof String r) || r.isEmpty()) return null;
            a = a.clone();
            a[1] = name + "@" + r;
            a[2] = "classloading";
            Object out = apply(a);
            return out;
        }
        if (a[3] == null) {
            routed.add(name.contains("@") ? name.substring(0, name.indexOf('@')) : name);
            Entry e = fresh.get(name);
            if (e == null) e = base().get(name);
            if (e == null || e.kind == SKIP || !e.confirmed || e.inLen != in.length || e.inCrc != crc(in)) {
                ArrayDeque<String> stack = active.get();
                if (stack.size() > 256) stack.clear();
                stack.push(name);
                if (e != null && e.kind == SKIP) skipped.incrementAndGet();
                return null;
            }
            if (verify) {
                // Let the real pipeline run, then compare in the record call.
                return null;
            }
            hits.incrementAndGet();
            return bytesOf(e, in);
        }
        byte[] out = (byte[]) a[3];
        ArrayDeque<String> stack = active.get();
        while (!stack.isEmpty()) if (stack.pop().equals(name)) break;
        if (dynamic(out)) {
            tainted.add(name);
        }
        if (verify) {
            Entry e = base().get(name);
            if (e != null && e.kind != SKIP && e.inLen == in.length && e.inCrc == crc(in)) {
                byte[] cached = bytesOf(e, in);
                if (!Arrays.equals(cached, out)) {
                    mismatches.incrementAndGet();
                    fresh.put(name, new Entry(in.length, crc(in), SKIP, true, 0, 0, null));
                    dirty = true;
                    System.err.println("[IkaBoost] MISMATCH " + name);
                    try {
                        Path d = dir.resolve("mismatch");
                        Files.createDirectories(d);
                        Files.write(d.resolve(name + ".cached.class"), cached);
                        Files.write(d.resolve(name + ".fresh.class"), out);
                    } catch (IOException ignored) {
                    }
                } else hits.incrementAndGet();
                return null;
            }
        }
        misses.incrementAndGet();
        lastMiss = System.nanoTime();
        boolean same = out == in || Arrays.equals(out, in);
        int inCrc = crc(in);
        if (dynamic(out)) tainted.add(name);
        Entry prev = base().get(name);
        boolean seen = prev != null && prev.kind != SKIP && prev.inLen == in.length && prev.inCrc == inCrc;
        if (tainted.contains(name) || (prev != null && prev.kind == SKIP && prev.inLen == in.length && prev.inCrc == inCrc)) {
            fresh.put(name, new Entry(in.length, inCrc, SKIP, true, 0, 0, null));
        } else if (seen && !Arrays.equals(bytesOf(prev, in), out)) {
            unstable.incrementAndGet();
            System.err.println("[IkaBoost] " + name + " changed between launches; not cached");
            fresh.put(name, new Entry(in.length, inCrc, SKIP, true, 0, 0, null));
        } else {
            fresh.put(name, new Entry(in.length, inCrc, same ? UNCHANGED : BYTES, seen, 0,
                    same ? 0 : out.length, same ? null : out.clone()));
        }
        dirty = true;
        return null;
    }

    private void flushLoop() {
        try {
            while (true) {
                Thread.sleep(2000);
                long quiet = System.nanoTime() - lastMiss;
                if (dirty && quiet > 8_000_000_000L) flush();
                // Background warm-up launch (hidden desktop): once class loading has
                // settled, the cache is complete enough; stop the game.
                if (PREWARM && quiet > 12_000_000_000L && misses.get() + hits.get() > 5000) {
                    flush();
                    System.err.println("[IkaBoost] prewarm done");
                    Thread killer = new Thread(() -> {
                        try { Thread.sleep(5000); } catch (InterruptedException ignored) { }
                        Runtime.getRuntime().halt(0);
                    }, "IkaBoost-halt");
                    killer.setDaemon(true);
                    killer.start();
                    System.exit(0);
                }
            }
        } catch (InterruptedException ignored) {
        }
    }

    private synchronized void flush() {
        long h = hits.get(), m = misses.get();
        boolean good = h > 5000 && h > 9 * m;
        if (good) {
            try { Files.writeString(dir.resolve("ready"), h + " hits, " + m + " misses\n"); } catch (IOException ignored) { }
        }
        if (!dirty) return;
        dirty = false;
        if (!good) {
            try { Files.deleteIfExists(dir.resolve("ready")); } catch (IOException ignored) { }
        }
        long start = System.nanoTime();
        Map<String, Entry> b = base();
        TreeMap<String, Entry> all = new TreeMap<>();
        b.forEach(all::put);
        all.putAll(fresh);
        Path tmp = dir.resolve("classes.ikc.tmp");
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(tmp), 1 << 20))) {
            out.writeInt(MAGIC);
            out.writeInt(all.size());
            for (Map.Entry<String, Entry> me : all.entrySet()) {
                Entry e = me.getValue();
                byte[] n = me.getKey().getBytes(java.nio.charset.StandardCharsets.UTF_8);
                out.writeShort(n.length);
                out.write(n);
                out.writeInt(e.inLen);
                out.writeInt(e.inCrc);
                out.writeByte(e.kind | (e.confirmed ? CONFIRMED : 0));
                out.writeInt(e.outLen);
                if (e.kind == BYTES) {
                    if (e.out != null) out.write(e.out);
                    else {
                        byte[] tmpb = new byte[e.outLen];
                        data.get(e.offset, tmpb);
                        out.write(tmpb);
                    }
                }
            }
        } catch (IOException e) {
            System.err.println("[IkaBoost] could not write cache: " + e);
            return;
        }
        try {
            // The current file may be memory-mapped; the swap happens on the next launch.
            Files.move(tmp, pending, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException e) {
            System.err.println("[IkaBoost] could not store cache: " + e);
            return;
        }
        System.err.printf("[IkaBoost] saved %d classes in %d ms (hits %d, misses %d, uncacheable %d, unstable %d, mismatches %d)%n",
                all.size(), (System.nanoTime() - start) / 1_000_000, hits.get(), misses.get(), tainted.size() + skipped.get(), unstable.get(), mismatches.get());
    }
}












