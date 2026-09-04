
> ⚠️ **Template Cloner 模式下的角色:VALIDATOR / FALLBACK**
> 本文件是历史经验(基于 30 套作者样本的统计摘要),在克隆模式中已从 GENERATOR 降级。
> 生成正文以 `template-library/` 原文与 `template-mutation.md` 为准;本文件只用于:
> (a) 校验克隆结果是否仍在作者实测区间内; (b) 模板必须新增少量内容时的补丁参考。
> 冲突时:真实作者作品 > 本文件统计结论。

# 范例:3 条完整单格 + 连续 5 格 delta 片段

> 来源:《作者风格指南》第 18-19 节。全部为原文照录(含拼写错误与空格),仿写时以此为"尺子":凡输出与之同构者即像;以下每条附"仿写要点"。

## 原型套头部对照表(第 4 步:先查原型套,头部逐字照抄,禁止跨套混用)

| 头部系 | 逐字串(段内顺序,含空格/逗号) | 适用套 |
|---|---|---|
| 三玖/公园系 | `-0.3::muted color ::, -0.5::realistic, 3d, photorealistic, ::, 1.0::ai-generated::, 0.6::ai-assisted, ::, 0.6::shiny skin,::  -2::lineart, clean lineart, thin lineart, ::   4.5::anime coloring, ::,` 女体 `2::medium breast ::,  -0.8::loli,, ::` 光影**单块** `-4.5:: overexposure, bright lighting,breast shade::`(无 low light/shadow 块) | 夜の公園三玖 83 |
| 弱み/深色调系 | `-0.5::muted color ::, -0.5::realistic, 3d,::, 1.0::ai-generated::, 0.6::ai-assisted, ::  -1::lineart, thin lineart, ::` 光影多块 `-5.0:: overexposure, bright lighting, monochrome,::  4.9::low light,, dark ::   -7.5::shadow, shade, deep shadows::` | 弱み三玖(无 shiny skin/anime coloring) |
| 深夜系 | `-0.5::muted color ::, -0.5::realistic, 3d,::, 1.0::ai-generated::, 0.6::ai-assisted, ::  -1::lineart, thin lineart, ::` 光影多块 `-6.0:: overexposure, right lighting, monochrome,::  6.9::low light,night, dark ::   -6::shadow, shade, deep shadows::` | 痴漢マリン(夜路) |
| AZKi/タクシー系 | `-1.0::muted color ::, 0.5::realistic, 3d, photorealistic, ::, 1.5::ai-generated::, 0.6::ai-assisted, ::  0.0::shiny skin,::  3::lineart, clean lineart, thin lineart, ::   1.0::anime coloring, ::, 2::large breast ::,  -0.8::loli,, ::` 光影多块——**realistic/lineart 取正权(与三玖/公园系负权相反),含 shiny skin+anime coloring** | タクシーAZKi(计程车夜景) |
| **0.8/amazing系(泥酔·保健室ミコ)** | 头**不以 2.6 起**:`0.8::amazing quality, 4k, very aesthetic, absurdres, masterpiece::,year 2025,  -1.5::muted color ::,  -1.2::realistic, 3d, photorealistic,shade:: {{{extremely detailed hair}}}, {{year 2024}}, ,3.3::ai-generated , ai-assisted  :: 4.3::shiny skin:: ,  0.1::oiled ::, ,-0.9::anime coloring::,  -0.3::loli ,::  medium  breast,` + 画师组**换 `ask (askzy)` 等**(非 sy4)+ `no lineart 正 1.7` | 保健室ミコ 82 / 泥酔ラミィ |
| **更衣室系(2.6 头 shiny 变体)** | **仍以 2.6 起**,但 `muted 重负(-4.7)/ shiny skin 正 / anime coloring 正 / no lineart 正` | 更衣室ラミィ / 更衣室英梨々(英梨々为 2.6 标准负权) |

**关键**:realistic/lineart/shiny skin 的正负号是**套级开关**,不能按"夜景→深夜负权系"直觉推断。计程车夜景的 AZKi 反而 realistic 正 0.5、lineart 正 3.0。同题材仿写务必读原型 JSON 头部逐字。

