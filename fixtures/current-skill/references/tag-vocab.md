
> ⚠️ **Template Cloner 模式下的角色:VALIDATOR / FALLBACK**
> 本文件是历史经验(基于 30 套作者样本的统计摘要),在克隆模式中已从 GENERATOR 降级。
> 生成正文以 `template-library/` 原文与 `template-mutation.md` 为准;本文件只用于:
> (a) 校验克隆结果是否仍在作者实测区间内; (b) 模板必须新增少量内容时的补丁参考。
> 冲突时:真实作者作品 > 本文件统计结论。

# 高频标签表 + 骨架模板 + 加权语法 + 防错词 + 画师混排 + 负面 + 参数

> 来源:《作者风格指南》(references/style-guide.md)第 8-15 节整理。供 SKILL.md 第 4 步(逐格写提示词)使用。**所有写法均保持作者原样,禁止用通用 NAI 写法替换。**

## 1. 单格骨架模板(13 槽位 + CC 双卡)

### 主提示词(顺序固定,92-98/92-98 无乱序)

```
① 画质头  2.6:: masterpiece, best quality, detailed face: ::, year 2024,
② 风格组  X::muted color ::, X::realistic, 3d(, photorealistic), ::, 1.0/0.5::ai-generated::, 0.6::ai-assisted, ::[,], X::lineart(, clean lineart, thin lineart), ::[`0.6::shiny skin,::` 可选成员——**三玖/公园系必需**,位于 ai-assisted 与 lineart 之间;`4.5::anime coloring, ::` 可选成员——三玖/公园系必需,位于 lineart 与胸块之间]**——同题材仿写照抄原型套头部逐字**
③ 女体   X::(medium|small|large) breast::, X::loli,, ::
④ 光影   X:: overexposure, (right|bright) lighting, monochrome,::, X::low light(,,night)(,, dim, dark) ::, X::shadow(, breast shade), shade, deep shadows::——**块数随套**:三玖/公园系=单块 `-4.5:: overexposure, bright lighting,breast shade::`(无 low light/shadow 块);深夜系=多块(`6.9::low light,night, dark ::`+`-6::shadow, shade, deep shadows::`)。同题材仿写照抄原型套,禁止跨套混用
⑤ 画师组 见 §6(逐字)
⑥ X::flat color::
⑦ 服装块  -N/+N::(lifted up|open) shirt, (white bra, )nipples, :: / nude / torn pantyhose…
⑧ 场景块  套级常量、段内逐字不动(词库见 §10)
⑨ 男块    X:: 1 nude (big|fat|muscular) (faceless) man, (body|stomach) hair(, brown skin)(, medium penis)::
⑩ 男脸 glom  -2::male blush, male teeth, (window, outdoors), separated screens, ::
⑪ uncensored
⑫ 自由区(每格重写) [表情簇:嘴→眼→鼻红] [体位簇] [视角簇(dutch angle 最先)] [部位簇(focus pussy/ass)] [0:: 存档块] [-N:: 防错散点] [括号簇]
⑬ 尾句   nsfw(,可插 location/from side/upper body 等补丁), very aesthetic, masterpiece, no text
```

### CC1 女主卡(7 位)

```
official style, <角色名>(, (1st costume)),
→ [服装块 N::…::](状态机)→ [-N::官方装备防错] → [眉] → [鼻红] → [anime coloring N::]
→ [1.0:::sobbing with transparent  teardrop::] → [3::arms behind back, bound wrists(, tape gag)] → [尾块:手位/表情/剧情标签(如 4::rape::)]
```

### CC2 男卡(72-79/83-92 存在)

```
boy, → [位姿块 2-5::] → [手部动作块 3-6::] → [-N::male head / male teeth](可加 male head ouf of the frame)
```

## 2. 固定串(逐字,不可动)

- 头部:`2.6:: masterpiece, best quality, detailed face: ::, year 2024,`(イレイナ型为 1.6::;`detailed face:` 后空格+`::` 残标保留)
- 画师:`0.7::artsit: jon_(pixiv31559095)  ::  ,0.7::artist: houkisei  ::0.5::artist: modare ::,,0.4:: artist: sy4 ::`
- 尾句:`nsfw(, …), very aesthetic, masterpiece, no text`
- glom:`-2::male blush, male teeth, (separated screens), (window, outdoors), (close-up), (blood on penis), ::`
- CC1 身份行:`official style, <角色> (1st costume),`(佐天型双逗号 `official style,, saten ruiko,`)

