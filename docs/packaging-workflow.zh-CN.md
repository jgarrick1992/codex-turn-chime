# CodexTurnChime 完整打包与发布流程

本文档记录 CodexTurnChime 从版本准备、质量检查、本机打包、PR 合并、tag 到 GitHub Release 的完整流程。命令和约束以 `v0.1.0-beta.2` 实际发布流程为基础。

## 1. 支持目标与产物

| 平台 | 构建环境 | 分发产物 | 当前签名状态 |
| --- | --- | --- | --- |
| macOS | macOS 13+、Apple Silicon、Node.js 22、stable Rust | `.app`、ARM64 `.dmg` | ad-hoc signing，未 notarize |
| Windows | Windows 11 x64、Node.js 22、Rust MSVC、Visual Studio C++ Build Tools | x64 NSIS `.exe` | 未做 Authenticode 签名 |

beta 版本只构建 Windows NSIS 安装包。WiX/MSI 不接受 `0.1.0-beta.2` 这类带文字的预发布版本；MSI 留到纯数字稳定版本启用。

GitHub Release 的标准资产一共 6 个：

```text
CodexTurnChime_<version>_aarch64.dmg
CodexTurnChime_<version>_x64-setup.exe
CodexTurnChime-macos-arm64.spdx.json
CodexTurnChime-windows-x64.spdx.json
SHA256SUMS-macos-arm64.txt
SHA256SUMS-windows-x64.txt
```

GitHub build provenance 通过 Attestations 提供，不会额外显示为普通 Release 资产。

## 2. 发布主流程

完整顺序如下：

1. 确定新版本号并更新所有版本文件。
2. 更新 `CHANGELOG.md` 和发布说明。
3. 在本机运行前端与 Rust 检查。
4. 在 Apple Silicon Mac 上生成并验证 `.app`、`.dmg`。
5. 提交 release 分支并推送到 GitHub。
6. 创建 PR，等待 Frontend、Rust、依赖安全和 CodeQL 全部通过。
7. 通过 PR 合并到受保护的 `main`。
8. 从最新的远端 `main` 创建 annotated tag。
9. 推送 tag，触发 `.github/workflows/release.yml`。
10. 等待 macOS 与 Windows 安装包、SHA-256、SBOM 和 provenance 全部成功。
11. workflow 自动公开 Prerelease。
12. 下载发布资产并复核 SHA-256。

不要先在未合并的 release 分支上打 tag。tag 必须指向已经进入远端 `main` 的发布提交。

## 3. 更新版本号

以下文件中的版本必须保持一致，不增加旧 key 或别名映射：

```text
package.json
package-lock.json
src-tauri/Cargo.toml
src-tauri/Cargo.lock
src-tauri/tauri.conf.json
src/components/BrandMark.tsx
CHANGELOG.md
```

项目计划、Roadmap、README 或 Issue 模板中如果展示了当前公开版本，也要同步检查。

检查是否仍残留旧版本：

```bash
rg -n '0\.1\.0-beta\.1|0\.1\.0-beta\.2' \
  --glob '!node_modules/**' \
  --glob '!src-tauri/target/**'
```

发布日期使用发布所在地的实际日期。`CHANGELOG.md` 应保留空的 `[Unreleased]`，并为新版本增加独立章节。

## 4. 发布前检查

在项目根目录运行：