**开场句法(三玖/公园系)**:P1 场景=**括号叙事句**`{{1 girl walking in residental area,night,  , modern house }}`(含漂移 residental)+ `{school bag shouldering}` 单括号道具句 + 残位句 `1.2::, ::` + `{{cowboy shot, solo focus}}`;街段 P1-4,公园段 P5 起,入园帧为关键帧 cfg 6.4。

## 套级开关速查表(第 2 步先定,防"头部系/torogao/终局族"三处必错)

同作者跨套有 **3 个不能互推的套级开关**,场景/角色都猜不出,必须按此表按题材家族锁定(或向用户取一个同家族原型套名核对):

| 题材家族 | 头部系 | torogao 占比 | 终局签名族 |
|---|---|---|---|
| 公园/街头夜袭(三玖型) | 三玖/公园系(2.6) | 0%(零快感派) | 标准 `.0.0/.4.0::closed legs…,o,cum on leg::` |
| 拐带/闯入纯暴力(ノエル型) | 弱み/深夜系(2.6) | 12-28% | 标准全签名块 |
| 计程车(AZKi型) | AZKi/タクシー系(2.6,realistic/lineart 正) | 28% | 标准全签名块 |
| 更衣室(ラミィ/英梨々型) | **更衣室系(2.6 头 shiny 变体)** | 38% | **简化变体** |
| 意识剥夺·酒/睡/药(泥酔型) | **0.8/amazing系** | **58%** | **简化变体** |
| 保健室(ミコ型) | **0.8/amazing系** | **57%** | **简化变体** |

**铁律**:①头部家族≠场景,`2.6` 与 `0.8` 是两个不共享数值的族,同为 2.6 头内部数值/正负号也跨套差异极大;②torogao 占比**锚定本表家族值,禁止在区间内自取,禁止被"用户指定的节奏模式"覆盖**(教训:曾一套 84% 一套 12% 横跳;保健室系明明 57% 却被误设"标准节奏 35%");③终局族选错=一眼假(简化变体见 narrative-templates §6.4)。三者仿写前若无同家族原型确认,须向用户取一个原型套名核对,**不得默认 2.6 头 + 标准签名块**。

**⚠️ 节奏模式 vs 套级开关的优先级**:节奏模式(快/标准/慢)只决定**总格数**和**服装撕裂速度**;torogao 占比由**本速查表的题材家族**决定,**家族值优先于节奏档**。例:保健室系即使用户说"标准节奏 80 格",torogao 仍应 55%+(家族值),不是标准档的 35%。**生成前第一步必查本表锁定 torogao 家族值,再叠加节奏模式定格数。**

**⚠️ 禁止纯脚本模板拼接生成 prompt**(实测教训):用"HEAD + 固定服装块 + 变量自由区"的脚本拼接法,Jaccard 天然卡在 0.82+(原型 0.61-0.68),因骨架占每格 60% token 无法后期打散。**必须逐格手写自由区**(体位/视角/焦点/表情每格重排),并按 SKILL §delta 双峰规则插入 4-9 对双抽帧,才能达到原型的 Jaccard。脚本仅用于最终 JSON 序列化,不用于生成 prompt 内容。

## 范例 1——《夜の公園で犯される中野三玖》P36(正戏态·传教士接吻帧,1024×1024,cfg 6.4)

主提示词:

```
2.6:: masterpiece, best quality, detailed face: ::, year 2024,   -0.3::muted color ::, -0.5::realistic, 3d, photorealistic, ::, 1.0::ai-generated::, 0.6::ai-assisted, ::  0.6::shiny skin,::  -2::lineart, clean lineart, thin lineart, ::   4.5::anime coloring, ::, 2::medium breast ::,  -0.8::loli,, :: -4.5:: overexposure, bright lighting,breast shade::     ::, 0.7::artsit: jon_(pixiv31559095)  ::  ,0.6::artist: houkisei  ::0.6::artist: modare ::,0.2::artist: a_re  ::, ,0.3:: artist: sy4 ::  ,0.2::flat color::,   : 2.2::,open shirt, white bra,  nipples, ::   2::dark, night, in park, bush,dirt ,  on cardboard,::  5::on cardboard::     1.5:: 1 nude fat ugly body vagrant man,  brown skin, stomach hair  ::  2::torn pantyhose::  -2::male blush, male teeth, close-up,::  uncensored, ,,4::  sensual eyes, penis penetration ::   5.5:: nose blush,, ::,,,,5::sucking nipple,one eye closed, surprised, single breast grab ::male lip::,6::wide-spread legs::   2:: clenched teeth, closed eyes with tears,, face to nipples,:: ,,,,,,,0::kiss, from side, missionary,face to face, surprised,  leaning on girl ,grabbing_another's_single breast , body to body, scared, open eyes, wide spread legs, boy on top, :::: , 4::sensual eyes :: -5::separated screens::  0.3::upside-down, focus face ::, 1.5::trembling, torogao,  :::  2:: eyes  closed::,  4::looking at boy:: , 3::cowboy shot, ::2::trembling, motion lines::   -2::feet:: , 1.2:: deep penetration,, 3::on back, :: , , from above,,spread legs, lying on back,  large penis,  nsfw,,, very aesthetic, masterpiece, no text
```

