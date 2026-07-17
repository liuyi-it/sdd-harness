# codebase-memory-mcp 在 Windows 下的安装现状与诊断

> 作者：Claude（Claude Code 生成）
> 生成日期：2026-07-16
> 场景：`sdd init` 报 "降级模式：codebase-memory-mcp 当前不可用" 的排查记录

---

## 一、一句话结论

机器上**同时存在两套** `codebase-memory-mcp`：

1. **独立安装的完整二进制** —— Claude 的 MCP 通道正在用它，工作正常（能索引、能查询）。
2. **npm 全局安装的 wrapper 包** —— 只是个下载壳子，真正的二进制**没下载成功**（`bin/` 目录为空）。

而 `sdd init` **只认 npm 包**、不认独立二进制、也不走 MCP 协议探测，所以它判定"不可用"并降级为受限文件扫描。

**因此：Claude 侧一切正常，只有 `sdd` 侧报警告。二者互不冲突。**

---

## 二、现状明细

### 1. MCP 通道（Claude 侧）—— ✅ 正常

`claude mcp list` 输出：

```
codebase-memory-mcp: C:/Users/viruser.v-desktop/AppData/Local/Programs/codebase-memory-mcp/codebase-memory-mcp.exe  - ✔ Connected
```

- 实际使用的二进制：
  `C:\Users\viruser.v-desktop\AppData\Local\Programs\codebase-memory-mcp\codebase-memory-mcp.exe`
- 文件大小：约 **269 MB**（`269,067,776` 字节），日期 `Jun 12`
- 功能验证：已成功对本项目建立索引
  - 项目名：`D-workSpace-04-crm-b2cweb-crm-cloud`
  - 节点数：**59,939**，边数：**165,172**
  - 状态：`indexed`

> 说明：这是一个独立分发的**单文件静态二进制**，与 npm 无关，Claude 直接以进程方式启动并通过 stdio 通信。

### 2. npm 全局包 —— ⚠️ 装了壳子，二进制缺失

`npm ls -g codebase-memory-mcp`：

```
C:\Users\viruser.v-desktop\AppData\Roaming\npm
`-- codebase-memory-mcp@0.9.0
```

包目录结构（`<npm-global>\node_modules\codebase-memory-mcp`）：

```
README.md
bin/          <-- 空目录（关键问题）
bin.js        <-- CLI shim（wrapper）
install.js    <-- postinstall 下载脚本
package.json
```

`package.json` 关键字段：

```jsonc
{
  "name": "codebase-memory-mcp",
  "version": "0.9.0",
  "description": "... single static binary MCP server",
  "bin": { "codebase-memory-mcp": "./bin.js" },
  "scripts": { "postinstall": "node install.js" },
}
```

**工作原理**：这个 npm 包本身不含二进制。安装时 `postinstall` 会运行 `install.js`，从 GitHub Release 下载对应平台（win32/x64）的静态二进制放到 `bin/codebase-memory-mcp.exe`。`bin.js` 是启动壳，运行时若发现二进制缺失，会再触发一次下载。

### 3. 二进制下载失败 —— ❌ 根因

- `bin/` 目录为空，说明 `postinstall` 阶段的下载**没有成功**。
- 手动执行 `node install.js` 复现：**进程挂起**，90 秒超时被强制终止（退出码 124/143）。
- 直接运行 `codebase-memory-mcp --version` 同样**卡死**（壳子发现二进制缺失 → 再次尝试下载 → 挂起）。

判断：从 GitHub 下载二进制的网络请求被阻断 / 超时（很可能是网络到 GitHub Release 不通或需代理）。

---

## 三、为什么 `sdd init` 说"不可用"

`sdd init` 的输出里有关键一行：

```
[codebase] 正在检查项目本地及 npm 全局安装…
State: INDEX_READY
Next: sdd new
Warning: 降级模式：codebase-memory-mcp 当前不可用，已切换为受限文件扫描
Warning: 安装建议：请先安装并配置 codebase-memory-mcp，官方项目地址：https://github.com/DeusData/codebase-memory-mcp
```

- sdd 的检测方式是**查项目本地 `node_modules` 和 npm 全局**是否有可用的 `codebase-memory-mcp`。
- 它**不识别**独立安装的那个 `.exe`，也**不通过 MCP 协议**去探测已连接的服务。
- npm 全局虽装了包，但因为 `bin/` 里没有真正的二进制、且实际运行会挂起，sdd 判定其不可用 → 降级。

> 注意：这只是**降级提示**，`sdd init` 本身已成功（`State: INDEX_READY`），可以继续 `sdd new`。

---

## 四、修复方案（任选其一）

### 方案 A（推荐，无需联网）：复用已有二进制

把已经能用的独立二进制复制到 npm 全局包的 `bin/` 下，满足 `bin.js` 壳子和 sdd 的检测：

```bash
# 目标目录（npm 全局包）
GLOBAL_PKG="$(npm root -g)/codebase-memory-mcp"

# 源二进制（Claude 正在用的那个）
SRC="/c/Users/viruser.v-desktop/AppData/Local/Programs/codebase-memory-mcp/codebase-memory-mcp.exe"

cp "$SRC" "$GLOBAL_PKG/bin/codebase-memory-mcp.exe"
```

> 前提：两者版本兼容（现有 exe 为 Jun 12 版本，npm wrapper 为 0.9.0）。复制后建议用 `codebase-memory-mcp --version` 验证能正常启动再重跑 `sdd init`。

### 方案 B：修好网络后重新安装（走官方下载）

```bash
# 配置好到 GitHub 的网络/代理后
npm uninstall -g codebase-memory-mcp
npm install -g codebase-memory-mcp
# 或仅重跑下载脚本：
node "$(npm root -g)/codebase-memory-mcp/install.js"
```

### 方案 C：让 sdd 指向已有二进制 / MCP

若 sdd 支持配置外部可执行文件或已连接的 MCP，可直接指向：
`C:\Users\viruser.v-desktop\AppData\Local\Programs\codebase-memory-mcp\codebase-memory-mcp.exe`
（需查阅 sdd 配置项，官方地址：<https://github.com/DeusData/codebase-memory-mcp>）

---

## 五、验证清单

修复后依次确认：

- [ ] `ls "$(npm root -g)/codebase-memory-mcp/bin/"` 能看到 `codebase-memory-mcp.exe`
- [ ] `codebase-memory-mcp --version` 能正常返回（不再挂起）
- [ ] 重跑 `sdd init`，不再出现 "降级模式" 警告
- [ ] `claude mcp list` 中 `codebase-memory-mcp` 仍为 `✔ Connected`

---

## 附：关键路径速查

| 项                         | 路径 / 值                                                                                       |
| -------------------------- | ----------------------------------------------------------------------------------------------- |
| 独立二进制（MCP 实际使用） | `C:\Users\viruser.v-desktop\AppData\Local\Programs\codebase-memory-mcp\codebase-memory-mcp.exe` |
| npm 全局前缀               | `C:\Users\viruser.v-desktop\AppData\Roaming\npm`                                                |
| npm 全局包目录             | `<上者>\node_modules\codebase-memory-mcp`                                                       |
| npm 包版本                 | `0.9.0`                                                                                         |
| 官方项目                   | <https://github.com/DeusData/codebase-memory-mcp>                                               |