```bash
npm ci
npm run lint
npm run typecheck
npm test -- --run
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

如果本机安装了安全检查工具，再运行：

```bash
cd src-tauri
cargo audit
cargo deny --config ../deny.toml check
cd ..
```

还应检查：

```bash
git diff --check
git status --short
ruby -e "require 'yaml'; YAML.load_file('.github/workflows/release.yml'); puts 'release workflow yaml: ok'"
```

提交前不应包含这些生成目录：

```text
dist/
node_modules/
src-tauri/target/
src-tauri/binaries/
src-tauri/gen/
artifacts/
```

`src-tauri/Cargo.lock` 是源码发布需要的锁文件，应纳入 Git。

## 5. 本机打包 macOS ARM64

在 Apple Silicon Mac 的项目根目录运行：

```bash
bash scripts/build_macos_arm.sh
```

脚本执行以下工作：

1. 调用 `scripts/init_env.sh --skip-debug-sidecar` 检查 macOS、Xcode Command Line Tools、Node.js 22 和 Rust。
2. 运行 `npm run build` 生成 `dist/`。
3. 创建 Tauri build script 所需的目标 sidecar 占位文件。
4. 编译 release Hook helper。
5. 用真实 Hook helper 替换占位文件。
6. 构建 `.app` 和 `.dmg`。
7. 运行 `scripts/smoke_macos_app.sh`，验证生产 identifier、ARM64 主程序/helper、可执行权限、签名、独立启动和第二实例行为。

输出位置：

```text
src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CodexTurnChime.app
src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CodexTurnChime_<version>_aarch64.dmg
```

验证应用签名结构：

```bash
codesign --verify --deep --strict --verbose=2 \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CodexTurnChime.app
```

读取实际版本：

```bash
/usr/libexec/PlistBuddy \
  -c 'Print :CFBundleShortVersionString' \
  src-tauri/target/aarch64-apple-darwin/release/bundle/macos/CodexTurnChime.app/Contents/Info.plist
```

生成本机校验值：

```bash
shasum -a 256 \
  src-tauri/target/aarch64-apple-darwin/release/bundle/dmg/CodexTurnChime_<version>_aarch64.dmg
```

macOS beta 使用 ad-hoc signing，不等于 Apple Developer ID 签名，也没有 notarization。

开发版固定使用 `io.github.jgarrick1992.codexturnchime.dev` 和独立数据目录，不会占用生产版 single-instance socket。smoke test 如果检测到真正的生产实例正在运行会明确失败；先从托盘退出生产版后重试。

## 6. 本机打包 Windows x64

必须在 Windows 11 x64 或 Windows GitHub runner 上运行。macOS 不能直接生成受支持的 Windows Tauri 安装包。

准备：

- Node.js 22
- stable Rust MSVC toolchain
- `x86_64-pc-windows-msvc` target
- Visual Studio C++ Build Tools
- Tauri 2 Windows prerequisites

在项目根目录打开 PowerShell：

```powershell
powershell -ExecutionPolicy Bypass -File scripts/build_windows_x64.ps1
```

脚本执行以下工作：

1. 验证 Windows、Node.js、npm、Rust 和 rustup。
2. 安装 Rust Windows target。
3. 使用 `npm ci` 安装锁定依赖。
4. 运行 `npm run build` 生成 `dist/`。
5. 创建 `.exe` sidecar 占位文件。
6. 编译并暂存真实 release Hook helper。
7. 构建 NSIS 安装包。

输出位置：

```text
src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\CodexTurnChime_<version>_x64-setup.exe
```

验证 SHA-256：

```powershell
Get-FileHash -Algorithm SHA256 `
  src-tauri\target\x86_64-pc-windows-msvc\release\bundle\nsis\CodexTurnChime_<version>_x64-setup.exe
```

Windows beta 没有 Authenticode 证书，SmartScreen 可能显示未知发布者提醒。

## 7. 提交、PR 与 main 合并

仓库规则禁止直接推送 `main`，必须通过 PR，并等待必需检查。

创建发布分支：

```bash
git fetch --prune origin
git switch main
git pull --ff-only origin main
git switch -c codex/release-<version>
```

提交前复核：

```bash
git diff --check
git status --short
git diff --stat
```

提交并推送：

```bash
git add -A
git commit -m "release: prepare v<version>"
git push -u origin codex/release-<version>
```

在 GitHub 创建指向 `main` 的 PR。PR 至少应说明：