**每格必有块(占比)**:画质头 100% / 画师组 100% / CC1 official style 100% / anime coloring 100%(4.0-7.0 档)/ uncensored 90-100%(序幕 1-9 格常无)/ 尾句 88-100% / 男脸 glom 68-87 格(模板帧也挂)。

## 3. delta 规则(第 4 步核心)

### 三层模型

| 层 | 内容 | 变化频率 |
|---|---|---|
| L0 冻结层 | 画质头、CC1 身份行+常量块、尾句三件套、画师组 | 永不 |
| L1 段冻结层 | 服装块、场景块、男块、glom、CC1 表情档位、袜权 | 阶段边界/事件帧 |
| L2 逐格层 | 自由区全部、CC2 全卡、nsfw 位置、画幅、seed | 每格 |

### 数值基准

相邻格 Jaccard 均值 ≈0.6-0.7(实测三玖 0.654、AZKi 0.581、泥酔 0.624);普通推进**全提示词保留率 55-60%**(实测三玖 0.785——是大面积自由区重写,不是复制微调);**红线:相邻格 Jaccard 应 ≤0.7,若 >0.75 = 自由区重写不足(过度模板化),须每格重排构图簇/表情簇顺序+增删括号包裹句返工**;每格加权块 21-51(中位≈37);普通推进=**自由区 8-15 个加权/括号块成对重写**(勿只改 4-6 舱);段际 <0.47;每套 ≥4 对逐字节/近零差对(含**同帧双抽对**:同 prompt 改 seed/画幅,实测三玖 P24≈P25);0:: 存档 **0.0-0.9/格,按原型套走**(三玖 0.7/格、泥酔系 ≈0/格——泥酔系用 `0.0:: eye closed, crying ::` 等脚本式寄存器替代 0:: 散点,勿每格填充)。

### 六种事件的手法

| 事件 | 手法 |
|---|---|
| 普通推进 | 舱级重写,主题词包封闭复现(closed eyes+tears/head back/clenched teeth 往返) |
| 服装变化 | ①整块替换 ②块内加词(ノエル P63 同块插 `lifted up skirt, pussy`)③负权→翻正(AZKi P22→P23)④CC 整行重写 ⑤删词留位 |
| 场景切换 | 整舱替换+前值(光块/flat)微调+画幅/cfg 换挡;单帧硬切 |
| 阶段切换 | 五系统同日跳变(乳量+表情档+场景+男块+服装) |
| 模板帧 | 复制旧模板,只改①签名块 0.0↔4.0/5.0 ②CC1 glare 符号翻转 ③角色卡追加状态行 ④画幅/seed |
| 同帧双抽 | prompt 逐字节,只换 seed+画幅 |

## 4. 段内排序习惯

主提示词位序:画质→年代→色调→写实→AI 标记→线稿→女胸→女龄→曝光→低光→阴影→画师→flat color→服装→时间→地点→男块→男脸负组→uncensored→自由区(表情→体位→视角→部位→防错散点)→人数裸词→nsfw→结尾。

CC1:official style→角色→服装→装备防错→眉→鼻红→anime coloring→sobbing→缚腕→尾缀。CC2:boy,→位姿→手部动作→男脸负权。

自由区内序(シリカ):先眼后嘴——`wide-eyed/closed eyes → tears → surprised/scared → open/closed mouth → nose blush 殿后`;构图块 `dutch angle` 最先,随后 from above→side→pov→cowboy shot→solo focus→upper body;`cowboy shot, solo focus` 成对;`pink anus` 是肛专属标志位。

**排印指纹(必须模仿)**:块间 `::`+双空格+逗号堆叠(`5.9::low light,, dark ::`);`:::`/`::::` 残位;逗号雨 1-13 个;全角字符混入;括号残缺(`{{head tilt}]`、5 开 4 闭);**拼写漂移**按 §11 词表每套随机分配 10-30 处。

## 5. 加权语法

### 数字权重档位表(区间 0.0-9.5 / -0.2~-9)

