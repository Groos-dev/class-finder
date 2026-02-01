## Find Class（class-finder）

当无法在工作目录查到某个类的实现时，通过该 Skill 查询 jar 中的类实现（反编译源码）。

### 你需要提供
- 类名（推荐全限定名，例如 `org.apache.commons.lang3.StringUtils`）
- 可选：版本号（如果你想指定 Maven 版本）

### 我会做什么
- 在本地 `~/.m2/repository` 中定位包含该类的 jar
- 反编译并返回源码（必要时会缓存结果以加速后续查询）

### 示例
- 查找某个类的实现：
  - `class-finder find org.apache.commons.lang3.StringUtils`
- 指定版本：
  - `class-finder find org.apache.commons.lang3.StringUtils --version 3.12.0`
