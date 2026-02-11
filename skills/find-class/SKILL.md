---
name: "find-class"
description: "当无法通过grep查找类的实现时，使用全类名从本地Maven仓库查找Java类并返回反编译源码。"
---

# find-class

使用全类名从本地 Maven 仓库中查找 Java 类的反编译源码。

## 使用规范

**必须使用全类名（包含完整包名），例如：`org.springframework.stereotype.Component`**

如需查看更多边界场景与排障方法，参考：`references/REFERENCE.md`

### 查找类源码

```bash
class-finder org.springframework.stereotype.Component --code-only
```

### 指定版本

```bash
class-finder org.springframework.stereotype.Component --version 6.2.8 --code-only
```

### 保存到文件

```bash
class-finder org.springframework.stereotype.Component --code-only -o Component.java
```

## 输出要求

- 默认优先使用 `--code-only`
- 未命中时给出下一步排查建议（版本、仓库路径、是否已下载依赖）