- 本次功能和修复摘要
- 版本号
- 本机执行过的检查
- macOS 真机打包结果
- tag 合并后才创建

合并前确认这些检查全部为 `success`：

```text
Frontend
Rust (macOS ARM64 target)
Rust (Windows x64)
Rust dependency policy
CodeQL
```

PR 合并后同步本地：

```bash
git switch main
git pull --ff-only origin main
```

确认本地和远端没有分叉：

```bash
git rev-list --left-right --count main...origin/main
```

预期输出：

```text
0	0
```

## 8. 创建和推送 tag

确认版本还没有同名 tag：

```bash
git tag --list 'v<version>'
```

从最新 `main` 创建 annotated tag：

```bash
git tag -a v<version> -m "CodexTurnChime v<version>"
git rev-parse HEAD
git rev-list -n 1 v<version>
```

上面两个 SHA 必须一致，然后推送：

```bash
git push origin v<version>
```

推送 `v*` tag 后，GitHub 自动运行 `.github/workflows/release.yml`。

## 9. GitHub Release workflow

两个平台并行执行：

1. Checkout tag。
2. 安装 Node.js 22 和 stable Rust target。
3. 恢复 Rust cache。
4. 运行 `npm ci`、lint、typecheck、test、frontend build 和 icons。
5. 创建目标平台 sidecar 占位文件。
6. 编译 release Hook helper。
7. 暂存真实 target-triple sidecar。
8. Tauri 构建 DMG 或 NSIS。
9. 附加安装包到 Draft Prerelease。
10. 生成 SPDX SBOM。
11. 生成 SHA-256 文件。
12. 创建 GitHub build provenance。
13. 上传 SHA-256 和 SBOM。

两个平台都成功后，`Publish prerelease` job 使用已认证的 Release 列表查找相同 tag 的 draft，然后设置：

```text
draft: false
prerelease: true
```

不要使用 get-release-by-tag 去查找 draft；该接口可能对 draft 返回 `404 Not Found`。

## 10. 发布后验证

打开：

```text
https://github.com/jgarrick1992/codex-turn-chime/releases/tag/v<version>
```

确认：

- Release 是公开的 prerelease，不是 draft。
- DMG、NSIS EXE、两个 SBOM、两个 SHA-256 文件都存在。
- macOS 与 Windows provenance 可验证。
- Release 标题、版本号和说明正确。
- 安装包文件名包含正确版本和架构。

下载后验证 macOS：

```bash
shasum -a 256 CodexTurnChime_<version>_aarch64.dmg
cat SHA256SUMS-macos-arm64.txt
```

下载后验证 Windows：

```powershell
Get-FileHash -Algorithm SHA256 CodexTurnChime_<version>_x64-setup.exe
Get-Content SHA256SUMS-windows-x64.txt
```

计算值必须和发布的校验文件完全一致。

## 11. 常见失败与处理

### `cargo metadata`: No such file or directory

原因：`cargo` 不在当前 shell 的 `PATH`。

macOS 使用：

```bash
bash scripts/init_env.sh
```

如果使用 fish，确认当前终端已经加载 rustup 或 Homebrew 的 Rust 路径。

### Vite 端口 1420 已被占用

查找旧进程并先正常结束它，再运行 debug：

```bash
lsof -nP -iTCP:1420 -sTCP:LISTEN
bash scripts/debug.sh
```

### `resource path binaries/codex-turn-chime-hook-<target> doesn't exist`

原因：Tauri build script 会在 Hook helper 编译完成前校验 external binary 路径。

正确顺序是：

1. 创建目标 triple 的零字节占位文件。
2. 编译 Hook helper。
3. 运行 `scripts/stage-sidecar.mjs` 用真实文件替换占位文件。

本项目的 macOS、Windows 和 GitHub Release 脚本已经包含这个顺序。

### `frontendDist` 设置为 `../dist`，但路径不存在

