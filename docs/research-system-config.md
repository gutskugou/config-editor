# Linux 配置编辑工具 —— 研究与设计文档

> [!NOTE]
> 这是项目早期围绕 systemd 与系统级配置管理开展的研究归档，不代表当前 v0.1.0 MVP 的功能范围。当前实现边界请参阅 [TUI MVP 设计](tui-mvp-design.md)。

> 版本:v0.1(2026-08-09)
> 定位:研究 + 方案设计(不涉及产品代码实现)
> 验证格式:systemd unit(含 drop-in 分层机制)

---

## 0. 摘要

Linux 的系统配置(`/etc`)与用户配置(`~/.config`)由成千上万种互不兼容的文本格式组成,每个应用一套语法,没有统一 schema,没有统一的编辑、校验与生效机制。Windows 用注册表强制统一被证明是失败的,Linux 也先后出现过 Augeas、Elektra 等"统一配置"尝试,均未普及。

本设计文档研究该问题的根源,盘点现有工具,提出一个"**以安全为第一性、以可理解为第二性**"的配置编辑工具的四层架构(双向变换解析层 / 校验层 / 安全写入层 / 文档发现层),并以 **systemd unit 格式** 为 PoC 验证对象,在本机(Ubuntu 24.04,systemd 255)做了实证采样,验证核心架构假设成立。

**核心结论**:
1. 该工具的难点不在编辑器本身,而在"任意格式的解析与无损写回"(双向变换)。
2. 单靠工具无法解决生态问题——工具的长期价值取决于格式插件(社区)数量,因此冷门格式必须优雅回退到"纯文本 + 语法高亮"。
3. systemd unit 是最佳 PoC 格式:一个格式同时覆盖分层合并、校验、生效、跨格式引用四大痛点,且本机即有大量实证样本。

---

## 1. 问题定义

### 1.1 /etc 是什么

`/etc` 是 Filesystem Hierarchy Standard(FHS)定义的**本机系统级配置目录**。名字源于 "etcetera"(早期 UNIX 把不属于任何其它目录的东西都放这里)。FHS 对 `/etc` 的约束只有两条:

- 存放**本机专属的静态配置**;
- 不得存放二进制文件(部分发行版允许少量辅助脚本)。

关键点:**FHS 只规定"配置放哪里",完全不规定"怎么写"**。这为语法碎片化埋下了制度性伏笔。

相关子目录:`/etc/opt`(第三方包配置)、`/etc/xdg`(XDG 系统级配置)、`/etc/default`(Debian 系环境变量式配置,如 `/etc/default/cron`)。

### 1.2 ~/.config 是什么

`~/.config` 是 **XDG Base Directory Specification** 定义的用户级配置目录(`$XDG_CONFIG_HOME`,未设置时默认 `~/.config`)。该规范同时定义了:

| 变量 | 默认值 | 用途 |
|---|---|---|
| `$XDG_CONFIG_HOME` | `~/.config` | 用户级配置 |
| `$XDG_CONFIG_DIRS` | `/etc/xdg` | 系统级配置(搜索路径,冒号分隔) |
| `$XDG_DATA_HOME` | `~/.local/share` | 用户级数据 |
| `$XDG_STATE_HOME` | `~/.local/state` | 用户级状态(历史、最近使用等) |
| `$XDG_CACHE_HOME` | `~/.cache` | 用户级缓存 |

关键机制:**分层查找与覆盖**。应用按 `$XDG_CONFIG_HOME` → `$XDG_CONFIG_DIRS` 的顺序查找同名配置,前者覆盖后者。也就是说,"一个配置项的真实生效值"可能是多个文件合并的结果——**用户在 `~/.config` 里看到的文件不一定等于应用实际使用的值**。

### 1.3 为什么一个应用一种语法

1. **没有中央权威**:Linux 没有微软那样的平台所有者。FHS/XDG 只解决"放哪",没有任何机构能强制统一"怎么写"。对比 Windows 注册表(强制统一但公认为失败)与 macOS plist(有平台强制但只覆盖自家生态)。
2. **历史包袱**:UNIX 自 1960-70 年代起每个程序自带解析器。最早的 rc 文件、行式 `key=value` 文件没有任何正式规范,靠"约定俗成"存活至今。
3. **技术栈决定**:每个应用使用自己语言生态的解析库——glib 的 GKeyFile、Python 的 configparser、Rust 的 serde + TOML、C 的 flex/bison 手写解析器…… 换库成本高,无人重写。
4. **后期标准化迟到且不彻底**:JSON(2001,无注释)、YAML(2001,缩进敏感)、TOML(2013)都是新格式,老应用不可能迁移;INI 至今无正式规范、无类型系统。

