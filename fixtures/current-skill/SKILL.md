---
name: novelai-author-storyboard
description: NovelAI 分镜套图"参考模板克隆器"(Reference Template Cloner)。从内置 30 套真实作者成品模板中选择最接近的一套,整卷复制后仅做最小修改(换角色/换场景/用户明确变更)生成 schemaVersion 2 项目 JSON。当用户要求"写 NovelAI 提示词、写套图、写分镜、生成漫画项目、用某角色生成一套、按某题材生成一套"时触发。默认不重写模板;从零生成仅作为无匹配时的 fallback。
---

# NovelAI Reference Template Cloner

## 0. 本 Skill 是什么

以 **30 套原始作者 JSON**(`references/template-library/`,只读)为唯一事实源。用户提出需求后:

```
用户需求
→ 从 30 套真实模板中检索最接近的完整套图
→ 选定 1 套 Primary Template
→ 复制整套 JSON(deep copy)
→ 只修改用户要求变化的内容
→ 保留原模板的大部分 Prompt、顺序、权重、括号、镜头、节奏、参数结构
→ 输出新的完整 JSON
```

**核心原则(永久生效)**:

```
模板优先于生成
复制优先于重写
局部替换优先于重新组织
原作者实际 Prompt 优先于 AI 总结出的规则
```

**如果模板已经能够表达用户要求:禁止重新生成该部分 Prompt。**

产出物(对话中给出):
1. 【Template Match】报告(流程第 3 步,短报告;**默认作为结果报告展示,不阻塞执行**)
2. 修改范围说明(改了什么/没改什么,第 4-7 步)
3. 完整 project JSON(第 8-9 步,写入文件)

---

## 1. 生成流程(严格按序,不得跳步)

| 步骤 | 内容 | 用什么 |
|---|---|---|
| **Step 1** | 解析用户需求(场景/地点/时间/人物数量/人物身份/剧情类型/节奏/特殊要求/指定格数) | — |
| **Step 2** | 检索 `template-index.json`(先归一化地点词,再同场景族过滤,池内打分) | `references/template-index.json` + `template-selection.md` |
| **Step 3** | 确定 Primary Template(Txxx),输出【Template Match】短报告(**默认不阻塞,自动继续 Step 4-9**;仅两种情形停等用户:①用户显式要求"先让我选模板";②Top1/Top2 差异极小(<0.05)且两者修改代价明显不同) | `template-selection.md` §5 |
| **Step 4** | 打开 `template-library/template_XXX.json` **完整原文**(只读) | 原文,不是摘要 |
| **Step 5** | Deep Clone:内存复制整个 JSON(字段/panel 顺序/prompt 原文/cc 结构/paramsOverride/imageSize 调度全保留) | `template-mutation.md` §0 |
| **Step 6** | Character Replacement:只替换身份相关块(角色名/发/眼/固有服装/固有道具)→ `title`、`official style` 锚、CC1 段、角色名引用;**禁止碰**镜头/动作/表情/剧情状态。服装变更必须先跑服装状态链分析(§3.4):提取用 `tools/clothing_chain.py extract`,校验用 `verify` | `template-mutation.md` §3 + `tools/clothing_chain.py` |
| **Step 7** | Scene / User Delta:用户在 Step 1 明确要求的变更(换场景 → Scene Mapping §4;指定道具/群交/全裸 → User Delta §5);无要求则跳过 | `template-mutation.md` §4-5 |
| **Step 8** | Validator:三道 Gate(Anti-Rewrite / Identity Leak / Scene Leak)+ JSON 结构合规(§3);失败则回滚重做 | `template-mutation.md` §7 |
| **Step 9** | 输出新 JSON(新 UUID/seed/title,结构沿模板)写入工作区 `{套名}.json` | §3 |

**运行时经验规则**:
1. 索引(i)只用于"找模板";真正生成时**必须重新打开 `source_file` 原文**。
2. 默认 `ONE REQUEST = ONE PRIMARY TEMPLATE`;跨套拼接仅在用户明确要求"融合两套"时允许,且必须列明融合点。
3. 30 套真实模板 **>** `template-index.json` 统计结论 **>** 本文正文规则。正文规则是历史摘要,可能失真。
4. 用户对模板风格做任何修改,按用户要求执行;用户没有要求的部分,一律继承模板原文。
5. **默认全自动**:Step 3 报告只作为结果展示,不停等;检索→克隆→输出一次完成。例外(停等):用户明确要求先选,或 Top1/Top2 差异 <0.05 且修改代价明显不同。

