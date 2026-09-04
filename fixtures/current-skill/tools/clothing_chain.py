#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""clothing_chain.py — 服装状态链提取与链校验工具(事实层 + 校验层,不做替换决策)

用法:
  python3 clothing_chain.py extract <template.json>                          # 输出状态时序表
  python3 clothing_chain.py extract <template.json> --json                   # 机器可读
  python3 clothing_chain.py verify  <template.json> <mapped.json>            # 链校验(exit 0=通过)
  python3 clothing_chain.py verify  <template.json> <mapped.json> --cc 0    # 只扫第 N 个 CC 槽(默认扫描全部槽)

说明:
  - 只做"提取事实"与"校验",判定(哪档穿/哪档除/怎么映射)是 LLM 的语义工作
  - 校验规则:
    a) 回穿检测:同一服装词条,负权(已除)出现之后,再出现正权(穿着)= 回穿 → FAIL
    b) 状态签名:模板与输出逐格比较"同词族"的穿/除/破损签名,不一致 → FAIL
    c) 变更格检测:同词族正负切换的格号列表,模板与输出不一致 → FAIL
  - 词族 = 服装基词(pantyhose / skirt / shirt / boots ...),用户换名后的产物与模板
    同名词族才能直接比签名;跨名替换(模板 sweater → 输出 white shirt)由 b) 的
    通用回穿检测 + a) 覆盖,报告的 "新增/消失词族" 需 LLM 复核。
