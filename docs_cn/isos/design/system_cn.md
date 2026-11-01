# IsOS 0.x 架构与算法

[English](../../../docs/isos/design/system.md)

**目标设计；整体尚未实现。** IsOS `0.1.0` 管理已保存的部署图、精确版本包、生成配置、本地工作台和直接 EOS／EGIA 组件子进程。EOS `0.2.0` 管理硬件准入、Rust 运行时与 MuJoCo 子进程。EGIA `0.1.0` 管理智能体阶段；IsosUI `0.1.0` 通过 IsOS 编辑草稿并展示状态。模型类服务保持为图外、显式启动的 IsOS 服务。现有命令见[首次使用](../usage/get-started_cn.md)；目标契约见[图](../interfaces/node-graph_cn.md)、[本地 API](../interfaces/local-api_cn.md)、[软件包](../interfaces/releases_cn.md)和[配置／日志](../interfaces/configuration_cn.md)。

```text
注册表／目录 → 已验证发行收据 → 节点类型、集合、资源
                               ↓
用户草稿 → IsosUI／CLI → IsOS 校验 → 修订检查 Apply → 已保存图
                         ↓                   ↓
                    解析选择器         不可变核心配置
                                             ↓
                       IsOS 管理器 → EOS Rust → MuJoCo 子进程／套接字
                                    → EGIA Python 阶段
                       状态／日志游标 → IsosUI 工作面板
```

图按 `core → layer → group → node` 分层；注册类型确定位置、能力、设置、有类型端口、生命周期和说明。实例确定身份、操作员说明、包固定版本和值。包可以贡献节点，但不能因此取得任意命令执行权。IsOS 管理验证与配置；EOS 的**内部适配器注册表**保存已编译的 MuJoCo 等实现。仿真器程序与场景仍是 IsOS 安装的外部精确版本包。EOS 只读应用后的图与生成快照，不安装包或修改图。所有执行器命令包括来自软件包适配器的命令，都经过同一 EOS 调度器。

## 准入与 Apply 算法

1. 读取图原始字节，在规范化之前计算 SHA-256 修订。按图版本选择解析器。目标读取器接受新 `0.1.0` 与已有安全 `1.0.0` 图；拒绝未知未来版本。只在内存补齐可选说明和边的默认值。
2. 请每个启用的核心校验自己的子树与类型 ID。合并内置类型与已验证包侧车；拒绝重复 ID、未注册位置、不兼容的有类型端口和未支持的 codec。已注册旧 `@v1.*` ID 保持原义；绝不执行旧任意 `command` 节点。
3. 对每个精确包固定版本验证收据、发行清单、可执行文件、描述文件、场景及引用资源。把选择器 ID 解析为已验证资源；已选 ID 不可用时按节点／设置路径拒绝 Apply。校验仿真设备 ID、套接字所有权和适配器兼容性。图保存稳定 ID 与版本，不保存资源绝对路径。
4. 比较调用方期望修订与已保存修订。新 `0.1.0` 草稿先序列化规范字节并计算新 SHA-256；未变的旧图保留原始字节与修订。按这些字节生成有界 EOS／EGIA setup 文件，先写私有临时文件，再按**新修订**原子发布快照。原子保存新草稿字节前再次确认旧修订匹配；并发编辑失败遗留的未使用快照可安全删除。返回结果修订；运行中进程保持旧修订，直到显式 Restart。

显式迁移先预览旧图到新图及类型变化，要求原始修订并返回新修订。接受前保留旧图字节。解析、包校验、生成或修订比较失败时，已保存图和运行中进程都不变。

## Run、Stop 与恢复算法

Run 重新读取已保存字节、包收据、选定资源、配置的核心程序和生成快照身份；对照每个核心声明的版本数组校验。IsOS 按依赖顺序启动直接子进程并等待就绪；EOS 启动 MuJoCo 子进程，连接仅所有者访问的 Unix 套接字并构造已声明适配器。只有所需 EOS gateway 能力健康后才启动 EGIA；单独 TCP 监听不足以证明 EGIA 就绪。故障时 IsOS 先停依赖者再停提供者，报告最近错误，写一条状态变化事件。Stop 要求 EOS 先停止已准入执行器，再终止 MuJoCo；IsOS 随后逆依赖顺序在有界 TERM／KILL 宽限内停止进程组。Restart 失败时尽可能恢复旧配置，否则标记部署降级。

链接包可能在磁盘上变化，因此 Apply 与 Run 复查其指纹。运行中进程不热替换程序或场景。已保存或活动部署固定的包不可删除。独立服务有自己的显式生命周期，不会成为隐式图依赖。

## 选择器与连线算法