---

## 2. 硬约束区(与模板冲突时的裁决顺序)

### 2.1 裁决顺序(降级后的规则体系)

```
1. Primary Template 原文(最高优先)
2. template-index.json(实测统计,用于检索与校验)
3. style-guide.md / tag-vocab.md / narrative-templates.md / examples.md
   (从 GENERATOR 降级为 VALIDATOR / FALLBACK)
```

只有两种情况下才允许查旧规则文档:
- **校验**:检查克隆结果是否仍符合作者体系的度量区间(权重档分布/括号密度/男卡率等,见 §4 表格)。
- **补丁**:模板必须新增少量内容时(用户要求加道具/改服装),从 `tag-vocab.md` 取词、从 `style-guide.md` 取写法。

**发生冲突时,真实作者作品 > AI 总结出的作者规律。除非用户明确要求改变。**

### 2.2 模板选择(硬性)

- 场景匹配优先级最高。用户说"公园" → 先筛 park 族模板(T010/T025),**禁止**把完全不相关模板放进随机池。
- 随机不是全库随机:Top-K 默认 3;`Top1 - Top2 >= 0.15` 时直接选 Top1;否则在 Top3 内按分数加权随机。
- 相似度不足(score < 0.55):选最接近模板 + 明确标记需要 Scene Adaptation + 尽量保持 Template Skeleton,不得假装有完美模板、不得从零生成。

### 2.3 禁止跨模板自动"取长补短"

```
✗ "这个镜头我觉得 T005 更好,这个权重 T021 更好,这个结尾 T009 更好" → 自己拼接
```

这会重新退回"AI 创作"。默认禁止。

### 2.4 格数

- 用户没要求格数 → `panel_count = Primary Template panel_count`(模板 92 格输出 92 格;**模板真实格数 > 抽象统计默认值,禁止强制压成 80**)。
- 用户指定格数 → 按阶段比例采样/合并(压缩)或复制相邻格+最小 Delta(扩展),见 `template-mutation.md` §6。**禁止从零重写。**

### 2.5 默认运行模式

| 模式 | 使用时机 |
|---|---|
| Mode A — Exact Clone | 用户需求与某模板高度一致:复制 → 只换人物 |
| Mode B — Scene Clone | 剧情结构接近、地点不同:复制 → 换人 → 全套替换场景 |
| Mode C — Nearest Template | 无完全匹配:保留阶段/镜头/Prompt Grammar,只改冲突部分 |
| **Mode D — From Scratch** | **默认禁止**。仅 30 套完全无可用结构时允许,且必须向用户说明。 |

---

## 3. 输出 JSON(结构沿模板,不新建 Schema)

最终必须继续输出原软件兼容 JSON(与模板同一 schemaVersion 2 结构)。

### 3.1 继承规则

```
复制模板 JSON(deep copy)
→ 生成新的顶层 UUID
→ 生成新的 Panel UUID(每格)
→ 更新 title(套名)
→ 替换角色(§6)
→ 替换必要场景(§7)
→ 应用用户明确修改
→ 更新 seed(每格随机固定且互不重复;或沿用用户指定)
→ 保持 JSON 字段和结构完全不变
```

### 3.2 骨架(严格沿用,与模板一致)

顶层 11 键:

```json
{
  "schemaVersion": 2,
  "id": "<uuid>",
  "title": "<套名>",
  "globalStylePrompt": "",
  "globalNegativePrompt": "<继承模板原文,不修改>",
  "sizeMode": "uniform",
  "initialGenerationCount": 1,
  "globalParams": { "model": "nai-diffusion-4-5-full", "stylePrompt": "", "positivePrompt": "",
    "negativePrompt": "<继承模板原文>", "width": 832, "height": 1216, "steps": 28,
    "cfgScale": 6, "cfgRescale": 0.5, "sampler": "k_euler_ancestral",
    "noiseSchedule": "karras", "seed": 0, "seedMode": "fixed",
    "ucPreset": 3, "qualityPreset": "none", "qualityToggle": false,
    "transparentBackground": false, "smea": false, "smeaDyn": false,
    "variety": false, "fileNamePrefix": "" },
  "preciseReferences": [],
  "characters": [],
  "panels": []
}
```

Panel 12 键:

```json
{
  "id": "<uuid>", "index": 1, "title": "<项目标题>",
  "prompt": "<继承模板原文,仅身份/场景块按修改替换>",
  "preciseReferences": [], "charactersMode": "custom", "characterRefs": [],
  "customCharacters": [
    {"prompt": "<CC1 继承原文,仅身份块替换>", "negativePrompt": "<继承原文>", "useCoords": false, "x": 0.9, "y": 0.3},
    {"prompt": "<CC2 继承原文>", "negativePrompt": "", "useCoords": false, "x": 0.5, "y": 0.5}
  ],
  "paramsOverride": { "enabled": true, "params": { "stylePrompt": "", "steps": 28,
    "cfgScale": 6, "cfgRescale": 0.5, "seed": 123456, "sampler": "k_euler_ancestral",
    "noiseSchedule": "karras", "smea": false, "smeaDyn": false,
    "model": "nai-diffusion-4-5-full", "ucPreset": 3, "qualityPreset": "none",
    "variety": false, "seedMode": "fixed" } },
  "status": "ready", "candidates": [],
  "imageSize": {"width": 832, "height": 1216}
}
```

**禁止新增软件没有的字段**:`base_prompt` / `content_prompt` / `cc1` / `cc2` / `delta` / `promptMode` / `compiled_positive_prompt` / `characterStates` 等一律不写。槽位数 = 模板原槽位数(单角色 1 槽/双角色 2 槽/三角色 3 槽,结构、坐标、useCoords 均沿模板)。

### 3.3 交付前必检(JSON 结构合规)

```
✓ 顶层 11 键 / panel 12 键(与模板逐字段一致)
✓ paramsOverride:enabled=true + params 14 键
✓ customCharacters[]:五键(prompt/negativePrompt/useCoords/x/y)
✓ panels.length == 期望格数(默认=模板格数)
✓ title 全格一致;status="ready";index 从 1 连续
✓ 无任何新增字段(§3.2 清单)
✓ json.loads 可解析
```

---

## 4. references 读取指引

| 用途 | 读什么 |
|---|---|
| **找模板(唯一入口)** | `references/template-index.json`(30 套实测索引 + scene_aliases) |
| **检索/选择算法** | `references/template-selection.md`(打分/Top-K/模式 A-D/【Template Match】报告) |
| **克隆与修改规则** | `references/template-mutation.md`(deep copy/四操作/Change Budget/两 Gate/格数压缩与扩展) |
| **只读源模板(30 套原文)** | `references/template-library/template_001.json ~ template_030.json`(READ-ONLY,禁止写回) |
| 校验(Validator):作者签名级度量区间 | `references/style-guide.md`(权重分布/括号密度/男卡率/位序指纹;**仅校验,不是生成源**) |
| 校验(Validator) + 补丁词库 | `references/tag-vocab.md`(内容词库;仅用于最小补丁与写法定级) |
| 剧情结构参照(仅作文本背景) | `references/narrative-templates.md`(节奏/工期模板;模板在则模板优先) |
| 完整单格范例(仅作文本背景) | `references/examples.md`(示例中的 artist/quality 仅作背景,不用于克隆) |

### 4.1 各优先级冲突时的处理

- index 找不到记录:以 `template-library/` 实际文件为准(索引是导航工具,不是想象出来的 metadata)。
- style-guide 与 Primary Template 冲突:以 Primary Template 为准,并记录偏差。
- 用户要求与模板冲突:按用户要求改(并在报告里说明原始行为)。

### 4.2 三 Gate(交付前必跑,详见 template-mutation.md §7)

```
① Anti-Rewrite Gate :选中模板后若大量 panel 被完全重写(原文保留率 <90% 身份替换。
                       针对场景替换 <80%)→ 回滚。
② Identity Leak Gate:旧角色名/发色/眼睛/专属服装/固有道具残留 → 清除;但不得
                       误删剧情状态/镜头/动作结构。附:服装状态链校验
                       (变更格=模板同格/只减不回穿/负权防回穿随链换词,见
                       template-mutation.md §7.4)。
③ Scene Leak Gate  : 场景替换后原地名/环境物/场景道具残留 → 清除;只处理真正的
                       Scene Leak,不借此重写其他 Prompt。
```

---

## 5. 有效性与验收

- 模板库 30 套只读,任何一次生成都不覆盖 `template-library/`。
- 每套交付后,报告里列出:Primary Template、修改范围(Character/Scene/User Delta)、保持不动项、Gate 检查结果。
- 若产生的结果出现"Template selected but output mostly regenerated"(模板选了但输出基本是新写的)→ 判定流程违规,退回 Step 5 按模板原文重新复制。
