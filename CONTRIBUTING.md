## 贡献指南

感谢你愿意参与贡献。

### 开发环境

- Rust stable（建议用 `rustup` 安装）
- macOS / Linux / Windows 均可
- 运行时反编译依赖 `java`（JRE/JDK），但单测与集成测试会 mock `java`，无需真实安装

### 本地开发

```bash
cargo test
```

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

### 代码风格

- 保持模块职责单一：scan / registry / cache / warmup / hotspot / incremental
- 避免打印/记录敏感路径与环境变量（除非用户显式开启 verbose）
- 对外接口优先返回结构化 JSON（CLI 默认输出 JSON）

### 提交 PR 前自检

- `cargo test` 通过
- `cargo fmt` 之后无 diff
- `cargo clippy` 无 warning
- README / CLI 帮助信息同步更新（如新增命令/参数）

