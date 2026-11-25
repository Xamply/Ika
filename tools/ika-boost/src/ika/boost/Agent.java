package ika.boost;

import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Method;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.security.ProtectionDomain;

/**
 * Java agent injected by Ika Launcher. It wraps ModLauncher's
 * ClassTransformer.transform(byte[], String, String) so already transformed
 * classes come from a disk cache instead of going through Mixin (whose
 * launch plugin serialises every class load behind one lock).
 *
 * The bytecode patch uses the game's own ASM jar, loaded in a private class
 * loader: ASM in the app class loader would clash with the org.objectweb.asm
 * module that BootstrapLauncher defines later. The patched method only uses
 * java.base types, so the named modlauncher module needs no extra reads.
 */
public final class Agent {
    static final String TARGET = "cpw/mods/modlauncher/ClassTransformer";
    static final String ORIGINAL = "ika$transform";

    public static void premain(String args, Instrumentation inst) {
        if (args == null || args.isEmpty()) return;
        try {
            Path asm = null;
            for (String e : System.getProperty("java.class.path", "").split(java.io.File.pathSeparator)) {
                String n = Paths.get(e).getFileName().toString();
                if (n.startsWith("asm-") && Character.isDigit(n.charAt(4)) && n.endsWith(".jar")) asm = Paths.get(e);
            }
            if (asm == null) throw new IllegalStateException("ASM not on the classpath");
            URL self = Agent.class.getProtectionDomain().getCodeSource().getLocation();
            // Child-first: Patch and ASM must both be defined by this loader, never
            // by the app loader (which also sees ika-boost.jar).
            URLClassLoader isolated = new URLClassLoader(new URL[]{self, asm.toUri().toURL()},
                    ClassLoader.getPlatformClassLoader()) {
                @Override
                protected Class<?> loadClass(String name, boolean resolve) throws ClassNotFoundException {
                    if (name.startsWith("ika.boost.Patch") || name.startsWith("org.objectweb.asm.")) {
                        synchronized (getClassLoadingLock(name)) {
                            Class<?> c = findLoadedClass(name);
                            if (c == null) c = findClass(name);
                            if (resolve) resolveClass(c);
                            return c;
                        }
                    }
                    return super.loadClass(name, resolve);
                }
            };
            Class<?> patches = isolated.loadClass("ika.boost.Patch");
            Method patch = patches.getMethod("patch", byte[].class);
            Method patchMixin = patches.getMethod("patchMixinProcessor", byte[].class);
            Method patchInner = patches.getMethod("patchInnerClassGenerator", byte[].class);
            Method patchJar = patches.getMethod("patchJar", byte[].class);
            Method patchZip = patches.getMethod("patchZipFileSystem", byte[].class);
            Method patchScan = patches.getMethod("patchBackgroundScan", byte[].class);
            Method patchFrame = patches.getMethod("patchMinecraftFrameLimit", byte[].class);
            Method patchOpcodes = patches.getMethod("patchMixinBytecode", byte[].class);
            boolean pump = !"false".equals(System.getProperty("ika.boost.pump"));
            System.getProperties().put("ika.boost.pkgs", new Packages(Paths.get(args).getParent()));
            System.setProperty("ika.boost.session", Paths.get(args).getFileName().toString());
            Hook hook = new Hook(args);
            System.getProperties().put("ika.boost", hook);
            inst.addTransformer(new ClassFileTransformer() {
                @Override
                public byte[] transform(ClassLoader loader, String name, Class<?> redefined,
                                        ProtectionDomain domain, byte[] bytes) {
                    if (redefined != null || name == null) return null;
                    if (name.equals("net/minecraftforge/fml/loading/moddiscovery/BackgroundScanHandler")) {
                        try {
                            return (byte[]) patchScan.invoke(null, (Object) bytes);
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch mod scan: " + t);
                            return null;
                        }
                    }
                    if (name.equals("jdk/nio/zipfs/ZipFileSystem")) {
                        try {
                            return (byte[]) patchZip.invoke(null, (Object) bytes);
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch zipfs: " + t);
                            return null;
                        }
                    }
                    if (name.equals("cpw/mods/jarhandling/impl/Jar")) {
                        try {
                            return (byte[]) patchJar.invoke(null, (Object) bytes);
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch SecureJar: " + t);
                            return null;
                        }
                    }
                    if (name.equals("org/spongepowered/asm/mixin/transformer/MixinProcessor")
                            || name.equals("org/spongepowered/asm/mixin/transformer/InnerClassGenerator")) {
                        try {
                            byte[] out = (byte[]) (name.endsWith("MixinProcessor") ? patchMixin : patchInner).invoke(null, (Object) bytes);
                            if (out == null) hook.disable();
                            return out;
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch Mixin: " + t);
                            hook.disable();
                            return null;
                        }
                    }
                    if (name.equals("org/spongepowered/asm/util/Bytecode")) {
                        try {
                            return (byte[]) patchOpcodes.invoke(null, (Object) bytes);
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch Mixin Bytecode: " + t);
                            return null;
                        }
                    }
                    if (pump && name.equals("net/minecraft/client/Minecraft")) {
                        try {
                            return (byte[]) patchFrame.invoke(null, (Object) bytes);
                        } catch (Throwable t) {
                            System.err.println("[IkaBoost] could not patch frame limiter: " + t);
                            return null;
                        }
                    }
                    if (!TARGET.equals(name)) {
                        // Only game-layer classes: a class that ModLauncher would transform but
                        // that was defined without going through it is generated code.
                        if (loader != null && loader.getClass().getName().equals("cpw.mods.modlauncher.TransformingClassLoader"))
                            hook.onDefine(name);
                        return null;
                    }
                    try {
                        return (byte[]) patch.invoke(null, (Object) bytes);
                    } catch (Throwable t) {
                        Throwable c = t.getCause() != null ? t.getCause() : t;
                        System.err.println("[IkaBoost] could not patch ModLauncher: " + c);
                        for (StackTraceElement s : c.getStackTrace()) System.err.println("[IkaBoost]   at " + s);
                        return null;
                    }
                }
            });
        } catch (Throwable t) {
            System.err.println("[IkaBoost] disabled: " + t);
        }
    }
}










