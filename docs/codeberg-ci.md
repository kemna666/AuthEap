# Codeberg CI/CD

本仓库同时保留了 **Woodpecker** 与 **Forgejo Actions** 两套配置，但 Codeberg 上
**建议只启用其中一套**，避免同一提交被重复构建、并发创建 Release。

| 方案 | 文件 | 适用 | Codeberg 托管 |
| --- | --- | --- | --- |
| Woodpecker（推荐） | `.woodpecker.yml` | Codeberg 官方托管 CI | 需要申请/激活 |
| Forgejo Actions | `.forgejo/workflows/*.yml` | 自建 Forgejo runner；或 Codeberg 托管 Actions alpha | 限量 2/5/10 分钟 |

Codeberg 官方文档目前建议：需要 Codeberg 托管 CI 时优先使用 **Woodpecker**；
Forgejo Actions 的托管版本仍是 limited alpha，且单个 job 有时限、不能跑 Docker daemon。
本项目包含 Rust 编译 + 7 类 ABI 的 musl 交叉编译，量比较大，所以：

- **Woodpecker**：push/PR 只跑构建测试；**只有 tag / 手动 Pipeline** 才做全部交叉编译
  和 Release，避免每次提交都消耗大量 CI 资源。
- **Forgejo Actions**：CI 与 Release 都拆成按目标矩阵的短 job；Release 用 Forgejo
  维护的 artifact action 聚合产物后再统一发布。若使用 Codeberg 托管 alpha 且
  mipsel job 超 10 分钟，建议改用自建 runner，或只保留 Woodpecker。

---

## 一、Woodpecker（推荐）

1. 在 Codeberg 申请 Woodpecker 访问权限：
   <https://codeberg.org/Codeberg-e.V./requests>（选择 Woodpecker CI 模板）。
2. 通过后登录 <https://ci.codeberg.org>，添加并激活本仓库。
3. 添加发布令牌（只有自动 Release 需要）：
   - Codeberg → Settings → Applications → Generate new token；
   - 权限包含 **write:repository**；
   - 在 Woodpecker 仓库设置的 Secrets 中添加：
     - 名称：`release_token`
     - 值：上一步的令牌。
4. 推送到 `main` 或发起 PR 即可看到 `build-test`。
5. 发布时二选一：

   ```bash
   # 方式 A：推 tag（版本号直接用 tag，去掉 v 前缀）
   git tag 0.1.42
   git push origin 0.1.42

   # 方式 B：在 Woodpecker 页面手动 Run pipeline（manual 事件）
   ```

   发布步骤会生成 `.ipk` / `.apk` / LuCI 包 / 桌面 `.deb`，并调用
   `scripts/publish-release.sh` 创建/更新 Codeberg Release。

### 支持的路由器架构

CI 按 **ABI 代表性架构** 矩阵构建；同一个二进制可以覆盖同 ABI 的多个 OpenWrt
子架构，需要时把下面的 `ipk_arch` / `apk_arch` 改成固件实际的软件包架构名即可。

| OpenWrt 架构（示例） | Rust target | 交叉链接器 / 工具链 | 说明 |
| --- | --- | --- | --- |
| `x86_64` | `x86_64-unknown-linux-musl` | 系统 `cc` | 软路由 / x86 |
| `aarch64_cortex-a53` | `aarch64-unknown-linux-musl` | `aarch64-linux-gnu-gcc` | ARM64，可改 cortex-a72 等 |
| `arm_cortex-a7_neon-vfpv4` | `armv7-unknown-linux-musleabihf` | `arm-linux-gnueabihf-gcc` | ARMv7，可覆盖 cortex-a9/a15 |
| `arm_arm1176jzf-s_vfp` | `arm-unknown-linux-musleabihf` | `arm-linux-gnueabihf-gcc` | 老 ARMv6 |
| `riscv64_riscv64` | `riscv64gc-unknown-linux-musl` | `riscv64-linux-gnu-gcc` + 强制静态 musl | RISC-V 64 |
| `mipsel_24kc` | `mipsel-unknown-linux-musl` | musl.cc `mipsel-linux-muslsf-cross` | MT7621 等，软浮点 |
| `mips_24kc` | `mips-unknown-linux-musl` | musl.cc `mips-linux-muslsf-cross` | 大端 MIPS，软浮点 |

- `mipsel` / `mips` 的 Rust 标准库没有预编译版本，CI 会用 `-Z build-std` 和
  对应的 soft-float musl 工具链；`scripts/ci-build-package.sh` 会自动下载。
- `riscv64gc-unknown-linux-musl` 默认是动态 musl；CI 只有 glibc 交叉 gcc 时
  会缺少 `Scrt1.o`。构建脚本会强制 `+crt-static` 和 `link-self-contained=yes`，
  并在打包前校验 `statically linked`，避免链接到错误 libc。