CC1:

```
official style, nakano miku,4::school uniform, light blue sweater,torn pantyhose, green skirt::  2::headphone around neck,::  5.4::nose blush, embarrassed,  ::  1.5::furrowed bro, teardrop::  4::arms behind back:: , bound wrists::  4::rape::
```

CC2:

```
boy, 4::kiss, face to face, boy on top::  2::male lips, bald hair, faceless,::-4::male blush, male eyes:
```

**仿写要点**:①画师区五件套含 `0.2::artist: a_re`;②`-0.8::loli`+`2.2::open shirt, white bra, nipples`(只开不脱);③男块常驻档 1.5+stomach hair;④`0::kiss,…` 零权挂词表(接吻帧常法);⑤`-5::separated screens`(本套 17 负 1 正的口径);⑥CC1 5.4+1.5 双表情块+`4::rape::` 剧情标签;⑦尾句 `nsfw,,,` 三逗号;⑧**头部逐字照抄**(见上方"原型套头部对照表·三玖/公园系"):`0.6::shiny skin,::`+`4.5::anime coloring, ::`+光影单块 `-4.5:: overexposure, bright lighting,breast shade::`,禁止混入 low light/shadow 块;⑨主块裤袜镜像块位置=**场景块之后、男块之后**(`…on cardboard::  1.5:: vagrant man…  2::torn pantyhose::  -2::male blush…`),不在服装块内。

## 范例 2——《弱みを握られた中野三玖》P47(正戏态·裸上身+破袜,含跨套模板尾块,832×1216,cfg 6.4)

主提示词:

```
2.6:: masterpiece, best quality, detailed face: ::, year 2024,  -0.5::muted color ::, -0.5::realistic, 3d,::, 1.0::ai-generated::, 0.6::ai-assisted, ::  -1::lineart, thin lineart, ::   , 2::medium breast::,  -1.5::loli,, :: -5.0:: overexposure, bright lighting, monochrome,::  4.9::low light,, dark ::   -7.5::shadow, shade, deep shadows:: , 0.7::artsit: jon_(pixiv31559095)  ::  ,0.7::artist: houkisei  ::0.5::artist: modare ::,,0.4:: artist: sy4 ::  0.3::flat color::, 1.2::upper body nude, ,  nipples, ::  school chair,  2:: in school, clubroom,on grey floor:: 4:: dark room, ::  2.5:: 1 nude faceless big man , medium penis,:: ,-2::male blush, male teeth, , separated screens,blood on penis ::  uncensored,, ,,,,,0.5::blood on penis:: ,  7.5::spread legs, :: 2::nose blush,tears :: 2.5::missionary, pussy, ::   4::m-legs, legs up , thick legs, medium  penis penetration,  :: ,,,,,,,,,,,,0::upside-down, wide-spread legs, dutch angle,,from side, , , ::, ,1.2::deep penetration::, ,,6::pov, from above:::,,,,1.::open mouth, trembling:: 0::clenched teeth ::  4::, looking at penis, looking down, , :: 0.8::large eye, surprised::   3.6::half-closed eyes with scared, wide-eyed,suprised, ,,thick legs ::,,4.5::from side, dutch angle, ::,,,upside-down,   7:: knees,focus pussy, ::  7::, cowboy shot:: , 5::, m-legs, legs  grab, ::, 2::folded knees,, :: ,,,4::scared, cute,  with tears ,::, cute 1.5:: motion lines, ::  1.0::open mouth, ::,  3::pov, ,pussy::, 0::head back:: ,, 1.5::lying on back,, ::   1.5::solo focus, cowboy shot::, 2:: , from above, ::  4::missionary, penis penetration::  3:: pussy, body on floor, ::   2::solo focus, cowboy shot, :: , 2::pov,, on back :: -2::close-up, feet, stomach bulde, on side, bound legs, cowgirl position, :: , boy's arms on girl's thigh,   pov hands, missionary, nsfw,, very aesthetic, masterpiece, no text
```

