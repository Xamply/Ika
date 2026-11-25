package ika.boost;

import java.io.*;
import java.nio.file.*;
import java.nio.file.attribute.BasicFileAttributes;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Function;

/**
 * Package-set cache for SecureJar (Forge's module layer). Keyed by the jar's
 * absolute path, size and modification time.
 */
final class Packages implements Function<Object[], Object> {
    private record Key(long size, long mtime) {}
    private record Value(long size, long mtime, Set<String> packages) {}

    private final Path file;
    private final Map<String, Value> map = new ConcurrentHashMap<>();
    private volatile boolean dirty;
    private int hits, misses;

    Packages(Path dir) {
        this.file = dir.resolve("packages.idx");
        try (DataInputStream in = new DataInputStream(new BufferedInputStream(Files.newInputStream(file), 1 << 16))) {
            int n = in.readInt();
            for (int i = 0; i < n; i++) {
                String path = in.readUTF();
                long size = in.readLong(), mtime = in.readLong();
                int c = in.readInt();
                HashSet<String> set = new HashSet<>(c * 2);
                for (int j = 0; j < c; j++) set.add(in.readUTF());
                map.put(path, new Value(size, mtime, Collections.unmodifiableSet(set)));
            }
        } catch (IOException ignored) {
        }
        Runtime.getRuntime().addShutdownHook(new Thread(this::save, "IkaBoost-pkgs"));
    }

    private static long[] stat(Path p) {
        try {
            if (!"file".equals(p.getFileSystem().provider().getScheme())) return null;
            BasicFileAttributes a = Files.readAttributes(p, BasicFileAttributes.class);
            if (!a.isRegularFile()) return null;
            return new long[]{a.size(), a.lastModifiedTime().toMillis()};
        } catch (Throwable e) {
            return null;
        }
    }

    @Override
    @SuppressWarnings("unchecked")
    public Object apply(Object[] a) {
        Path p = (Path) a[0];
        long[] st = stat(p);
        if (st == null) return null;
        String key = p.toAbsolutePath().toString();
        if (a[1] == null) {
            Value v = map.get(key);
            if (v != null && v.size == st[0] && v.mtime == st[1]) {
                hits++;
                // getPackages() returns a mutable HashSet: hand out a copy.
                return new HashSet<>(v.packages);
            }
            return null;
        }
        misses++;
        map.put(key, new Value(st[0], st[1], Set.copyOf((Set<String>) a[1])));
        dirty = true;
        return null;
    }

    synchronized void save() {
        if (!dirty) return;
        dirty = false;
        Path tmp = file.resolveSibling("packages.idx.tmp");
        try (DataOutputStream out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(tmp), 1 << 16))) {
            out.writeInt(map.size());
            for (Map.Entry<String, Value> e : map.entrySet()) {
                out.writeUTF(e.getKey());
                out.writeLong(e.getValue().size);
                out.writeLong(e.getValue().mtime);
                out.writeInt(e.getValue().packages.size());
                for (String s : e.getValue().packages) out.writeUTF(s);
            }
        } catch (IOException e) {
            return;
        }
        try {
            Files.move(tmp, file, StandardCopyOption.REPLACE_EXISTING);
        } catch (IOException ignored) {
        }
        System.err.println("[IkaBoost] package index: " + map.size() + " jars (hits " + hits + ", misses " + misses + ")");
    }
}