- 每种架构生成一对 `AuthEAP-Client_<ver>-1_<arch>.ipk` 和
  `AuthEAP-Client-<ver>-r1_<arch>.apk`；文件名带架构，避免互相覆盖。
- 桌面 `.deb` 只在 x86_64 构建一份；它和所有路由架构的 ipk/apk、LuCI 包
  一起上传到 Release。
- Codeberg 托管 Forgejo Actions 单 job 限制 10 分钟；MIPS 的 build-std 较慢，
  若超时请改用 Woodpecker 或自建 runner。

版本号规则由 [`scripts/ci-version.sh`](../scripts/ci-version.sh) 决定：

- tag 事件：直接用 tag 名（建议 tag 使用 `0.1.42`，不要带 `v` 前缀）；
- 手动事件：取同版本线远端最大版本 +1；
- 普通 push/PR：使用 `Cargo.toml` 当前版本，只构建不发布。
- CI 全流程设置 `AUTHEAP_NO_VERSION_BUMP=1`，版本只由 `ci-version.sh` 计算一次；
  本地构建脚本默认会每次自动 patch+1，不要把这个行为带进多架构 CI。

> 未配置 `release_token` 时，tag/manual 流水线会在最后一步报错并停止发布；
> 这样不会出现“看起来成功但 Release 没产物”的情况。

---

## 二、Forgejo Actions（自建 runner / Codeberg alpha）

1. 仓库 Settings → Units → 勾选 **Actions**。
2. 配置 runner（自建推荐）或在 Codeberg 托管 alpha 中使用以下标签：
   `codeberg-tiny` / `codeberg-small` / `codeberg-medium`（可加 `-lazy`）。
3. 添加 Secret：`RELEASE_TOKEN`（同样需要 **write:repository**）。
4. push/PR 运行 `.forgejo/workflows/ci.yml`；
   推 tag 或手动触发 `.forgejo/workflows/release.yml`。
5. Release 流程：
   - `version` 计算版本；
   - `openwrt` 矩阵、`luci`、`desktop` 各自构建并上传 artifact；
   - `publish` 用 `forgejo/download-artifact@v4` 聚合后调用
     `scripts/publish-release.sh` 发布。

### 为什么使用 Forgejo 的 artifact action URL

Forgejo 上 **不能直接使用 GitHub 的 `actions/upload-artifact@v4`**（v4 后端 API
不兼容）。本仓库使用 Forgejo 维护的版本：

```yaml
- uses: https://data.forgejo.org/forgejo/upload-artifact@v4
- uses: https://data.forgejo.org/forgejo/download-artifact@v4
```

如果 runner 很旧只支持 v3，把上述 `v4` 改成 `v3`；但新版 Forgejo runner 建议直接用
v4。

---

## 三、发布令牌

Codeberg/Forgejo 的 Release 上传需要 access token，权限至少：

- `write:repository`（创建 Release、上传附件）。

不要用账号密码；令牌只放 CI Secret，不要提交到仓库。

---

## 四、常见问题

**没有流水线 / 一直是 queued**

- Woodpecker：仓库是否已在 ci.codeberg.org 激活？访问权限是否已通过审核？
- Forgejo Actions：仓库是否勾选 Actions？有没有匹配 `runs-on` 的 runner 在线？
- 两套都启用时，先确认你看的是哪一个 CI 系统。

**Release 没有附件 / 只发布了一部分**

- 检查 Secret 名称：Woodpecker 是 `release_token`，Forgejo 是 `RELEASE_TOKEN`；
- 检查 token 是否含 `write:repository`；
- 检查构建 job 是否成功、artifact 是否已上传/下载；
- 可先在本地用 `DRY_RUN=1` 检查发布参数：

  ```bash
  DRY_RUN=1 REPO_SLUG=owner/repo TOKEN=x \
    ./scripts/publish-release.sh 0.1.42 0.1.42 some.ipk
  ```

**版本号没有递增**

- 普通 push/PR 不会递增（CI 用 `CI_VERSION_NO_BUMP=1`）；
- 只有 tag 或 manual 发布事件才会递增；
- 手动发布时如果远端 API 不可达，会退回本地 tag；仍失败时可显式推一个 tag。

**mipsel job 超过 10 分钟（Codeberg 托管 Actions）**

- 这是 Codeberg 托管 Forgejo Actions 的单 job 限制；
- 使用自建 runner，或改用 Woodpecker 跑完整发布。

---

## 五、本地检查

提交 CI 配置前可运行：

```bash
./scripts/ci-checks.sh          # shell / JSON / JS / YAML
python3 -c 'import yaml; yaml.safe_load(open(".woodpecker.yml"))'
python3 -c 'import yaml; yaml.safe_load(open(".forgejo/workflows/release.yml"))'
```
