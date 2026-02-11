# find-class Reference

## Preconditions

- Use fully qualified class name (FQCN), for example `org.springframework.stereotype.Component`
- Ensure dependency jars are already present in local Maven repository

## Common Command Forms

```bash
class-finder org.springframework.stereotype.Component --code-only
```

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

```bash
class-finder --m2 /data/m2 --db /data/class-finder.redb --cfr /tools/cfr.jar find org.example.Foo
```

## Troubleshooting

- If class is not found, verify the jar exists under `~/.m2/repository`
- If CFR download fails, pass `--cfr /path/to/cfr.jar`
- For ambiguous simple class names, always switch to FQCN
