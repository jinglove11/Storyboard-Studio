# Template Selection(模板检索与选择)

> 本文件只负责一件职责:**从 30 套真实作者成品中选出 1 套 Primary Template**。
> 索引(`template-index.json`)只是导航工具——**真正生成时必须重新打开 `template-library/template_XXX.json` 的完整原文**。

## 0. 只许有一个 Primary Template

```
ONE REQUEST = ONE PRIMARY TEMPLATE
```

整个新项目以这一套作为骨架(阶段顺序/每格 prompt 基础文本/镜头序列/画幅序列/权重体系/CC 结构)。
**禁止默认跨套拼接**("T003 的开头 + T017 的中段 + T022 的镜头")。只有用户明确要求"融合两套"时才允许引入 Secondary Template,并且必须把融合点明确列出、不得吞掉 Primary 骨架。

## 1. 检索:先归一化,再打分

### 1.1 用户需求解析

用户输入解析成查询向量:

```
query = {
  scene_family / exact_scene:   场景族 + 精确场景(如 park / night park)
  location_tokens:              地点词(公园/公园长椅)
  time:                         白天/夜晚/黄昏 等
  characters:                   人物数量、女主身份标签、男主身份(教师/出租车司机/匿名怪男)
  narrative_type:               剧情类型(强暴/睡眠姦/痴漢/把柄/泥酔/囚禁)
  pace_hint:                    格数诉求 / 节奏诉求
  special:                      用户明确给出的事件/道具/镜头
  desired_panel_count:          用户要求的格数(默认 None = 继承模板)
}
```

### 1.2 归一化(同义词 → 场景族)

凡是地点词先过 `template-index.json` 的 `scene_aliases`:

| 用户说 | 归一化后 |
|---|---|
| 公园 / park / city park / outdoor park / green space / 夜间公园 | `park` |
| 办公室 / office / 事务所 / workplace | `office` |
| 出租车 / taxi / 车内 | `vehicle` |
| 学校 / 教室 / 学校厕所 / 保健室 / 体育仓库 | `school`(再用精确词细分 school/toilet/medical_room/gym) |
| 更衣室 / locker room | `changing_room` |
| 森林 / 山 / 露营 / 树下 | `forest` / `mountain` |
| 民宿 / 旅店 / 宿屋 | `inn` |
| 空房子 / 废弃屋 | `house` |

每次检索都必须打开 `template-index.json` 的 `scene_aliases` 查询,不得凭记忆写死映射;若将来别名表扩充,以表为准。

### 1.3 同场景族优先(硬性过滤)

- 用户明确了地点,先**只保留与该场景族兼容的模板**,不兼容模板直接出局,不进 Top-K 池。
- 例如:用户说"公园" → 候选池 = T010 夜の公園で犯される中野三玖、T025 痴漢に犯される宝鐘マリン(均为 park)→ 在池内按 §1.4 打分。
- 用户说"出租车" → 池 = T003、T004。
- 用户说"学校教室" → 池 = T008(体育仓库)、T009(保健室)、T015(教室)、T021(教室双人)、T023(用務員)、T024(厕所)、T029(体育馆)、T030(厕所)。
- **用户说"公园"时,禁止把办公室模板(score=0.31)和公园模板(score=0.94)放进同一随机池。**

### 1.4 打分(池内排序)

打分基准权重(满分 1.0):

```
场景/地点精确匹配          35%
剧情结构匹配              20%
人物数量匹配              15%
时间/环境匹配            10%
节奏匹配                 10%
道具/构图/镜头           10%
```

- 场景分:exact_scene 命中给 35,scene_family 命中给 25,同族不同精确场景按别名距离递减。
- 剧情结构分:叙事模板关键词重叠(强暴/睡眠/痴漢/把柄/泥酔/囚禁/双人)。
- 人物数量:female=1/2、male 身份(教师/司机/匿名者)匹配。
- 节奏:`pace`(fast/standard/slow)+ `panel_count` 与用户期望格数的距离。
- 道具/镜头:用户点名了道具(如"篝火""纸板床""夜间公园的灌木丛")就在池内检索 `important_props` / `keywords`。

输出池内分数,取 top。

### 1.5 Top-K 随机

```
默认 Top-K = 3
若 Top1 - Top2 >= 0.15 → 直接选 Top1(不随机)
否则 → 在 Top3 内随机
```

随机时按分数加权(分数越高权重越大),不是均匀随机。用户说"随机选一套"时执行此逻辑;**禁止在全库 30 套无差别随机**。

## 2. 相似度不足时(score < 0.55)

不要假装有完美模板。执行:

```
选最接近的一套(不管分数多低)
→ 在报告里明确标注:该模板需要 Scene Adaptation
→ 尽量保持 Template Skeleton(阶段/镜头/画幅/权重体系)
→ 待 mutation 阶段处理场景替换
```

禁止从零生成整套(对应模式 D,仅当 30 套完全没有任何可用结构时才允许,并且必须先向用户说明)。

## 3. 无地点信息时的选择

- 用户只说人物(无地点):从**与该角色设定兼容**的模板中按 §1.5 随机,场景尽量继承模板。
- 用户只说地点:按 §1.3 选同场景族模板,人物结构占位继承模板;若用户没给角色,请求必要角色信息,不得自己发明角色身份。

## 4. 运行模式

| 模式 | 触发条件 | 操作 |
|---|---|---|
| **Mode A — Exact Clone** | 用户需求与某模板高度一致(同场景+同剧情+同人物数) | 复制模板 → 只换人物 |
| **Mode B — Scene Clone** | 剧情结构接近,但地点不同 | 复制模板 → 换人物 → 全套替换场景 |
| **Mode C — Nearest Template** | 无完全匹配(score<0.55) | 选最高相似 Primary → 保留阶段/镜头/Prompt Grammar → 只修改冲突部分,标记 Scene Adaptation |
| **Mode D — From Scratch** | 30 套完全无可用结构(默认禁止) | 才允许回到旧"抽象规则生成",仍参考 style-guide/tag-vocab/narrative-templates |

## 5. 输出【Template Match】报告(默认非阻塞)

> **默认流程**:报告作为 Step 3 结果直接展示,不等待确认,自动执行 Step 4-9(检索 → Clone → 输出 JSON 一次完成)。
> **只有两种情形停等用户选择**:
> ① 用户显式要求"先让我选模板";
> ② Top1 与 Top2 差异极小(<0.05),且两者的修改代价明显不同(如一个只是换角色、另一个需要全套换场景)——此时列出两者供用户决定。
> 其余情况一律自动继续,报告与 JSON 一起交付。

格式(短,不写长篇):

```
【Template Match】

Primary Template:
T010 夜の公園で犯される中野三玖(83 格, pov 56.6%, 夜)

匹配原因:
- 场景: 公园(park)exact_scene=night park
- 剧情结构: 夜公园强暴, victim 校内制服 → 断片的持续性流程
- 人物数量: 1 女 + 1 匿名男(faceless man)+ 男卡率 80%
- 节奏: standard / first_sex=P6

本次修改范围:
- Character Replacement(nakano miku → 用户角色)
- User Requested Delta(用户指定道具/剧情变更)

保持不动:
- Panel Structure(83 格,继承模板格数)
- Camera Rhythm(dutch angle 63/pov 47/..., 画幅序列逐格继承)
- Prompt Grammar / Weight Syntax(逐字)
```

报告中必须写清"本次修改范围"与"保持不动"两栏;若包含 Scene Replacement 还要给出 Scene Mapping 摘要(见 template-mutation.md)。