### 1.4 为什么难以编辑(痛点清单)

| # | 痛点 | 说明 |
|---|---|---|
| P1 | 权限与安全 | `/etc` 文件 root 所有;普通编辑器直写会破坏属主/权限位/ACL/SELinux context;写坏系统配置 = 系统起不来 |
| P2 | 语法碎片化 | 几十种方言:INI、JSON、YAML、TOML、nginx 块、apache 指令、systemd unit、shell 风格、RC…… 每种都是"半格式" |
| P3 | 无 schema 无预检 | 没有类型、没有选项枚举、没有必填项定义;写错只能"改完重启试试" |
| P4 | 注释即文档 | 大量使用说明写在注释里(如 postfix 默认配置数百行注释);程序无视注释,普通编辑器也不理解注释,损坏注释 = 丢失文档 |
| P5 | 生效机制混乱 | 启动时读 / SIGHUP 重读 / `systemctl reload` / `systemctl restart` / 热更新,各应用不一 |
| P6 | 分散与分层 | 一个应用的配置散在 `/usr/lib/…`(发行版默认)、`/etc/…`(系统)、`~/.config`(用户)、`*.d/` drop-in 目录;真实生效值是多层合并 |
| P7 | 跨格式引用 | 一个配置文件会引用另一种语法的文件(如 systemd unit 的 `EnvironmentFile=/etc/default/cron`);工具链被切断 |
| P8 | 包管理冲突 | 升级产生 `.pacnew`/`.rpmnew`,需与用户修改人工合并;`/etc` 无版本管理(etckeeper 是民间补丁) |
| P9 | 文档获取难 | 选项说明在 man page、发行版默认文件、上游文档三处,难以索引 |

---

## 2. 现状工具盘点

### 2.1 能力矩阵

| 工具 | 覆盖格式 | 安全写入 | 校验 | 分层合并 | 文档内联 | 生态现状 |
|---|---|---|---|---|---|---|
| `visudo` / `vipw` / `vigr` | sudoers / passwd / group | ✅ 锁+语法检查 | ✅ | ❌ | ❌ | 存活,但只覆盖极少数格式 |
| `sudoedit` | 任意文本 | ✅ 提权+保留属主 | ❌ | ❌ | ❌ | 通用底座,非编辑器 |
| `systemctl edit` | systemd unit | ✅ | ✅(partial) | ✅(自动建 drop-in) | ❌ | 存活,只覆盖 systemd |
| `dconf-editor` / `gsettings` | dconf(GNOME) | ✅ | ✅ | ✅(叠加显示) | ⚠️ 部分 | 存活,只覆盖 GNOME 生态 |
| YaST | openSUSE 全栈 | ✅ | ✅ | ⚠️ | ✅ | 存活,绑定 SUSE |
| `etckeeper` | git 化 /etc | ⚠️ 提交时 | ❌ | ❌ | ❌ | 存活,只管版本 |
| **Augeas** | 70+ lens | ✅(原子写) | ⚠️ 部分 | ❌ | ❌ | **维护近停滞**(最后发布 1.14.1,2023-07) |
| **Elektra** | 插件式任意格式 | ✅ | ✅(validation 插件) | ✅(挂载) | ⚠️ SpecElektra | **活跃但应用集成少** |

### 2.2 Augeas:双向变换的先驱(为什么没普及)

Augeas(1.14.1,2023)是"配置编辑库"而非编辑器。核心思想:

- **lens**(镜片):为每种格式写一个双向变换程序(parser + printer 一体),把文件映射成树,用 XPath 访问;
- 修改树后再由 lens 打印回文件,**保留注释、顺序、格式**(前提是 lens 写得好);
- 被 Puppet、SaltStack、Certbot 作为底层库使用。

失败原因分析:
1. **它是库,不是工具**——终端用户和普通运维没有直接可用的产品,只有 API;
2. **lens 编写门槛极高**(专用的 lens 语言,双向约束),格式覆盖难以规模化;
3. 维护停滞,发行版默认配置变更后 lens 失效,无人修。

### 2.3 Elektra:全局键数据库(为什么没普及)

Elektra 把配置做成"全局分层键值数据库",类似"配置的虚拟文件系统":