| 档 | 语义 | 典型 |
|---|---|---|
| 9-9.5 | 单帧"眼点"/焦点唯一性 | `9.5::focus penis, cowboy shot, thick legs`、`9::, head out of the frame`、`9::from below, focus pussy, ass focus`——**全部是构图/焦点/面外词,无一例是体液性词** |
| 7-8 | 必须出现的体位/焦点/重表情 | `8::focus pussy`、`8::hip-focus`、`7.5::spread legs`、`7.5::nose blush,tears` |
| 5-6.9 | 次重体位/构图/场景锚 | `6::legs up, m-legs`、`6::standing sex`、`6::night , dark::` |
| 4-4.9 | 基础强调(主力档) | `4::m-legs`、`4.9::low light`(多数套固定气氛值) |
| 2.5-3.9 | 常规场景/男块/标准动作 | `2.5::missionary, pussy`、男块 1.5/2.5 档 |
| 1.2-2.5 | 弱强调 | 地点 1.2-1.5、男块 1.5、服装 1.4-2.2、画质 2.6 |
| 0.4-0.9 | 画师/血(0.3-0.9)/瞳 | |
| 0.0 | **存档/药丸**(残标占位/语义弱化不强制显形/括号让位) | 射精 0::→4:: 存档重锤;模板 0.0 签名块;0.5-0.9/格(仅关键帧,勿每格填充自创存档词) |
| -0.2~-1.5 | 弱压制/铺垫 | `-0.2::anal sex` 预埋→后转正 3-7 |
| -2~-3 | 标准防错(男脸/构图/装备/部位) | |
| -4~-9 | 强杀(以超重负权代删) | `-9::separated screens,close-up` |

### 括号层级

`{tag}`=轻提/防拆;`{{tag}}`=构图句/模板词;`{{{tag}}}`=必须出现(模板 `{{{open mouth}}}`/`{{{cum drop}}}`);`{{{{}}}}`=超强(`{{{{forced irrumatio}}}}`、负面内 `{{{{censor, bar censor}}}}`);`{{{{{}}}}}` 五层=**群交段/性核心词专用**(`{{{{{2boys}}}}}`、`{{{{{gang bang}}}}}`、`{{{{{sucking another's breast}}}}}`);`[[tag]]`/`[[[tag]]]`=表情/属性二极管(`[[empty eyes]]`、`[[[[defloration blood]]]]`、`[[oil painting (medium)]]` 防油画)。**密度实测基准(同套仿写对标原型套,勿拍脑袋)**:保健室ミコ系花括号 **20.0/格**、泥酔系 19.3/格、`[[..]]` 0.8/格、四层 `{{{{…}}}}` 0.3/格、负权字簇 `-N::` 6-10/格;AZKi/タクシー系花括号 6.0/格;三玖/公园系花括号 8.6/格(均值,峰值 70)。

**⚠️ 花括号硬配额(仿写头号红线,违反=一眼假)**:正戏每格自由区**必须**含 8-20 个 `{}`/`{{}}` 包裹的**姿态/视角/部位短句**,与数字权重块**并行**(数字定价、括号定构图),**不得用数字权重替代括号**。达成法:每格把体位/机位/部位写成括号句 `{{lying on back}} {{spread legs}} {{pussy}} {large penis} {{dutch angle}} {{1boy, nude man}}`,性核心词升五层 `{{{{{sucking another's breast}}}}}`。参考 ミコ P30 单格实测 15+ 括号句。**交付前测本套花括号/格,低于原型家族基准即返工**(实测教训:仿写曾只 1.1/格 vs 原型 20/格)。

**分工**:数字权重=定价;花括号=加锁(无数字价);方括号=属性锁定;负权=禁止定价。**高潮表达=更厚括号而非更高权重**(イレイナ S7 每格 24-43 花括号、数字权重反而下移)。

### 阶段对比

开场/街段:≥3 权块 3.5/格、无 nsfw/uncensored、视角词裸写、括号≤10 → 前戏:表情权首升(4-8)、负权预埋 -2::sex → 插入/破处:峰值焦点权(7-9.5)、血低权、光块最暗、多系统同日跳变 → 口交/深喉:行为权全局最高(9.5::deepthroat)、男块 2.5 → 群交/终局:括号簇、男块 2.5-3.5::2/3 男、人数锁定 → 模板帧:权重全面回落(≥3 权块 3-6/格),强调靠 {{{}}}/{{}}。

## 6. 画师混排

主组(逐字):`0.7::artsit: jon_(pixiv31559095)  ::  ,0.7::artist: houkisei  ::0.5::artist: modare ::,,0.4:: artist: sy4 ::`——`artsit` 100% 错拼、无 "by"、CC 卡零画师、套内逐格不变。

变体:sy4 0.3(イレイナ/夜の公園三玖)/0.4(主流)/0.6(いろは);加 a_re `0.2::artist: a_re`(夜の公園三玖 83/83)或 `0.0::`(AZKi 开场后删);第四位换 mamerakkk-kko/imo norio(桃鈴ねね)/satou_kibi/mamerakkk-kko/imo norio(更衣室ラミィ)/ask askzy/gweda/mako_makoda(雪ラミィ痴漢段B);定末组可加 rurudo 0.4(トワ 10-14 格)。