对每个注册的 `Setting.selector`，根据 `package`、`registry` 或 `graph` 提供者从**草稿**图求选项。包提供者只读取已验证的精确版本。结果按节点路径和设置键分组，包含稳定 ID、标签、说明、可用性与原因。不可用的已选值保持可见，供操作员修复；Apply 重新解析所有选择。IsosUI 的可搜索控件可类似模型权重选择器，但保存稳定 ID，而不是文件路径。

IsOS 校验边的来源、目的类型、`priority` 和 `frequency_hz`。**当前实现：**编辑器校验 API 对比注册端口的精确数据类型，并在 Apply 前返回定位到连线的 `edge_type_mismatch` 诊断；数据类型 schema 载明字段单位。其他校验失败仍使用 EOS 校验器的通用诊断。目标中的实际路由由 EOS 管理：每边单调时钟 token bucket 限流；加权公平服务和有界队列防止高优先级流饿死其他流；溢出计数并限速记录。`expected_rate_hz` 是生产者元数据，不会暗中变成边上限。仅设计连线可在 IsosUI 展示，但不得声称有运行时调度。优先级不可绕过调度器安全准入或执行器停止。

## 实现里程碑

### G01–G02 兼容读取证据

G01–G02 读取切片只准入精确注册的旧类型身份，按 JSON 指针拒绝未知版本和不安全进程节点，并保留保存字节的修订号。G03 现已提供 `0.1.0` 写入器和显式迁移。读取器仍保留原始 JSON 值；类型化归属方校验器应用既定可选默认值，不修改保存字节。

在 IsOS 仓库根目录运行 `cargo fmt --all -- --check`、`cargo test --no-default-features`、`cargo clippy --all-targets --no-default-features -- -D warnings` 和 `cargo build --no-default-features`。2026-11-01 全部通过，共 76 个单元测试及 5 个集成测试；完整本地输出保存在 `logs/test/2026-11-01-g01-g02/`。另外，用干净的临时 `ISOS_HOME` 和指向 EOS release 程序的 `ISOS_AETHER_BIN_DIR` 验证了首次编辑初始图 Apply、结构相同且空白不同的图无写入 Apply 保留 SHA-256，以及已应用旧图改动返回 `migration_required` 且字节不变。精确输入及预期路径见[`tests/fixtures/graph_compat/`](../../../tests/fixtures/graph_compat/README.md)。

依赖顺序、产出及验收门槛统一放在 [IsOS 里程碑计划](../maintenance/roadmap_cn.md)。本页负责上述算法，契约字段仍放在 `interfaces/`，节点行为见[节点目录](../layers/README_cn.md)。本次发布不包含 WorkflowService、ActionList 或 workflow DAG。

### G03 迁移事务

