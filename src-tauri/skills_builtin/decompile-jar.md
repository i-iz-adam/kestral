# Decompiling and rebuilding a JVM jar

Use this for "unpack this jar / decompile this class / recover source from this obfuscated app" style tasks — including ones where the jar's origin is unknown, which is exactly the case to be more careful about (see Sandboxing below).

This is naturally a long-running, many-step job (unpack → decompile per-package → fix compile errors → verify → repeat). Call `update_plan` early with the major phases below as steps, and keep it current as you go — that's what keeps a task like this coherent across the number of tool calls it actually takes, since the raw conversation history may get summarized well before the task is done.

## Sandboxing
Code recovered from an unknown or obfuscated jar is, by definition, untrusted — decompiled output that gets compiled and *run* (not just read) is a meaningfully different risk than ordinary source you were handed. If this session doesn't already have `sandbox_shell` on, say so explicitly and suggest the user turn it on before you run anything beyond static unpacking/decompiling (jar extraction and decompilation themselves are read-only and safe to do unsandboxed; compiling and *executing* the recovered code is the part worth isolating). A sandboxed run will need `sandbox_network` on too, at least for the first setup step, to install a JDK/build tool inside the sandbox — see Toolchain setup below.

## Toolchain setup
Check what's already on PATH before installing anything (`java -version`, `javac -version`, `./gradlew -v` or `mvn -v` if a wrapper/build file is present).

For decompiling, prefer a modern, actively-maintained decompiler over an old one — they materially differ in how well they handle obfuscated/minified bytecode:
- **Vineflower** (successor to Fernflower) — best general-purpose choice, good at reconstructing lambdas/switch expressions/records. Runs as a jar: `java -jar vineflower.jar -o=out_dir input.jar`.
- **CFR** — strong alternative, sometimes recovers cleaner output on heavily obfuscated code CFR-specific quirks trip up. `java -jar cfr.jar input.jar --outputdir out_dir`.
- Try both on a sample class if the first one's output looks mangled (renamed-but-inconsistent locals, failed control-flow reconstruction) before committing to decompiling the whole jar with one tool.

If the jar is obfuscated (single/double-letter class and member names, string encryption, control-flow flattening), decompiling alone won't undo the obfuscation — it just turns bytecode back into compilable-looking Java with the same short names. Don't spend effort manually renaming everything upfront; get it building first, then rename incrementally as you understand what each piece does, only where it actually helps the task at hand.

## Workflow
1. **Unpack**: `unzip` (or `jar xf`) the jar into a working directory. Skim `META-INF/MANIFEST.MF` for the main class and any `Class-Path` entries — tells you what's an entry point vs. a bundled dependency.
2. **Decompile**: run the chosen decompiler against the whole jar (or per-package if it's large enough that one pass risks timing out — see run_shell's timeout_seconds). Keep the original jar around unmodified as a reference; work in a separate `src/` tree for decompiled output.
3. **Set up a build**: create a minimal build file (Maven `pom.xml` or Gradle `build.gradle`) targeting the same Java version as the original bytecode (check the class file major version, or `javap -verbose` on one class, to infer it) with the decompiled sources and bundled dependency jars on the classpath.
4. **Iterate on compile errors**: decompiled code frequently doesn't compile as-is (synthetic bridge methods, ambiguous overloads a decompiler couldn't fully resolve, anonymous-class numbering artifacts). Fix these in small batches — get a shrinking, stable count of errors per pass rather than trying to fix everything from one giant error dump. If a fix undoes progress (error count goes back up), reconsider that fix rather than layering another one on top.
5. **Verify against the original**: once it builds, don't assume it's correct — run whatever the task calls for (the app's own entry point, its test suite if one exists) and compare behavior against the original jar where you can (same inputs, same outputs). For anything you renamed or restructured, a quick bytecode-level sanity check (`javap -c` on both the original class and your recompiled one, looking for the same rough instruction shape) catches an accidental behavior change that still "compiles fine."
6. **Report clearly** what's now understood vs. still opaque (obfuscated names you didn't get to, code paths you couldn't exercise) rather than implying full recovery if it wasn't achieved.