## 7. 防错词清单

### 行内固定级

| 词 | 防什么 |
|---|---|
| `uncensored`(正) | 官方打码(序幕未点荤帧无) |
| `no text`(正) | 台词/字 |
| `nsfw`(正,偶写 `sfw`) | safe 化 |
| `-2::male blush, male teeth, separated screens`(负) | 男脸红/男牙+分屏 |
| `faceless`(男块正词) | 男脸符号(漂移常态) |
| `-N::loli,, ::`(负) | 幼化(与正权胸块互为锚定) |
| `-N::overexposure/right lighting/monochrome`(负) | 过曝/错光/黑白 |
| `-N::shadow, shade, deep shadows`(负) | 阴影糊 |
| `-N::realistic, 3d`(负) | 写实化 |
| `-N::lineart, thin lineart`(负) | 线稿化 |
| `-N::flat color`(负) | 平涂 |
| `-N::muted color`(负) | 灰调 |
| `official style` + `(1st costume)` | OOC/官方造型定格(P7 起转轨+装备负权) |

### 行内阶段级(随段增减)

`-2::outdoors, window(, sky)`(室内套防穿景)/`-2::other people`(街段)/`-2::sakura`(樱花感)/`-2::bed, pillow, navel`(床景污染)/`-3:: skirt, pussy, kneeling, sitting, skrit , ass`(口交轮防串帧)/`-2::stomach bulde, on side, bound legs, cowgirl position`(模板尾固定成员,防错误姿势组合)/`-4.5::face, trembling, looking back…`(防女脸误入)/`head out of the frame`·`male head ouf of the frame`(男头出画)/`only 1girl`·`1boy`·`2boys`·`only 2 penis`(人数锁定)/`[[oil painting (medium)]]`+`thin drawing lines`(画风锚)。

### 与全局负面冲突的正权词(保留)

`pov hands`(正 28-61/92 vs 负面 POV hands);`kneeling`(正 8-20 格 vs 负面首词);`x-ray/cross-section` 正负并用;`separated screens` 少数套 1 格正权。

## 8. 负面提示词(全文逐字,永不修改)

```
kneeling, blurry, lowres, error, film grain, scan artifacts, worst quality, bad quality, jpeg artifacts, very displeasing, chromatic aberration, multiple views, logo, too many watermarks, camera, tiara, coat, graffiti, Boxing glove, futa, poorly drawn, lowers, bokeh, low quality, out of focus, ugly, {{{{censor, bar censor}}}}, mosaic censorship, puffy nipples, extra digits, POV hands, hutanari, low quality:1.4), text, bad anatomy, watermark, extra fingers, missing fingers, extra arms, missing arms, extra legs, counter, body writing，
```

(尾部为全角逗号;`low quality:1.4)` 残句;四括 censor=全系列最大禁忌。CC 级负面默认 "",仅部分格 `"lowres, aliasing,"`。)

## 9. 参数表

```
model: nai-diffusion-4-5-full | steps: 28 | sampler: k_euler_ancestral | noiseSchedule: karras
cfgScale: 6(全局)/6.4(关键帧:模板帧·破处·射精·脱衣·关键特写;ノエル另有 6.5×1)/マリン型可全 6.0
cfgRescale: 0.5(双落点帧可 0.4)| ucPreset: 3 | qualityPreset: none | smea/smeaDyn/variety: false
seedMode: fixed | 全局 seed 722652769(从不采用)| 每格 seed 独立互异 | initialGenerationCount: 1
sizeMode: uniform | stylePrompt/positivePrompt: "" | transparentBackground: false
```

每格 paramsOverride 全量覆盖 14 键(见 SKILL.md §4.3),只改 seed/cfgScale/imageSize;negativePrompt 与 stylePrompt 从不逐格覆盖。

## 10. 场景词库(12+ 套级词库,段内逐字不动)