[图契约](../interfaces/node-graph_cn.md#g03-显式图迁移)定义精确类型映射；[本地 API](../interfaces/local-api_cn.md#g03-迁移路由)定义请求与响应字段。HTTP 和 CLI 共享同一迁移引擎。引擎把原始 `SavedGraph` 字节及其 SHA-256 与解析值、建议值分别保存，预览不会改动来源。它只按表替换版本和类型，再确认其他所有已接受的 JSON 值相等。未知字段不能通过类型化反序列化而丢失，必须拒绝。映射后的类型仍须通过精确注册、设置、软件包来源、位置和端口兼容校验；不根据版本后缀猜测。

依赖未就绪时预览仍可计算建议结果，但标记阻塞。提交在图锁内根据当前字节重复全部检查，并要求原修订号和建议摘要。0.1 就绪屏障包括 IsOS 自有 component 注册、EOS setup 和运行时对 EOS 投影的支持、EGIA setup 对图中旧 common 节点的支持，以及整图跨 core 边和引用的准入。在 G06 提供完整准入前，含跨 core 连线的图不能迁移。EOS setup identity 不得声明其运行时无法读取的图版本。IsOS 将按修订保存的仅 EOS 生成投影传给 EOS 运行时，不能让运行时在迁移后重新解析未经完整校验的原始整图。

发布点是已暂存图文件的原子 rename。在这之前，按建议修订号命名的不可变生成文件、receipt 指纹及清单已校验、fsync 并发布；在锁内重查来源修订。准备失败则清理临时文件并保留旧图。未引用的已发布快照无害，可随后回收。rename 后还须 fsync 图父目录；失败时提交结果不确定，需重读保存修订号判断。图 rename 后不再进行 setup 或启动步骤。运行中子进程保留旧修订，只由显式 Restart 切换。rename 后崩溃或丢失响应时，重新 GET 并与预览摘要比较；用旧修订重试不会二次应用。

实现须在预览、旧修订冲突、软件包指纹变化、未知或不安全类型、render 失败、快照或图发布失败时断言保存字节不变。成功用例校验精确夹具映射与 pin/字段保留、保存与运行中修订分离、Run/Restart 预检，以及事务失败后旧图恢复。完整日志存于 `logs/test/<run-id>/`；G03 分支合并前用干净的 `ISOS_HOME` 重放 HTTP 和 CLI 边界。

新写入器以 `data/generated/<deployment>/<saved-revision>/` 作为 `0.1.0` 的不可变查找键；Run 和 Restart 校验其清单，并将经验证的 `eos/graph.json` 代入 EOS runtime 与 config-server 的 `--profile` 启动参数，不修改保存的 `settings.args`。它们不跟随可变的 `current` 指针或旧平面生成目录。旧平面的 `core-setup/launch-inputs.json` 只为 `1.0.0` 兼容保留。Init 和缺失图 GET 生成 `0.1.0` 初始图；修改新版本的 PUT 和 CLI Apply 使用相同的修订比较交换引擎。按归属方分区的校验器使用 IsOS 自有进程定义、EOS setup 投影、EGIA setup 与 IsOS 整图连线校验，不把 IsOS `@v0.1` 节点交给 EOS 整图 profile 校验。

Run 和 Restart 还会将每个已保存的生成文件与根据精确已保存图重新执行的归属方 render 比较。即使文件和清单摘要一起被修改，也会拒绝启动。重新构建核心程序后，快照中的启动输入会过期。`isos init` 报告过期的输入并保留不可变快照；须恢复固定的程序，或创建新的图修订并 Apply。对未修改图字节刷新快照所需的独立生成身份留待 P04/P05。

G03 实现证据（2026-11-01）：使用 `/home/ashex/projects/Island-eos/target/debug` 中的 EOS `fb57a0c5` 调试程序，以及 `logs/test/2026-11-01-g03/` 下的新 `ISOS_HOME` 目录，`cargo fmt --all -- --check`、`cargo clippy --all-targets --no-default-features -- -D warnings` 和 `cargo test --all-targets` 均通过（87 个单元测试及 5 个 CLI 测试）。CLI 重放执行 `isos config init`，预览已保存的 1.0 图（`can_apply:true`），用两个摘要提交，随后 `isos run` 在迁移后的活动修订号下启动 EOS、bridge 和 config；`isos version check --json` 显示 EOS 就绪。重复提交过期来源失败，保存字节不变。HTTP 重放依次调用 `GET /api/isos/v1/node-graph`、POST `/node-graph/migration/preview`、POST `/node-graph/migration/apply` 及重复的过期 POST：首次提交返回 0.1，重复请求返回 409，前后 SHA-256 相同。回归测试覆盖同时篡改投影和清单、跨 core 输入的精确路径、未知 core／旧 UI／类型位置拒绝、PUT 缺失修订号、精确软件包及适配器 pin 保留、Init 不可变行为及已保存 0.1 图的可编辑性。完整命令与输出位于 `logs/test/2026-11-01-g03/`。

### G06 整图连线准入——目标设计

从精确图修订及各归属方精确注册的目录构造一份不可变的准入视图。按规范节点路径顺序遍历节点，按原图顺序遍历每个输入来源；在检查连线之前解析两端描述符。在规范化之前记录对象或数组形状对应的实际 JSON Pointer。检查精确类型、单位模式一致性、传输、基数、必需输入、重复来源及环路，再对照明确的归属方路由表分类。IsOS 检查 EOS 所属投影无法看到的跨 core 桥及引用。进程依赖也在整图预检中检查，但不纳入数据连线路由表。归属方描述符或桥声明缺失时按原始指针拒绝。不可直接转发 EOS 旧 profile 校验器的文字错误，因为它可能缺少指针。[图契约](../interfaces/node-graph_cn.md#g06-整图连线准入目标契约)定义字段和安全规则。

整张图通过后，Validate 才返回有序连线检查列表。Apply 与迁移提交在归属方渲染及任何快照／图字节发布之前执行此准入。Run 和 Restart 在启动子进程之前，基于已保存字节及经验证的快照重新检查，防止变化的软件包描述或被篡改的图复用旧准入结果。旧 `1.0.0` 读取保留既有路径和字节。测试通过 HTTP、CLI、迁移及启动预检重放相同负面样例，断言相同指针／错误码和未变化的已保存／活动修订。G06 只发布声明；EOS R01 和 EGIA A04 后续证明实时交付、有界队列及缺口计数。

2026-11-01，IsOS 实现代理及 EOS 连线契约归属方完成独立设计审查，接受 M1 路由表和固定路径元数据。审查确认相机 frame → gate frame 是 M1 唯一适用证据路由、Vision 桥在注册 `0.1.0` 目标类型前保持关闭，以及 EOS 旧校验器仅返回文字错误时 IsOS 必须生成自身的精确指针诊断。