- 应用通过统一 API 读配置,管理员通过 `kdb` CLI、qt-gui、web-ui 管理;
- 支持把 `/etc/hosts`、`/etc/fstab` 等**挂载**进数据库;
- 插件体系:storage(格式)、validation(校验)、notification(变更通知 D-Bus/journald)、resolver(定位);
- SpecElektra:在键数据库里写配置规范(类型/默认值/文档)。

失败原因分析:
1. **需要应用改造接入**——应用不改用 Elektra API,工具再好也管不到应用;
2. 事实标准是"应用自己读文件",Elektra 试图改变存储语义,与 UNIX 哲学冲突过大;
3. 生态冷启动问题:先有鸡还是先有蛋。

### 2.4 历史教训总结

> **工具的价值 = 支持的格式数量;格式数量 = 社区持续贡献;而社区贡献的前提是有一个好用、安全、文档齐全的工具。** 所有失败者都死在这个循环里(Augeas 死在 lens 门槛,Elektra 死在应用接入)。

因此本方案的核心策略:
1. **先做产品(编辑器),不做库**——用户要的是一个能直接用的工具;
2. **标准格式先行**,冷门格式优雅回退纯文本;
3. 架构上为社区贡献插件预留极低门槛(每格式一个目录,天然隔离)。

---

## 3. 目标与非目标

### 3.1 目标

- 提供一个**交互式编辑器**(TUI 优先,理由见 4.6),让用户能:
  - 以**结构化视图**浏览任意配置(树/表格);
  - 理解**真实生效值**(分层合并后);
  - 安全修改(**保留注释/顺序/格式,保存前校验,出错可回滚**);
  - 一键**应用生效**(识别 reload/restart 方式);
  - 就地获取**选项文档**;
- 架构可扩展:新格式 = 新增一个插件目录。

### 3.2 非目标(明确排除)

- ❌ 不做配置管理系统(NixOS / Ansible / Chef 类——声明式重放,语义不同);
- ❌ 不做同步/备份工具(chezmoi/etckeeper 生态已成熟,可集成不重造);
- ❌ 不改变应用的读取方式(不做 Elektra 式"应用接入改造");
- ❌ 不追求 100% 格式覆盖(MVP 只覆盖标准格式 + systemd unit);
- ❌ 不做 GUI 优先(MVP 为 TUI)。

---

## 4. 核心架构设计

### 4.1 总体分层

```
┌─────────────────────────────────────────────┐
│  UI 层(交互):TUI / 未来 GUI / CLI 批处理      │
├─────────────────────────────────────────────┤
│  应用层:分层合并视图 / 生效识别 / 变更记录     │
├─────────────────────────────────────────────┤
│  文档发现层:man 索引 / 默认配置抽取 / 内联提示 │
├─────────────────────────────────────────────┤
│  校验层:类型检查 / 应用自带验证命令适配器      │
├─────────────────────────────────────────────┤
│  安全写入层:polkit 提权 / 原子写 / 备份回滚   │
├─────────────────────────────────────────────┤
│  解析层:双向变换(bidirectional transform)    │
├─────────────────────────────────────────────┤
│  格式插件:systemd-unit / ini / json / yaml…  │
└─────────────────────────────────────────────┘
```

核心设计原则:**解析层与安全写入层分离**。解析只负责"文本 ↔ 树"的互转,写入负责"如何落盘"。前者保证不破坏,后者保证写不错。

### 4.2 解析层:双向变换(本设计的灵魂)

**为什么必须双向**:朴素做法是"解析成对象 → 改 → 序列化回去",会丢失注释、顺序、多余空白、方言细节,等于销毁了用户的文档(痛点 P4)。

**双向变换的定义**:一个格式插件由一对函数组成:

```
parse  (bytes)      → Document(树 + 元数据)
render (Document)   → bytes
```

约束:对任何合法输入 x,`render(parse(x))` 必须产生**语义等价且结构可还原**的输出(不要求逐字节相同,要求保留注释与顺序)。

**Document 模型**(每个格式插件自行定义,但遵循统一接口):

```rust
trait Document {
    fn keys(&self) -> Vec<KeyPath>;                    // 遍历所有键
    fn get(&self, path: &KeyPath) -> Option<Value>;
    fn set(&self, path: &KeyPath, v: Value) -> Result<()>;  // 失败=破坏结构风险
    fn render(&self) -> Vec<u8>;
}
```

