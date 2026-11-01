# R04 IsOS 技能选择用例

[English](README.md) · [所有者契约](../../../docs_cn/isos/interfaces/skill-selection_cn.md) · [设计](../../../docs_cn/isos/design/skill-selection_cn.md)

`cases.json` 固定 IsOS 设计边界，不表示实现已经通过。基线是保存的 `0.1.0` 图、一个 role 为 EOS 的 `@v0.1` component、明确的 schema-2 V2 generation、已验证且 `kind:"skill"` 的 `example/native-skill@0.1.0` P01 发布、一个普通文件 `lib/island/skills/example.skill.so`，以及声明 `host-skill-abi-v2` 的 EOS stub。`selection.example.json` 展示生成结构，其中摘要和路径是合成占位符；测试以真实摘要和私有路径替代。解析固定用例时不运行 guest 代码。

实现应把每个 ID 对应到单元或跨所有者 stub 集成测试，把命令、IsOS/EOS 精确提交和完整输出保存于忽略的 `logs/test/<run-id>/`。成功必须证明生成文件被 manifest 列出、P04 generation 可重新计算、P01 重新验证、环境变量只到 EOS，并且 EOS 验证和暂存选中的字节之后才报告目录就绪。每个停止前失败均保留旧 PID、active revision、run ID 与 R03 grant。随后 EOS worker 拒绝属于候选运行，不能误报为 IsOS 停止前失败。P01/P04/P05 代码优先。EOS 测试另行覆盖 ABI 大小协商、无效描述符、能力拒绝和 worker 回收。

`catalog-setting.example.json` 固定 IsOS 拥有的 `@v0.1` setting metadata，依赖 EOS 的通用 object-array 支持。运行 `python3 tests/fixtures/skill_selection/check_cases.py` 校验用例 ID、两个示例结构和中英文链接。
