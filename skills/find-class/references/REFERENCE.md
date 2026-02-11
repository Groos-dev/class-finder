# find-class Reference

## Preconditions

- Prefer fully qualified class names (FQCN), for example `org.springframework.stereotype.Component`
- Ensure dependency jars are already present in local Maven repository

## Common Command Forms

### Basic FQCN lookup (recommended)

```bash
class-finder org.springframework.stereotype.Component --code-only
```

### Contract-first structure output (recommended for AI)

```bash
class-finder org.springframework.stereotype.Component --format structure
```

Use this when you only need class contract context (fields, method signatures, inheritance) without implementation details.

### Explicit `find` subcommand

```bash
class-finder find org.springframework.stereotype.Component --code-only
```

### Query by simple class name (fallback)

```bash
class-finder Component --code-only
```

If multiple classes share the same simple name, switch to FQCN.

### Pin a specific dependency version

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

### Use custom Maven repo, DB, and local CFR path

```bash
class-finder --m2 /data/m2 --db /data/class-finder.lmdb --cfr /tools/cfr.jar find org.example.Foo
```

### Use `CFR_JAR` env var instead of `--cfr`

```bash
CFR_JAR=/tools/cfr.jar class-finder org.example.Foo --code-only
```

### Write source to file (`--output`)

```bash
class-finder org.springframework.stereotype.Component --code-only --output /tmp/Component.java
```

### Write source to file (`-o` short flag)

```bash
class-finder org.springframework.stereotype.Component --code-only -o Component.java
```

### Save structure output to file

```bash
class-finder org.springframework.stereotype.Component --format structure --output /tmp/Component.structure.json
```

### Get JSON result (default)

```bash
class-finder org.springframework.stereotype.Component
```

### Get text summary

```bash
class-finder org.springframework.stereotype.Component --format text
```

### Equivalent implicit/explicit find forms

```bash
class-finder --db /tmp/cf.lmdb org.springframework.stereotype.Component
class-finder --db /tmp/cf.lmdb find org.springframework.stereotype.Component
```

## Troubleshooting

- If class is not found, verify the jar exists under `~/.m2/repository`
- If CFR download fails, pass `--cfr /path/to/cfr.jar`
- For ambiguous simple class names, always switch to FQCN
- If your dependencies are not under default `~/.m2/repository`, set `--m2 /your/maven/repo`
- If decompilation output is needed for downstream grep, use `--code-only` or `--format text`
- If your AI task is API understanding/review/planning, prefer `--format structure` before `--code-only`