`in japanese house,on white futon(, head on white pillow), ::, 1.2::tatami::` / `in park, bush, on grass(, dirt), metal mesh wall, wire mesh wall, dark` / `in school, clubroom(, on grey floor)(, school desk/chair)(, bookshelf)` / `in dark room, light grey floor, dim(, 4::on dirty wooden desk::)` / `in dark messy room, on tatami, dirty` / `in dim messy room, on wooden dirty desk, on tatami` / `in inn(, medieval|wooden inn), wooden wall, on white bed, white pillow` / `in shower room, shower, white wall, white tile floor` / `under a tree, tree shade, dappled sunlight, sunlight filtering through leaves` / `in wilderness, rock, on dirt, bush(, on tree)` / `in park, bush, dirt, 5::on cardboard::` / `in locker room, on wooden floor, grey locker` / `in public toilet, white wall, toilet` / `2::residential area, house` / `in taxi, night, city, sitting on car back seat, looking to window, relaxed, seatbelt` / `in music recording studio`(道具变体:肩包/书包/游泳圈/购物袋/黑手机/录像取景框 rec/白纸板/长课桌/pens and paper)。

残留词是常态(マリン "window, outdoors" 57 格残留),保留不清理。

## 11. 拼写漂移词表(每套随机分配 10-30 处)

`artsit`(画师,必带,100%)、`missionafry`、`mi3ssionary`、`struddle`、`pensi`、`extreamly egs up`、`foward`、`peneteration`/`penis penetation`/`penetratoin`(三族)、`nigiht`、`blsuh`、`glairng`、`fellatoi`、`head ouf of the frame`、`deepthroa::t`、`leaning backk`、`on wal`、`ccross-section`、`dutch anle`、`fown`、`skrit`、`thighhihg`、`baefoot`(barefoot)、`bloodo/bloond on penis`(血三拼写)、`sfw`(nsfw 误写 2-5 处)、`low quality:1.4)`(负面残句)、`012::`(无点权重)、`d0::enim`、`nave`、`clenhed`、`surpirsed`、`uppre body`、`agasint`、`unifrom`、`shinny`、`pusy/pushy`、`analn`、`ams`、`scholl chair`、`plsuh`、`facial`→`facrless`、`waiza/wariza` 正错并存、`positoin/positioin`、`anusk`、`frorm side`、`from abovet`、`abovet`、`tgirl`、`malde`、`m-lesg`、`girl:s`、`bule` 等——各词在原套均曾出现,分布规律:同词多次出现处优先漂移;眼/口/体位/画师词最易漂。

## 12. CC 卡词汇库

- **服装记账法**:`(4::)completely nude and lifted up white bra, white panties aside , pink line bra and panties::`(シリカ型);`4::lifted up white  t-shirt and  ,lifted up black bra,  denim shorts aside,  ::`(マリン型);`3::school uniform, light blue sweater,torn pantyhose, green skirt::`(三玖型,单块);**分块记账法**(实测夜の公園三玖 P1/P46 型):`3::school uniform, light blue sweater,::   1.3:: pantyhose, green skirt::  2::headphone around neck,:::`——上装/下装分块 + `:::` 残位,模板帧可改单行权重(如 P83 `-4:: pantyhose, green skirt`)。
- **表情三件套**:`X::nose blush, (scared|embarrassed),  ::  1.5::furrowed bro, teardrop::`(三玖 5.4/1.5;"scared↔embarrassed 按段互换"是表情状态机)。
- **苦难常量卡**(P11 起锁定):`2.4::nose blush,  :: 4:: despair, fear,terrified ,::,1.0::,angry, dark circles::, 1.3:::sobbing with transparent  teardrop::  ,5.5::anime coloring,  ::  3::arms behind back::bound wrists::   4::tape gag,:: -3::clenched teeth ,::`(マリン型;权重档 0.6-2.0 波动)。
- **缚腕**:`3::arms behind back, bound wrists( by handcuffs)`;胶带 `tape gag`;"缠缚"(シリカ)同义。
- **男卡动作库**(全部"手在做什么"具象短语):`ass grab`/`pull down panties`/`spread pussy`/`penis grab and ass grab by single hand,left hand and right hand,, penis on pussy, before sex`/`grab from behind by single hand`/`torso grab`/`breast grop from behind`/`double hands to breast grab`/`hip grab`/`leg grab`/`boy's thighs under girl's leg`/`knees under girl's leg`/`hand on another's head`/`head grab`/`penis grab to anal sex, penis penetration`/`grab own pensi to anal sex, penis grab to penetration`——从不写 rub/lick 类动词。**姿势补给簇(泥酔ラミィ实测,入典)**:`prone bone`/`mating press`(P59-75 共 5 帧)/`irrumatio`/`forced fellatio`(前戏口交 P13)/`-0.2::anal sex` 负权预埋→`3::pov, from behind, ass focus, pink anus::` 肛交簇(19 帧,配 `-0.4::deep penetration` 双负权)/`on all fours, hip up`/`boy on top, forced fellatio, grab own penis to insert mouth and another hand on head`。