原因：编译 Rust/Tauri package 前没有先构建前端。

处理：

```bash
npm run build
```

然后再编译 Hook helper 或 Tauri app。Release workflow 必须在 sidecar 编译前执行该步骤。

### MSI 拒绝 `beta.2`

错误特征：

```text
optional pre-release identifier in app version must be numeric-only
```

原因：WiX/MSI 版本规则不支持文字 SemVer prerelease。

处理：beta 版本使用 NSIS `.exe`；不要为了 MSI 伪造另一套应用版本号。稳定纯数字版本再单独启用 MSI。

### DMG 打包失败或 `/Volumes/CodexTurnChime` 已挂载

先在 Finder 正常推出旧镜像，或检查：

```bash
hdiutil info
```

确认没有旧的 CodexTurnChime volume 后再重试。不要在仍有文件访问时强制卸载。

### 两个平台成功，但 Release 仍是 draft

检查 `Publish prerelease` job。workflow 应通过 authenticated `listReleases` 找到相同 `tag_name` 且 `draft == true` 的 Release，然后调用 update release。

## 12. 失败恢复规则

发布失败时先区分是否已经公开：

### 尚未公开，仅存在 draft

- 修复源码或 workflow，通过 PR 合并到 `main`。
- 优先创建新的 prerelease 版本号。
- 如果项目维护策略允许重建尚未公开且无人使用的 tag，必须先确认没有公开 Release，也没有外部消费者，再删除旧 tag 并从修复后的 `main` 重建。
- 已上传的 draft 资产需要检查是否被同名新资产正确替换。

### Release 已经公开

- 不移动、不覆盖已经公开的 tag。
- 不替换不可变安装包来伪装同一版本。
- 修复后发布新的 prerelease 或 patch 版本。
- 在 Changelog 中说明前一个版本的问题。

## 13. 发布检查清单

### 版本与源码

- [ ] 所有版本文件一致
- [ ] `CHANGELOG.md` 已更新
- [ ] 没有兼容旧 key 或别名映射
- [ ] 工作区没有意外生成文件

### 质量

- [ ] lint 通过
- [ ] typecheck 通过
- [ ] 前端测试通过
- [ ] frontend production build 通过
- [ ] cargo fmt 通过
- [ ] cargo clippy 通过
- [ ] cargo test 通过
- [ ] cargo audit / cargo deny 通过
- [ ] CodeQL 通过

### 本机 macOS

- [ ] `.app` 已生成
- [ ] `.dmg` 已生成
- [ ] app 内版本正确
- [ ] `codesign --verify` 通过
- [ ] DMG SHA-256 已记录

### Git 与 GitHub

- [ ] release PR 已合并
- [ ] 本地 `main` 与 `origin/main` 一致
- [ ] annotated tag 指向预期发布提交
- [ ] tag 已推送
- [ ] macOS Release job 成功
- [ ] Windows Release job 成功
- [ ] Publish prerelease job 成功

### Release 资产

- [ ] macOS DMG
- [ ] Windows NSIS EXE
- [ ] macOS SPDX SBOM
- [ ] Windows SPDX SBOM
- [ ] macOS SHA-256
- [ ] Windows SHA-256
- [ ] build provenance
- [ ] Release 已公开为 prerelease

## 14. `v0.1.0-beta.2` 参考结果

本次发布结果：

```text
macOS DMG: CodexTurnChime_0.1.0-beta.2_aarch64.dmg
Windows:   CodexTurnChime_0.1.0-beta.2_x64-setup.exe
Release:   https://github.com/jgarrick1992/codex-turn-chime/releases/tag/v0.1.0-beta.2
```

Windows 安装包 SHA-256：

```text
98d01878d2ba3892148ccfba56dba06caadc4ea0dc0bd526c2bfca7e53049693
```

正式执行发布时，以对应 Release 页面附带的 SHA-256 文件为最终依据。