CC1:

```
official style, nakano miku,4::bare upper body, bare shoulder, torn pantyhose, green skirt:: -2::shirt, sweater::  -3::headphone around neck,::  3::furrowed brow ::,  1.2::nose blush,::, 3.0::anime coloring,,:: , ,1.0:::sobbing with transparent  teardrop::   ,3:: arms over breast, bound wrists by handcuffs, closed mouth::  -2::clenched teeth::,,, ,,2.7::crying, with clenched teeth, ::5::wide-eyed, surprised::,4.0::half-closed eyes with crying, pain::|||boy, 2::sitting, thick thighs::   3::legs grab:: 6::thighs grab:: -1::feet grab::
```

**仿写要点**:①E 态 CC1:裸上身+`-2::shirt, sweater::`+`-3::headphone`(装备防回归三连);②**跨套模板尾块** `4::missionary, penis penetration:: …-2::close-up, feet, stomach bulde, on side, bound legs, cowgirl position, :: , boy's arms on girl's thigh,   pov hands, missionary, nsfw`(与タクシーAZKi P19 同文——直接整串粘贴);③男块 2.5+男脸负组填料 blood on penis;④`0::upside-down…`/`0::clenched teeth` 零权存档;⑤CC2 `boy,` 双重编码;⑥`1.::open mouth` 有点号残字(1.0 误写)。

## 范例 3——《体育教師に犯される白銀ノエル》P20(正戏态·立位后入·腿部特写帧,1216×832,cfg 6.0)

主提示词:

```
2.6:: masterpiece, best quality, detailed face: ::, year 2024,  -1.5::muted color ::, -0.5::realistic, 3d,::, 1.0::ai-generated::, 0.6::ai-assisted, ::  -1::lineart, thin lineart, ::   , 2::large  breast::,  -1.1::loli,, :: -2.0:: overexposure, right lighting, monochrome,::  0.9::low light,, dark ::   -6::shadow, shade, deep shadows:: , 0.7::artsit: jon_(pixiv31559095)  ::  ,0.7::artist: houkisei  ::0.5::artist: modare ::,,0.4:: artist: sy4 ::  -0.3::flat color::, 1.4::open shirt, ,lifted white bra,   nipples, ::  2:: in school, clubroom,  :: 1.2::wooden floor:: ,0.5:: bookshelf :: 4::on long school  desk::   1.5:: 1 clothed  big muscular man standing , black pants, white shirt:: , -2::,male blush, male teeth,  separated screens:::    -2::on floor::  uncensored,, 0.4::blood on penis::,,,  4::sex, standing with leaning against desk::,,,,,,,,, 3::thick legs, thick thigh, thick feet, focus legs::    5.5:: nose blush,:::,,,,, 4::kneepits, closed eyees, head tilt, torogao, ::,,,,,,  -2::realistic soles, realistic feet skin , ::   1::toes::    ,5.5::standing, against ,,::,  ,2::large feet::   ,,3.5::trembling, torogao::, 3::one leg folded , ::,,  6::toes:: 4::cowboy shot , solo focus::  5:: focus stomach, folded , ::  one leg grab, ::,,1.5::one eye closed, , torogao, crying, slight open mouth, blush:: {{teardrop}}, {{clenched teeth}}  , penis penetration , , ,, {slight open mouth, torogao}   uncensored,  {{{nose blush}}} 3::standing split  sex from behind:: , 2:: torogao, cowboy shot,:: deep penetration::,   {upper body}, , {{standing}}, facing side,  {{{leaning back against wall}}}, {{{one leg up, one leg grab}}}, split   , {{head tilt}},   surprised, ,, {{tears}},, {{penis}} {{large hip}}, crying,  nsfw,, location, very aesthetic, masterpiece, no text
```

CC1:

