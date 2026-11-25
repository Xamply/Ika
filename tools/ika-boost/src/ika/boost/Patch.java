package ika.boost;

import org.objectweb.asm.*;

/** Loaded in an isolated class loader so the game's ASM module is untouched. */
public final class Patch {
    static final String TARGET = "cpw/mods/modlauncher/ClassTransformer";
    static final String ORIGINAL = "ika$transform";

    public static byte[] patch(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        boolean[] found = new boolean[2];
        reader.accept(new ClassVisitor(Opcodes.ASM9) {
            @Override
            public FieldVisitor visitField(int a, String n, String d, String s, Object v) {
                if (n.equals("transformingClassLoader")
                        && d.equals("Lcpw/mods/modlauncher/TransformingClassLoader;")) found[0] = true;
                return null;
            }
            @Override
            public MethodVisitor visitMethod(int a, String n, String d, String s, String[] e) {
                if (n.equals("transform") && d.equals("([BLjava/lang/String;Ljava/lang/String;)[B")) found[1] = true;
                return null;
            }
        }, ClassReader.SKIP_CODE);
        if (!found[0] || !found[1]) {
            System.err.println("[IkaBoost] unknown ModLauncher layout, cache disabled");
            return null;
        }
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_FRAMES) {
            @Override
            protected String getCommonSuperClass(String a, String b) { return "java/lang/Object"; }
        };
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                if (n.equals("transform") && d.equals("([BLjava/lang/String;Ljava/lang/String;)[B")) {
                    int priv = (access & ~(Opcodes.ACC_PUBLIC | Opcodes.ACC_PROTECTED)) | Opcodes.ACC_PRIVATE;
                    writeWrapper(super.visitMethod(access, n, d, s, e));
                    return super.visitMethod(priv, ORIGINAL, d, s, e);
                }
                return super.visitMethod(access, n, d, s, e);
            }
        }, 0);
        System.err.println("[IkaBoost] ModLauncher class cache enabled");
        return writer.toByteArray();
    }

    // byte[] transform(byte[] in, String name, String reason) {
    //   Object h = System.getProperties().get("ika.boost");
    //   if (h == null) return ika$transform(in, name, reason);
    //   Object[] a = {in, name, reason, null, this.transformingClassLoader};
    //   Object r = ((Function) h).apply(a);
    //   if (r != null) return (byte[]) r;
    //   r = ika$transform(in, name, reason);
    //   a[3] = r; ((Function) h).apply(a);
    //   return (byte[]) r;
    // }
    static void writeWrapper(MethodVisitor mv) {
        String desc = "([BLjava/lang/String;Ljava/lang/String;)[B";
        String fn = "java/util/function/Function";
        String apply = "(Ljava/lang/Object;)Ljava/lang/Object;";
        Label plain = new Label(), miss = new Label();
        mv.visitCode();
        mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "getProperties", "()Ljava/util/Properties;", false);
        mv.visitLdcInsn("ika.boost");
        mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/util/Properties", "get", "(Ljava/lang/Object;)Ljava/lang/Object;", false);
        mv.visitVarInsn(Opcodes.ASTORE, 4);
        mv.visitVarInsn(Opcodes.ALOAD, 4);
        mv.visitJumpInsn(Opcodes.IFNULL, plain);

        mv.visitInsn(Opcodes.ICONST_5);
        mv.visitTypeInsn(Opcodes.ANEWARRAY, "java/lang/Object");
        mv.visitVarInsn(Opcodes.ASTORE, 5);
        for (int i = 0; i < 3; i++) {
            mv.visitVarInsn(Opcodes.ALOAD, 5);
            mv.visitLdcInsn(i);
            mv.visitVarInsn(Opcodes.ALOAD, i + 1);
            mv.visitInsn(Opcodes.AASTORE);
        }
        mv.visitVarInsn(Opcodes.ALOAD, 5);
        mv.visitInsn(Opcodes.ICONST_4);
        mv.visitVarInsn(Opcodes.ALOAD, 0);
        mv.visitFieldInsn(Opcodes.GETFIELD, TARGET, "transformingClassLoader", "Lcpw/mods/modlauncher/TransformingClassLoader;");
        mv.visitInsn(Opcodes.AASTORE);

        mv.visitVarInsn(Opcodes.ALOAD, 4);
        mv.visitTypeInsn(Opcodes.CHECKCAST, fn);
        mv.visitVarInsn(Opcodes.ALOAD, 5);
        mv.visitMethodInsn(Opcodes.INVOKEINTERFACE, fn, "apply", apply, true);
        mv.visitVarInsn(Opcodes.ASTORE, 6);
        mv.visitVarInsn(Opcodes.ALOAD, 6);
        mv.visitJumpInsn(Opcodes.IFNULL, miss);
        mv.visitVarInsn(Opcodes.ALOAD, 6);
        mv.visitTypeInsn(Opcodes.CHECKCAST, "[B");
        mv.visitInsn(Opcodes.ARETURN);

        mv.visitLabel(miss);
        mv.visitVarInsn(Opcodes.ALOAD, 0);
        mv.visitVarInsn(Opcodes.ALOAD, 1);
        mv.visitVarInsn(Opcodes.ALOAD, 2);
        mv.visitVarInsn(Opcodes.ALOAD, 3);
        mv.visitMethodInsn(Opcodes.INVOKESPECIAL, TARGET, ORIGINAL, desc, false);
        mv.visitVarInsn(Opcodes.ASTORE, 6);
        mv.visitVarInsn(Opcodes.ALOAD, 5);
        mv.visitInsn(Opcodes.ICONST_3);
        mv.visitVarInsn(Opcodes.ALOAD, 6);
        mv.visitInsn(Opcodes.AASTORE);
        mv.visitVarInsn(Opcodes.ALOAD, 4);
        mv.visitTypeInsn(Opcodes.CHECKCAST, fn);
        mv.visitVarInsn(Opcodes.ALOAD, 5);
        mv.visitMethodInsn(Opcodes.INVOKEINTERFACE, fn, "apply", apply, true);
        mv.visitInsn(Opcodes.POP);
        mv.visitVarInsn(Opcodes.ALOAD, 6);
        mv.visitTypeInsn(Opcodes.CHECKCAST, "[B");
        mv.visitInsn(Opcodes.ARETURN);

        mv.visitLabel(plain);
        mv.visitVarInsn(Opcodes.ALOAD, 0);
        mv.visitVarInsn(Opcodes.ALOAD, 1);
        mv.visitVarInsn(Opcodes.ALOAD, 2);
        mv.visitVarInsn(Opcodes.ALOAD, 3);
        mv.visitMethodInsn(Opcodes.INVOKESPECIAL, TARGET, ORIGINAL, desc, false);
        mv.visitInsn(Opcodes.ARETURN);
        mv.visitMaxs(0, 0);
        mv.visitEnd();
    }

    /**
     * MixinProcessor names merged members with a random per-session id
     * (UUID.randomUUID()). Cached classes from one launch would then reference
     * names that classes transformed in the next launch don't have. Derive the
     * id from the cache key instead: UUID.nameUUIDFromBytes(prop.getBytes()).
     */
    public static byte[] patchMixinProcessor(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        int[] count = {0};
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                if (!n.equals("<init>")) return mv;
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitMethodInsn(int op, String owner, String name, String desc, boolean itf) {
                        if (op == Opcodes.INVOKESTATIC && owner.equals("java/util/UUID") && name.equals("randomUUID")) {
                            count[0]++;
                            super.visitLdcInsn("ika.boost.session");
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/String", "getBytes", "()[B", false);
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/util/UUID", "nameUUIDFromBytes", "([B)Ljava/util/UUID;", false);
                            return;
                        }
                        super.visitMethodInsn(op, owner, name, desc, itf);
                    }
                    @Override
                    public void visitMaxs(int stack, int locals) {
                        super.visitMaxs(stack + 2, locals);
                    }
                };
            }
        }, 0);
        if (count[0] != 1) {
            System.err.println("[IkaBoost] unknown MixinProcessor layout (" + count[0] + " UUIDs)");
            return null;
        }
        System.err.println("[IkaBoost] Mixin session id fixed");
        return writer.toByteArray();
    }

    /**
     * InnerClassGenerator names the synthetic copies of mixin inner classes
     * Target$Anonymous$<random UUID>. Make the UUID a function of the session
     * id, the original inner class and the target class so it is stable
     * across launches.
     */
    public static byte[] patchInnerClassGenerator(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        int[] count = {0};
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                if (!n.equals("getUniqueReference")
                        || !d.equals("(Ljava/lang/String;Lorg/spongepowered/asm/mixin/transformer/ClassInfo;)Ljava/lang/String;")
                        || (access & Opcodes.ACC_STATIC) == 0) return mv;
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitMethodInsn(int op, String owner, String name, String desc, boolean itf) {
                        if (op == Opcodes.INVOKESTATIC && owner.equals("java/util/UUID") && name.equals("randomUUID")) {
                            count[0]++;
                            super.visitTypeInsn(Opcodes.NEW, "java/lang/StringBuilder");
                            super.visitInsn(Opcodes.DUP);
                            super.visitMethodInsn(Opcodes.INVOKESPECIAL, "java/lang/StringBuilder", "<init>", "()V", false);
                            super.visitLdcInsn("ika.boost.session");
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "getProperty", "(Ljava/lang/String;)Ljava/lang/String;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                            super.visitLdcInsn("|");
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                            super.visitVarInsn(Opcodes.ALOAD, 0);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                            super.visitLdcInsn("|");
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                            super.visitVarInsn(Opcodes.ALOAD, 1);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "org/spongepowered/asm/mixin/transformer/ClassInfo", "getName", "()Ljava/lang/String;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "toString", "()Ljava/lang/String;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/String", "getBytes", "()[B", false);
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/util/UUID", "nameUUIDFromBytes", "([B)Ljava/util/UUID;", false);
                            return;
                        }
                        super.visitMethodInsn(op, owner, name, desc, itf);
                    }
                    @Override
                    public void visitMaxs(int stack, int locals) {
                        super.visitMaxs(stack + 4, locals);
                    }
                };
            }
        }, 0);
        if (count[0] != 1) {
            System.err.println("[IkaBoost] unknown InnerClassGenerator layout (" + count[0] + " UUIDs)");
            return null;
        }
        return writer.toByteArray();
    }

    /**
     * cpw.mods.jarhandling.impl.Jar.getPackages() walks every entry of the jar
     * (455 mods + libraries, every launch). Wrap it with a lookup keyed by the
     * jar file: Function "ika.boost.pkgs" gets Object[]{primaryPath, null} and
     * returns a Set or null; after a miss it gets Object[]{primaryPath, set}.
     */
    public static byte[] patchJar(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        boolean[] found = new boolean[1];
        reader.accept(new ClassVisitor(Opcodes.ASM9) {
            @Override
            public MethodVisitor visitMethod(int a, String n, String d, String s, String[] e) {
                if (n.equals("getPackages") && d.equals("()Ljava/util/Set;")) found[0] = true;
                return null;
            }
        }, ClassReader.SKIP_CODE);
        if (!found[0]) return null;
        ClassWriter writer = new ClassWriter(reader, ClassWriter.COMPUTE_FRAMES) {
            @Override
            protected String getCommonSuperClass(String a, String b) { return "java/lang/Object"; }
        };
        String owner = "cpw/mods/jarhandling/impl/Jar";
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                if (n.equals("getPackages") && d.equals("()Ljava/util/Set;")) {
                    MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                    String fn = "java/util/function/Function";
                    String apply = "(Ljava/lang/Object;)Ljava/lang/Object;";
                    Label plain = new Label(), miss = new Label();
                    mv.visitCode();
                    mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "getProperties", "()Ljava/util/Properties;", false);
                    mv.visitLdcInsn("ika.boost.pkgs");
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/util/Properties", "get", "(Ljava/lang/Object;)Ljava/lang/Object;", false);
                    mv.visitVarInsn(Opcodes.ASTORE, 1);
                    mv.visitVarInsn(Opcodes.ALOAD, 1);
                    mv.visitJumpInsn(Opcodes.IFNULL, plain);
                    mv.visitInsn(Opcodes.ICONST_2);
                    mv.visitTypeInsn(Opcodes.ANEWARRAY, "java/lang/Object");
                    mv.visitVarInsn(Opcodes.ASTORE, 2);
                    mv.visitVarInsn(Opcodes.ALOAD, 2);
                    mv.visitInsn(Opcodes.ICONST_0);
                    mv.visitVarInsn(Opcodes.ALOAD, 0);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, owner, "getPrimaryPath", "()Ljava/nio/file/Path;", false);
                    mv.visitInsn(Opcodes.AASTORE);
                    mv.visitVarInsn(Opcodes.ALOAD, 1);
                    mv.visitTypeInsn(Opcodes.CHECKCAST, fn);
                    mv.visitVarInsn(Opcodes.ALOAD, 2);
                    mv.visitMethodInsn(Opcodes.INVOKEINTERFACE, fn, "apply", apply, true);
                    mv.visitVarInsn(Opcodes.ASTORE, 3);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitJumpInsn(Opcodes.IFNULL, miss);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitTypeInsn(Opcodes.CHECKCAST, "java/util/Set");
                    mv.visitInsn(Opcodes.ARETURN);
                    mv.visitLabel(miss);
                    mv.visitVarInsn(Opcodes.ALOAD, 0);
                    mv.visitMethodInsn(Opcodes.INVOKESPECIAL, owner, "ika$getPackages", "()Ljava/util/Set;", false);
                    mv.visitVarInsn(Opcodes.ASTORE, 3);
                    mv.visitVarInsn(Opcodes.ALOAD, 2);
                    mv.visitInsn(Opcodes.ICONST_1);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitInsn(Opcodes.AASTORE);
                    mv.visitVarInsn(Opcodes.ALOAD, 1);
                    mv.visitTypeInsn(Opcodes.CHECKCAST, fn);
                    mv.visitVarInsn(Opcodes.ALOAD, 2);
                    mv.visitMethodInsn(Opcodes.INVOKEINTERFACE, fn, "apply", apply, true);
                    mv.visitInsn(Opcodes.POP);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitInsn(Opcodes.ARETURN);
                    mv.visitLabel(plain);
                    mv.visitVarInsn(Opcodes.ALOAD, 0);
                    mv.visitMethodInsn(Opcodes.INVOKESPECIAL, owner, "ika$getPackages", "()Ljava/util/Set;", false);
                    mv.visitInsn(Opcodes.ARETURN);
                    mv.visitMaxs(0, 0);
                    mv.visitEnd();
                    int priv = (access & ~(Opcodes.ACC_PUBLIC | Opcodes.ACC_PROTECTED | Opcodes.ACC_SYNCHRONIZED)) | Opcodes.ACC_PRIVATE;
                    return super.visitMethod(priv, "ika$getPackages", d, s, e);
                }
                return super.visitMethod(access, n, d, s, e);
            }
        }, 0);
        System.err.println("[IkaBoost] jar package index enabled");
        return writer.toByteArray();
    }

    /**
     * jdk.nio.zipfs.ZipFileSystem.<init> calls Files.isWritable(zip) for every
     * jar Forge opens. On Windows that is an ACL query (GetFileSecurity +
     * GetVolumePathName + AccessCheck) costing ~0.2 ms-several ms per jar, and
     * Forge opens hundreds of jars, several times. Mods are only read, so treat
     * every zip filesystem opened from a .jar as read-only.
     */
    public static byte[] patchZipFileSystem(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        String owner = "jdk/nio/zipfs/ZipFileSystem";
        int[] count = {0};
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                // Java 17/21: inside a lambda of <init>; Java 25: directly in <init>.
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitMethodInsn(int op, String o, String name, String desc, boolean itf) {
                        if (op == Opcodes.INVOKESTATIC && o.equals("java/nio/file/Files") && name.equals("isWritable")
                                && desc.equals("(Ljava/nio/file/Path;)Z")) {
                            count[0]++;
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, owner, "ika$isWritable", desc, false);
                            return;
                        }
                        super.visitMethodInsn(op, o, name, desc, itf);
                    }
                };
            }

            @Override
            public void visitEnd() {
                // static boolean ika$isWritable(Path p) {
                //     return p.toString().endsWith(".jar") ? false : Files.isWritable(p);
                // }
                MethodVisitor mv = super.visitMethod(Opcodes.ACC_PRIVATE | Opcodes.ACC_STATIC | Opcodes.ACC_SYNTHETIC,
                        "ika$isWritable", "(Ljava/nio/file/Path;)Z", null, null);
                mv.visitCode();
                Label check = new Label();
                mv.visitVarInsn(Opcodes.ALOAD, 0);
                mv.visitMethodInsn(Opcodes.INVOKEINTERFACE, "java/nio/file/Path", "toString", "()Ljava/lang/String;", true);
                mv.visitLdcInsn(".jar");
                mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/String", "endsWith", "(Ljava/lang/String;)Z", false);
                mv.visitJumpInsn(Opcodes.IFEQ, check);
                mv.visitInsn(Opcodes.ICONST_0);
                mv.visitInsn(Opcodes.IRETURN);
                mv.visitLabel(check);
                mv.visitFrame(Opcodes.F_SAME, 0, null, 0, null);
                mv.visitVarInsn(Opcodes.ALOAD, 0);
                mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/nio/file/Files", "isWritable", "(Ljava/nio/file/Path;)Z", false);
                mv.visitInsn(Opcodes.IRETURN);
                mv.visitMaxs(2, 1);
                mv.visitEnd();
                super.visitEnd();
            }
        }, 0);
        if (count[0] != 1) {
            System.err.println("[IkaBoost] unknown ZipFileSystem layout (" + count[0] + ")");
            return null;
        }
        return writer.toByteArray();
    }

    /**
     * Forge scans every mod jar for annotations (@Mod, @EventBusSubscriber...)
     * with a single-thread executor. Use a pool with one thread per core and
     * make the completion callback synchronized (it updates plain ArrayLists).
     */
    public static byte[] patchBackgroundScan(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        int[] count = {0, 0};
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                if (n.equals("addCompletedFile")) {
                    count[1]++;
                    access |= Opcodes.ACC_SYNCHRONIZED;
                }
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                if (!n.equals("<init>")) return mv;
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitMethodInsn(int op, String owner, String name, String desc, boolean itf) {
                        if (op == Opcodes.INVOKESTATIC && owner.equals("java/util/concurrent/Executors")
                                && name.equals("newSingleThreadExecutor")
                                && desc.equals("(Ljava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;")) {
                            count[0]++;
                            // stack: factory -> int, factory
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/Runtime", "getRuntime", "()Ljava/lang/Runtime;", false);
                            super.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/Runtime", "availableProcessors", "()I", false);
                            super.visitInsn(Opcodes.SWAP);
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, "java/util/concurrent/Executors", "newFixedThreadPool",
                                    "(ILjava/util/concurrent/ThreadFactory;)Ljava/util/concurrent/ExecutorService;", false);
                            return;
                        }
                        super.visitMethodInsn(op, owner, name, desc, itf);
                    }
                    @Override
                    public void visitMaxs(int stack, int locals) {
                        super.visitMaxs(stack + 2, locals);
                    }
                };
            }
        }, 0);
        if (count[0] != 1 || count[1] != 1) {
            System.err.println("[IkaBoost] unknown BackgroundScanHandler layout");
            return null;
        }
        System.err.println("[IkaBoost] parallel mod scan enabled");
        return writer.toByteArray();
    }

    /**
     * While the loading overlay is up, Minecraft caps the frame rate at 60 and
     * parks the render thread between frames (RenderSystem.limitDisplayFPS). The
     * resource reload hands work to that thread through Minecraft's task queue
     * (f_91023_ / BlockableEventLoop), so every hand-off waits for the next frame.
     * Replace the wait during loading with "run queued tasks until the frame is due".
     */
    public static byte[] patchMinecraftFrameLimit(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        int[] count = {0};
        String mc = "net/minecraft/client/Minecraft";
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                if (!n.equals("m_91383_") || !d.equals("(Z)V")) return mv;
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitMethodInsn(int op, String owner, String name, String desc, boolean itf) {
                        if (op == Opcodes.INVOKESTATIC && owner.equals("com/mojang/blaze3d/systems/RenderSystem")
                                && name.equals("limitDisplayFPS") && desc.equals("(I)V")) {
                            count[0]++;
                            // stack: fps -> (fps, this) -> ika$limit(fps)
                            super.visitVarInsn(Opcodes.ALOAD, 0);
                            super.visitMethodInsn(Opcodes.INVOKESTATIC, mc, "ika$limit", "(ILnet/minecraft/client/Minecraft;)V", false);
                            return;
                        }
                        super.visitMethodInsn(op, owner, name, desc, itf);
                    }
                };
            }

            @Override
            public void visitEnd() {
                // static void ika$limit(int fps, Minecraft mc) {
                //     if (mc.f_91081_ == null) { RenderSystem.limitDisplayFPS(fps); return; }
                //     double due = RenderSystem.lastDrawTime + 1.0 / fps;   (private: recompute)
                //     long end = System.nanoTime() + 1_000_000_000L / fps;
                //     while (System.nanoTime() < end) {
                //         Runnable r = (Runnable) mc.f_91023_.poll();
                //         if (r != null) r.run(); else Thread.onSpinWait / LockSupport.parkNanos(200_000);
                //     }
                // }
                MethodVisitor mv = super.visitMethod(Opcodes.ACC_PRIVATE | Opcodes.ACC_STATIC | Opcodes.ACC_SYNTHETIC,
                        "ika$limit", "(ILnet/minecraft/client/Minecraft;)V", null, null);
                mv.visitCode();
                Label overlay = new Label(), loop = new Label(), idle = new Label(), done = new Label();
                mv.visitVarInsn(Opcodes.ALOAD, 1);
                mv.visitFieldInsn(Opcodes.GETFIELD, mc, "f_91081_", "Lnet/minecraft/client/gui/screens/Overlay;");
                mv.visitJumpInsn(Opcodes.IFNONNULL, overlay);
                mv.visitVarInsn(Opcodes.ILOAD, 0);
                mv.visitMethodInsn(Opcodes.INVOKESTATIC, "com/mojang/blaze3d/systems/RenderSystem", "limitDisplayFPS", "(I)V", false);
                mv.visitInsn(Opcodes.RETURN);
                mv.visitLabel(overlay);
                mv.visitFrame(Opcodes.F_SAME, 0, null, 0, null);
                // end = nanoTime + 1e9 / fps  (local 2-3: long)
                mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "nanoTime", "()J", false);
                mv.visitLdcInsn(1_000_000_000L);
                mv.visitVarInsn(Opcodes.ILOAD, 0);
                mv.visitInsn(Opcodes.I2L);
                mv.visitInsn(Opcodes.LDIV);
                mv.visitInsn(Opcodes.LADD);
                mv.visitVarInsn(Opcodes.LSTORE, 2);
                mv.visitLabel(loop);
                mv.visitFrame(Opcodes.F_APPEND, 1, new Object[]{Opcodes.LONG}, 0, null);
                mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/lang/System", "nanoTime", "()J", false);
                mv.visitVarInsn(Opcodes.LLOAD, 2);
                mv.visitInsn(Opcodes.LCMP);
                mv.visitJumpInsn(Opcodes.IFGE, done);
                // BlockableEventLoop.pollTask(): runs one queued main-thread task
                mv.visitVarInsn(Opcodes.ALOAD, 1);
                mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, mc, "m_7245_", "()Z", false);
                mv.visitJumpInsn(Opcodes.IFEQ, idle);
                mv.visitJumpInsn(Opcodes.GOTO, loop);
                mv.visitLabel(idle);
                mv.visitFrame(Opcodes.F_SAME, 0, null, 0, null);
                mv.visitLdcInsn(200_000L);
                mv.visitMethodInsn(Opcodes.INVOKESTATIC, "java/util/concurrent/locks/LockSupport", "parkNanos", "(J)V", false);
                mv.visitJumpInsn(Opcodes.GOTO, loop);
                mv.visitLabel(done);
                mv.visitFrame(Opcodes.F_SAME, 0, null, 0, null);
                mv.visitInsn(Opcodes.RETURN);
                mv.visitMaxs(6, 4);
                mv.visitEnd();
                super.visitEnd();
            }
        }, 0);
        if (count[0] != 1) {
            System.err.println("[IkaBoost] unknown Minecraft frame limiter (" + count[0] + ")");
            return null;
        }
        System.err.println("[IkaBoost] loading-screen task pump enabled");
        return writer.toByteArray();
    }

    /**
     * Mixin's Bytecode.getOpcodeName(int, String, int) scans Opcodes' fields by
     * reflection on every call, and Mixin calls it for every instruction of every
     * mixin method (for its debug activity stack). Memoize the result per
     * (opcode, start, min): same strings, no reflection after the first call.
     */
    public static byte[] patchMixinBytecode(byte[] bytes) {
        ClassReader reader = new ClassReader(bytes);
        ClassWriter writer = new ClassWriter(reader, 0);
        String owner = "org/spongepowered/asm/util/Bytecode";
        String desc = "(ILjava/lang/String;I)Ljava/lang/String;";
        int[] count = {0};
        reader.accept(new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                if (n.equals("getOpcodeName") && d.equals(desc) && (access & Opcodes.ACC_STATIC) != 0) {
                    count[0]++;
                    // Wrapper: key = opcode | min<<16 ^ start.hashCode()<<20 (start is a constant per call site)
                    MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                    mv.visitCode();
                    Label miss = new Label();
                    // ConcurrentHashMap cache = ika$opcodes;
                    mv.visitFieldInsn(Opcodes.GETSTATIC, owner, "ika$opcodes", "Ljava/util/concurrent/ConcurrentHashMap;");
                    mv.visitVarInsn(Opcodes.ASTORE, 3);
                    // String key = opcode + ":" + start + ":" + min
                    mv.visitTypeInsn(Opcodes.NEW, "java/lang/StringBuilder");
                    mv.visitInsn(Opcodes.DUP);
                    mv.visitMethodInsn(Opcodes.INVOKESPECIAL, "java/lang/StringBuilder", "<init>", "()V", false);
                    mv.visitVarInsn(Opcodes.ILOAD, 0);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(I)Ljava/lang/StringBuilder;", false);
                    mv.visitIntInsn(Opcodes.BIPUSH, 58);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(C)Ljava/lang/StringBuilder;", false);
                    mv.visitVarInsn(Opcodes.ALOAD, 1);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(Ljava/lang/String;)Ljava/lang/StringBuilder;", false);
                    mv.visitIntInsn(Opcodes.BIPUSH, 58);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(C)Ljava/lang/StringBuilder;", false);
                    mv.visitVarInsn(Opcodes.ILOAD, 2);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "append", "(I)Ljava/lang/StringBuilder;", false);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/lang/StringBuilder", "toString", "()Ljava/lang/String;", false);
                    mv.visitVarInsn(Opcodes.ASTORE, 4);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitVarInsn(Opcodes.ALOAD, 4);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/util/concurrent/ConcurrentHashMap", "get", "(Ljava/lang/Object;)Ljava/lang/Object;", false);
                    mv.visitTypeInsn(Opcodes.CHECKCAST, "java/lang/String");
                    mv.visitVarInsn(Opcodes.ASTORE, 5);
                    mv.visitVarInsn(Opcodes.ALOAD, 5);
                    mv.visitJumpInsn(Opcodes.IFNULL, miss);
                    mv.visitVarInsn(Opcodes.ALOAD, 5);
                    mv.visitInsn(Opcodes.ARETURN);
                    mv.visitLabel(miss);
                    mv.visitFrame(Opcodes.F_FULL, 6, new Object[]{Opcodes.INTEGER, "java/lang/String", Opcodes.INTEGER,
                            "java/util/concurrent/ConcurrentHashMap", "java/lang/String", "java/lang/String"}, 0, new Object[0]);
                    mv.visitVarInsn(Opcodes.ILOAD, 0);
                    mv.visitVarInsn(Opcodes.ALOAD, 1);
                    mv.visitVarInsn(Opcodes.ILOAD, 2);
                    mv.visitMethodInsn(Opcodes.INVOKESTATIC, owner, "ika$getOpcodeName", desc, false);
                    mv.visitVarInsn(Opcodes.ASTORE, 5);
                    mv.visitVarInsn(Opcodes.ALOAD, 3);
                    mv.visitVarInsn(Opcodes.ALOAD, 4);
                    mv.visitVarInsn(Opcodes.ALOAD, 5);
                    mv.visitMethodInsn(Opcodes.INVOKEVIRTUAL, "java/util/concurrent/ConcurrentHashMap", "put", "(Ljava/lang/Object;Ljava/lang/Object;)Ljava/lang/Object;", false);
                    mv.visitInsn(Opcodes.POP);
                    mv.visitVarInsn(Opcodes.ALOAD, 5);
                    mv.visitInsn(Opcodes.ARETURN);
                    mv.visitMaxs(4, 6);
                    mv.visitEnd();
                    return super.visitMethod((access & ~(Opcodes.ACC_PUBLIC | Opcodes.ACC_PROTECTED)) | Opcodes.ACC_PRIVATE,
                            "ika$getOpcodeName", d, s, e);
                }
                return super.visitMethod(access, n, d, s, e);
            }

            @Override
            public void visitEnd() {
                super.visitField(Opcodes.ACC_PRIVATE | Opcodes.ACC_STATIC | Opcodes.ACC_FINAL | Opcodes.ACC_SYNTHETIC,
                        "ika$opcodes", "Ljava/util/concurrent/ConcurrentHashMap;", null, null).visitEnd();
                // static initializer helper: set in <clinit> wrapper below
                super.visitEnd();
            }
        }, 0);
        if (count[0] != 1) {
            System.err.println("[IkaBoost] unknown Mixin Bytecode layout");
            return null;
        }
        // Add the field initialization at the start of <clinit>.
        ClassReader r2 = new ClassReader(writer.toByteArray());
        ClassWriter w2 = new ClassWriter(r2, 0);
        boolean[] hasClinit = {false};
        r2.accept(new ClassVisitor(Opcodes.ASM9, w2) {
            @Override
            public MethodVisitor visitMethod(int access, String n, String d, String s, String[] e) {
                MethodVisitor mv = super.visitMethod(access, n, d, s, e);
                if (!n.equals("<clinit>")) return mv;
                hasClinit[0] = true;
                return new MethodVisitor(Opcodes.ASM9, mv) {
                    @Override
                    public void visitCode() {
                        super.visitCode();
                        super.visitTypeInsn(Opcodes.NEW, "java/util/concurrent/ConcurrentHashMap");
                        super.visitInsn(Opcodes.DUP);
                        super.visitMethodInsn(Opcodes.INVOKESPECIAL, "java/util/concurrent/ConcurrentHashMap", "<init>", "()V", false);
                        super.visitFieldInsn(Opcodes.PUTSTATIC, owner, "ika$opcodes", "Ljava/util/concurrent/ConcurrentHashMap;");
                    }
                    @Override
                    public void visitMaxs(int stack, int locals) {
                        super.visitMaxs(Math.max(stack, 2), locals);
                    }
                };
            }
            @Override
            public void visitEnd() {
                if (!hasClinit[0]) {
                    MethodVisitor mv = super.visitMethod(Opcodes.ACC_STATIC, "<clinit>", "()V", null, null);
                    mv.visitCode();
                    mv.visitTypeInsn(Opcodes.NEW, "java/util/concurrent/ConcurrentHashMap");
                    mv.visitInsn(Opcodes.DUP);
                    mv.visitMethodInsn(Opcodes.INVOKESPECIAL, "java/util/concurrent/ConcurrentHashMap", "<init>", "()V", false);
                    mv.visitFieldInsn(Opcodes.PUTSTATIC, owner, "ika$opcodes", "Ljava/util/concurrent/ConcurrentHashMap;");
                    mv.visitInsn(Opcodes.RETURN);
                    mv.visitMaxs(2, 0);
                    mv.visitEnd();
                }
                super.visitEnd();
            }
        }, 0);
        System.err.println("[IkaBoost] Mixin opcode names memoized");
        return w2.toByteArray();
    }
}