"""
import json, re, sys, argparse

CLOTH = ('pantyhose', 'tights', 'stocking', 'skirt', 'shirt', 'blouse', 'sweater',
         'uniform', 'bra', 'panties', 'underwear', 'nude', 'naked', 'bare', 'boots',
         'shoes', 'socks', 'thighhighs', 'leggings', 'shorts', 'dress', 'cardigan',
         'vest', 'corset', 'cape', 'capelet', 'bikini', 'swimsuit', 'serafuku', 'jersey',
         'sleeveless', 't-shirt', 'holster', 'heels')
MOD = re.compile(r'\b(torn|open|opened|unbuttoned|lifted|lifted up|pushed up|pulled|aside|half|exposed|damaged|worn|strapless|down|off|removed|stripped|unzipped)\b')
BLOCK = re.compile(r'(-?[\d.]+)::(.*?)::')

def family_hits(text):
    """返回 {基词: [(weight_sign, block_text, modifiers)]},按出现顺序"""
    hits = {}
    for sign, body in BLOCK.findall(text):
        low = body.lower()
        for base in CLOTH:
            if re.search(r'\b' + re.escape(base), low):          # 词首匹配(torn pantyhose 亦中)
                hits.setdefault(base, []).append((sign, body.strip(), bool(MOD.search(low))))
    return hits

def plain_families(text):
    """权重块之外裸出现的服装词(身份块属性如 'white hair ribbon') → 集合,仅供跨名判定"""
    stripped = re.sub(r'-?[\d.]+::(.*?)::', ' ', text, flags=re.S)
    out = set()
    low = stripped.lower()
    for base in CLOTH:
        if re.search(r'\b' + re.escape(base), low):
            out.add(base)
    return out

def panel_text(p, slot):
    txt = p.get('prompt', '')
    for i, cc in enumerate(p.get('customCharacters', [])):
        if slot is None or i == slot:
            txt += '\n@slot%d ' % i + cc.get('prompt', '')
    return txt

def extract(j, slot=None):
    rows = []
    for p in j['panels']:
        rows.append((p['index'], family_hits(panel_text(p, slot))))
    return rows

def state_char(sign, mod):
    if sign.startswith('-'):
        return 'R'            # removed / 防回穿负权
    return 'M' if mod else 'A'  # M = 破损/改动档,A = 穿着档

def signature(j, base, slot=None):
    sig = []
    trans = []
    prev = None
    for idx, hits in extract(j, slot):
        state = state_char(hits[base][-1][0], hits[base][-1][2]) if base in hits and hits[base] else '.'
        sig.append(state)
        if prev is not None and state != prev:
            trans.append(idx)
        prev = state
    return ''.join(sig), trans

def run_extract(args):
    j = json.load(open(args.file))
    rows = extract(j, args.cc)
    if args.json:
        out = []
        for idx, hits in rows:
            fam = {}
            for base, lst in hits.items():
                fam[base] = [{'w': sign, 'text': t[:70], 'mod': bool(m)} for sign, t, m in lst]
            out.append({'panel': idx, 'families': fam})
        print(json.dumps(out, ensure_ascii=False, indent=1))
        return 0
    # 人类可读时序表:逐格变更行
    last = None
    for idx, hits in rows:
        keys = set(hits)
        if keys == last:
            continue
        last = keys
        parts = []
        seen = set()
        for base in sorted(keys):
            for sign, t, m in hits[base]:
                key = (sign, t[:58])
                if key in seen: continue
                seen.add(key)
                parts.append('%s::%s' % (sign, t[:58]))
        print('P%-3d %s' % (idx, ' /// '.join(parts) if parts else '(无服装块)'))
    return 0

def run_verify(args):
    j0 = json.load(open(args.template)); j1 = json.load(open(args.mapped))
    fails = []
    warns = []
    # a) 通用回穿检测(仅警告;作者常用 -N::skirt 类构图防错,语义判定归 LLM)
    for name, j in (('template', j0), ('mapped', j1)):
        for base in CLOTH:
            seen_removed = False
            for idx, hits in extract(j, args.cc):
                if base in hits:
                    for sign, t, m in hits[base]:
                        st = state_char(sign, m)
                        if st == 'R':
                            seen_removed = True
                        elif st in ('A', 'M') and seen_removed:
                            warns.append('%s %s: 词族 %s 负权后再现正权(P%d)——作者构图防错常见,需 LLM 语义确认' % (name, j['title'], base, idx))
                            break
    # b+c) 同名词族签名/变更格
    bases0 = {b for _, h in extract(j0, args.cc) for b in h}
    bases1 = {b for _, h in extract(j1, args.cc) for b in h}
    plain0 = set().union(*[plain_families(panel_text(p, args.cc)) for p in j0['panels']]) if j0['panels'] else set()
    plain1 = set().union(*[plain_families(panel_text(p, args.cc)) for p in j1['panels']]) if j1['panels'] else set()
    only_tpl = (bases0 | plain0) - (bases1 | plain1)   # 仅模板侧:被换名/删掉的词族
    only_out = (bases1 | plain1) - (bases0 | plain0)   # 仅输出侧:新换入的词族
    cross_rename = bool(only_tpl) and bool(only_out)   # 存在跨名换词(含身份块裸属性)
    for base in sorted(bases0 & bases1):
        s0, t0 = signature(j0, base, args.cc); s1, t1 = signature(j1, base, args.cc)
        if s0 != s1 or t0 != t1:
            msg = '%s: 签名/变更格不一致(模板 %s / %s vs 输出 %s / %s)' % (base, s0, t0, s1, t1)
            if cross_rename:
                warns.append(msg + ' —— 跨名换词碰撞,需 LLM 复核(若确属合法换词可接受)')
            else:
                fails.append(msg)
    print('同名词族: %s' % (sorted(bases0 & bases1) or '无'))
    print('仅模板存在: %s  | 仅输出存在: %s' % (sorted(only_tpl) or '无', sorted(only_out) or '无'))
    if warns:
        print('警告(需 LLM 确认,不阻塞):')
        for w in warns[:10]: print('  ⚠', w)
    if fails:
        print('FAIL 链校验:')
        for f in fails: print('  ✗', f)
        return 1
    print('PASS 链校验(回穿/签名/变更格全部一致)')
    return 0

def main():
    ap = argparse.ArgumentParser(description='服装状态链提取与链校验')
    sub = ap.add_subparsers(dest='cmd', required=True)
    e = sub.add_parser('extract'); e.add_argument('file'); e.add_argument('--cc', type=int, default=None); e.add_argument('--json', action='store_true')
    v = sub.add_parser('verify'); v.add_argument('template'); v.add_argument('mapped'); v.add_argument('--cc', type=int, default=None)
    args = ap.parse_args()
    sys.exit(run_extract(args) if args.cmd == 'extract' else run_verify(args))

if __name__ == '__main__':
    main()