```
official style, shirogane noel (school uniform), 3::checkered skirt:: -3::navel:: , white shirt,  0.8::navy socks,  neck bow,::  2.8::nose blush, embarrassed, ::  ,  1.6:::sobbing with transparent  teardrop::  3:: despair, fear,terrified ,scared,:: -0.9::glaring:: ,1.5:::,, dark circles::,,  5.5::anime coloring,  ::  3::hand on desk, ::
```

CC2:

```
boy,  5::standing behind girll, sex from behind, one leg grab::   2::grab from behind by single hand::
```

**仿写要点**:①头部唯一活参数=large/-1.1(P14 破处五系统跳变后档);②乳部块 C4 1.4+男块 M-C 1.5(clothed big muscular=破处正文档)+glom 三重冒号 `:::`;③尾句 `nsfw,, location,` 插入形;④CC1 表情底座 E1:2.8 鼻红/1.6 sob/3 despair/**-0.9::glaring**(符号翻转=-0.9/+0.9 是"情绪方向开关")/1.5 dark circles/5.5 anime coloring;⑤腿特写簇(3::thick legs…6::toes)整簇改写是 L2 层写法示例。

## 连续 5 格 delta 片段——《痴漢に犯される宝鐘マリン》P1→P5(序幕段)

**P1(日常帧,832×1216,无男)关键段**:

```
…2.6:: masterpiece, best quality, detailed face: ::, year 2024,  -0.5::muted color ::, -0.5::realistic, 3d,::, 1.0::ai-generated::, 0.6::ai-assisted, ::  -1::lineart, thin lineart, ::   , 2::medium breast::,  -1.2::loli,, :: -6.0:: overexposure, right lighting, monochrome,::  6.9::low light,night, dark ::   -6::shadow, shade, deep shadows:: , 0.7::artsit: jon_(pixiv31559095)  ::  ,0.7::artist: houkisei  ::0.5::artist: modare ::,,0.4:: artist: sy4 ::  -0.3::flat color::,     walking,  2::carrying plastic bag, carrying shoulder bag,, arms down,  :: 2::residential area, house::  {{blush}} 0.2::nose blush::   2::nigiht, :: full body, leg, ::  -2::sakura::   {{1 girl walking in park, electrical light,, tree, bench,  night,   }}, ,near park, tree, ,   closed mouth,,  [[oil painting (medium)]],  from side,  -2::other people :: , , very aesthetic, masterpiece, no text
```

**ΔP1→P2(微调格,加权块变化 0)**:唯一改动=景别块 `full body, leg, ::` → `full body, , ::`(删 `leg`);道具词/表情/防错原样复制(道具词不随景别联动=复制痕迹)。换 seed 重出一格。

**ΔP2→P3(事件帧,+3 块)**:新增 `4::pov hands, arm grab:: 3::surprised, !? :: 2::looking back, from behind`——"手臂被抓"以 POV 手+颜文字完成;其余原样。

**ΔP3→P4(施害者定型帧)**:删 `shoulder bag` 道具、新增 `1.6::1 clothed fat big faceless man, black pants,white t-shirt, body hair, standing behind girl, ::`(唯一"穿衣+完整人设"定身句,1.6 权重)+捂嘴句 `3::covering another's mouth  from behind, hug,::`。

**ΔP4→P5(猥亵帧,男块退格+女体动作激活)**:男块从加权 1.6 句退回无权重 `{{1 clothed faceless fat man, white t-shirt, , black pants, standing behind girl}}`(单括号=轻量态);新增 `1boy standing behind girl`、`2::1 boy grope her  and hand in pantyhose from behind :: nsfw`、`2::molestation ,, hug from behind, breast grab`;CC1 维持 W0(`4::white  t-shirt :: 2:: denim shorts  ::`)。

**五格合计的 delta 形态**:L0 冻结层逐字不动 5 格;L1 段冻结(街区块 A 型)只在 P5 换模板句;L2 = 删 1 词(P2)→加 3 块(P3)→换男块+删道具(P4)→男块退格+事件激活(P5)。**序幕五格的写作强度远低于正戏**(正戏为轮内重写 8-12 块),证明按 L1 段别切换写作强度。

**第 6 格断层(参照)**:P5→P6 为全篇最大语义断层(0.48 保留):街→公园铁网墙根单帧硬切,男块退场(0.0 残标),P6 新增 `-2::close-up, male blush, separated screens, sex::` 防错与 `-0.2::in bathtub ::` 残句(嫁接帧指纹)。