**与 Augeas lens 的差异(吸取教训)**:
- 不发明新语言。格式插件用宿主语言(如 Rust)写,可测试、可调试、门槛低;
- 插件之间的契约只有一个 trait,不要求统一树模型——INI 用 section 树,systemd unit 用 section+键值树,JSON 用任意值树;
- 解析失败 ≠ 拒绝编辑:回退为"只读结构化 + 全文本编辑",并提示用户。

### 4.3 校验层

保存前的三层校验:

1. **格式校验**:`parse` 必须成功(写回后重新 parse 一致);
2. **类型/约束校验**(有 schema 时):数值范围、枚举、路径存在性、布尔语义。systemd unit 可静态提取已知选项的类型表(见 5.5);
3. **应用自带验证命令适配器**(最可靠,优先):
   - `systemd-analyze verify <unit>`
   - `nginx -t`、`sshd -t`、`apachectl -t`、`chronyd -Q` …
   - 适配器配置:每格式注册一个 `verify` 命令模板 + 解析其错误输出回显给用户。

**保存流程**(事务化):

```
校验通过? ──否──→ 回显错误,不落盘
    │是
原子写(临时文件 → rename)
备份(.bak 或版本目录,策略见 4.4)
写后复验(重新 parse + 应用 verify 命令)
提示生效方式(systemctl reload/restart …)
```

### 4.4 安全写入层

- **提权**:对 `/etc` 路径经 polkit(PKEXEC 或 D-Bus)提权,不经 shell;对 `~/.config` 直接写。目标:用户永远不该在编辑器里跑 root shell;
- **保留元数据**:写临时文件后 `rename()` 前,复制原文件的 owner/mode/xattrs(ACL、SELinux context)。`rename()` 保证原子性(进程不会读到半个文件);
- **备份策略**:写前自动生成时间戳备份(如 `file.bak-20260809-200800`),保留最近 N 份;提示用户"本次改动可通过备份回滚";
- **与 etckeeper 集成**(可选):提交前触发 `etckeeper commit`,实现 `/etc` 的 git 历史。

### 4.5 文档发现层

痛点 P9 的解法(优先级从低到高):

1. **内嵌 schema 文档**:格式插件里维护"已知选项 → 描述/类型/单位/示例"表(先覆盖热门格式的高频选项,数据可从 man page 人工整理);
2. **man 页索引**:按文件名关联 man page(`cron.service` → `man systemd.service`),就地打开;
3. **默认配置对比**:显示发行版默认值(/usr/lib/… 层)与当前值,标注差异;
4. 远期:包管理器元数据(/var/lib/dpkg/info/*.conffiles、conffile 校验和)对接,显示"该文件被包管理,升级可能覆盖"。

### 4.6 UI 形态:为什么先做 TUI

- **目标用户是运维与开发者,常在无 GUI 的服务器/容器/SSH 会话中工作**(痛点 P6 的典型场景);
- TUI 技术栈轻、依赖少(一个二进制),便于分发到任何发行版;
- 参考形态:`systemctl edit` 触发 $EDITOR 的替代品 + `htop` 式界面。MVP 甚至可以只做"**结构化 diff 式编辑**":渲染合并后的键值表,选中即进入 $EDITOR 编辑该键的文本值。

---

## 5. systemd unit PoC 思路验证(本机实证)

本节验证"以 systemd unit 作为首个格式插件"的架构假设是否成立。所有实证均在开发机本机(Ubuntu 24.04,`systemd 255 (255.4-1ubuntu8.16)`,170 个 .service 单元)只读采样完成。

### 5.1 实证 1:分层合并(痛点 P6)—— ✅ 假设成立,且比预想更复杂

`systemd-analyze unit-paths` 输出系统查找顺序(节选):

```
/etc/systemd/system.control
/run/systemd/system.control
/run/systemd/transient
/run/systemd/generator.early
/etc/systemd/system          ← 管理员层(最优先)
/run/systemd/system
/run/systemd/generator
/usr/local/lib/systemd/system
/usr/lib/systemd/system       ← 发行版默认层
```

共 **12 层**,优先级:`.control`(dbus 控制) > transient(运行时) > generator > **/etc** > /run > /usr/local > **/usr/lib**。

**对编辑器的影响**:用户看到的"配置"需要表达为三层视图:
1. 默认层(只读,/usr/lib);
2. 管理员层(/etc,可写);
3. **合并生效层**(用户真正想要的值)。

drop-in 机制使合并规则再复杂一层:同一 unit 的 `*.d/*.conf` 文件按字典序读取,**后读的覆盖先读的**,所有 drop-in 又覆盖主文件。因此编辑器必须:
- 显示"最终生效值" + "该值来自哪个文件哪一行"(溯源);
- 修改时给出两种落点选择:写主文件还是写 drop-in(推荐 drop-in,见 5.2)。

### 5.2 实证 2:drop-in 是现实机制,连发行版自己都在用 —— ✅

本机 `/usr/lib/systemd/system/sshd-keygen@.service.d/disable-sshd-keygen-if-cloud-init-active.conf`:

```
# In some cloud-init enabled images the sshd-keygen template service may race
# with cloud-init during boot causing issues with host key generation.  This
# drop-in config adds a condition to sshd-keygen@.service if it exists and
# prevents the sshd-keygen units from running *if* cloud-init is going to run.
#
[Unit]
ConditionPathExists=!/run/systemd/generator.early/multi-user.target.wants/cloud-init.target
```

三点发现:
1. **发行版自己就用 drop-in 修 unit**——drop-in 不是冷门技巧,是主流机制;
2. 这个 drop-in **含 3 行注释**——"注释保留"不是理论问题,是本机现实。任何重写式编辑器都会毁掉这类说明;
3. drop-in 允许添加主文件**没有的选项**(此处 `ConditionPathExists` 在主文件里不存在)——编辑器不能按"主文件 schema"校验 drop-in,必须按"unit 类型 schema"(template 单元 `sshd-keygen@.service` 的通用选项)。

**推荐默认行为**:编辑已有 unit 时,默认生成 `/etc/systemd/system/<unit>.d/override.conf`(即 `systemctl edit` 的落点),而不是改写 `/usr/lib` 下的发行版文件——**这同时解决了包升级冲突(痛点 P8)**,因为 /etc 层覆盖 /usr/lib 层。

### 5.3 实证 3:注释现状两极分化(痛点 P4)—— ⚠️ 需分级处理

| unit 文件 | 注释行数 | 来源 |
|---|---|---|
| `cron.service` | 0 | Ubuntu 精简 |
| `networkd-dispatcher.service` | 1 | Ubuntu 精简 |
| `getty@.service` | 23 | Debian 维护模板 |

结论:发行版打包的 unit 大多精简(0-2 行注释),少数 Debian 维护的文件有大量注释(23 行)。**对 systemd unit 而言,注释保留的收益中等**(不像 postfix main.cf 那样注释即文档),但 drop-in 文件(5.2)必须保留。这验证了:systemd unit 适合验证"分层+校验+生效"三大痛点,而"注释保留"的完整价值需要在 INI/专有格式上验证——可放入迭代路线图的第二格式。

### 5.4 实证 4:校验(痛点 P3)—— ✅ 开箱即用

```
$ systemd-analyze verify /usr/lib/systemd/system/cron.service
(无输出,exit 0)
```

`systemd-analyze verify` 本机可用,支持一次验证多文件、对损坏文件输出具体行号错误。**systemd unit 是少数自带官方验证器的格式**——作为首个插件,校验层无需自研语法检查,直接适配该命令即可验证整条"校验管线"的工程可行性(错误捕获、回显、阻止保存)。

### 5.5 实证 5:跨格式引用(痛点 P7)—— ✅ 最意外的重要发现

`/usr/lib/systemd/system/cron.service` 内容:

```
[Unit]
Description=Regular background program processing daemon
Documentation=man:cron(8)
After=remote-fs.target nss-user-lookup.target

[Service]
EnvironmentFile=-/etc/default/cron
ExecStart=/usr/sbin/cron -f -P $EXTRA_OPTS
...
```

其中 `EnvironmentFile=-/etc/default/cron` 引用了一个**完全不同的语法**(Debian 系 shell 风格 `KEY=VALUE`)。而本机 `/etc/default/cron` 的实际内容:

```
# This file has been deprecated. Please add custom options for cron using
# $ systemctl edit cron.service
# or
# $ systemctl edit --full cron.service
```

两点发现:
1. **跨格式引用是现实**:unit → 环境文件 → shell 语法,编辑链被切断;
2. **Ubuntu 官方自己都在把用户从 `EnvironmentFile` 往 drop-in 迁移**——这印证了"drop-in 是系统设计上更优雅的配置覆盖方式"这一判断,也意味着编辑器需要**感知这种迁移路径**(提示用户:与其编辑 /etc/default/cron,不如生成 drop-in)。

另外可静态提取 unit 选项的类型信息(如 `Restart=` 枚举 `no/on-success/on-failure/…`,`KillMode=` 枚举 `control-group/process/mixed/none`),验证"类型感知校验"可行。

### 5.6 PoC 结论

| 架构假设 | 实证结果 | 影响 |
|---|---|---|
| 分层合并是核心痛点 | ✅ 12 层查找 + drop-in 字典序合并,现实存在 | 必须做"合并生效层 + 溯源"视图 |
| 注释保留必要 | ⚠️ unit 注释少但 drop-in 注释必须保 | 双向变换是硬需求,不是可选优化 |
| 校验管线可复用官方验证器 | ✅ `systemd-analyze verify` 可用 | 校验层工程风险最低 |
| 修改默认落点应为 drop-in | ✅ 发行版自身实践 + Ubuntu 官方指引 | 默认写 drop-in,顺带解决包冲突 |
| 跨格式引用存在 | ✅ `EnvironmentFile=/etc/default/cron` | 需要"引用感知 + 迁移提示" |

**判定:systemd unit 作为首个格式插件成立,且能一次性验证分层、校验、生效、迁移四大机制。建议 MVP 阶段即按此实现,第二格式选 INI(验证注释保留与弱 schema 下的编辑)。**

---

## 6. 迭代路线图

### Phase 0 — PoC(systemd unit 专精)
- 只读:unit 树解析 + drop-in 合并 + 生效值溯源 + `systemd-analyze verify` 适配;
- 可写:生成 `/etc/systemd/system/<unit>.d/override.conf`(经 polkit),备份 + 原子写;
- 交付:CLI(`ce view cron.service` / `ce edit cron.service`)+ 简单 TUI;
- 成功标准:本机 170 个 unit 全部可解析,修改后可一键 verify + reload。

### Phase 1 — 格式插件化
- 定义 `FormatPlugin` trait,先落地 `ini` 插件(重点验证注释保留);
- 冷门格式回退:纯文本编辑 + 只读结构化提示;
- 文档发现:首版选项表(unit 高频选项 + INI 常见键)。

### Phase 2 — TUI 产品化
- 三栏式界面:文件树 / 键值表(生效值+溯源) / 文档与校验面板;
- 编辑事务流:预览 diff → 校验 → 落盘 → 生效提示;
- 与 `$EDITOR` 互操作:复杂值编辑仍唤起 vim。

### Phase 3 — 生态
- 插件仓库 + 提交模板(降低贡献门槛,吸取 Augeas lens 教训);
- 集成 etckeeper、包管理器 conffile 状态展示;
- 可选 GUI 前端复用同一内核。

---

## 7. 参考资料

- Filesystem Hierarchy Standard 3.0(freedesktop.org,2025-11 接管):`/etc` 定义与约束
- XDG Base Directory Specification v0.8(2021-05):`$XDG_CONFIG_HOME` / `$XDG_CONFIG_DIRS` / 分层覆盖规则
- Augeas(软件)概述:lens 双向变换、XPath 访问、Puppet/Salt/Certbot 使用者、最后发布 1.14.1(2023-07)
- Elektra Initiative README:全局键数据库、挂载、插件体系、kdb/qt-gui/web-ui
- systemd 255 本机实证:`systemd-analyze unit-paths`、`verify`、`/usr/lib/systemd/system/` 采样(§5)
- Windows 注册表 / macOS plist 对比(配置存储模式对照,来源:Configuration file - Wikipedia)

## 附录 A:本机实证原始数据(2026-08-09)

```
# 环境
Ubuntu 24.04, systemd 255 (255.4-1ubuntu8.16)
/usr/lib/systemd/system/*.service 共 170 个
systemd-analyze: /usr/bin/systemd-analyze 可用

# unit-paths(前 10 层)
/etc/systemd/system.control
/run/systemd/system.control
/run/systemd/transient
/run/systemd/generator.early
/etc/systemd/system
/etc/systemd/system.attached
/run/systemd/system
/run/systemd/system.attached
/run/systemd/generator
/usr/local/lib/systemd/system

# 发行版自带 drop-in(带注释)
/usr/lib/systemd/system/sshd-keygen@.service.d/disable-sshd-keygen-if-cloud-init-active.conf

# 注释统计
cron.service: 0 行     networkd-dispatcher.service: 1 行     getty@.service: 23 行

# 跨格式引用
cron.service → EnvironmentFile=-/etc/default/cron(内容为弃用说明,指引使用 systemctl edit)

# 校验
systemd-analyze verify /usr/lib/systemd/system/cron.service → exit 0
```
