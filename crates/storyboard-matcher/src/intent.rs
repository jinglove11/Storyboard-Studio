use storyboard_domain::{QueryIntent, SceneAliasTable};

/// Rule-based intent parser (P2-01). Chinese + English keyword rules,
/// deterministic. The agent runtime may refine the result later; this is the
/// program-side floor that always runs first.
pub fn parse_intent(input: &str, aliases: &SceneAliasTable) -> QueryIntent {
    let lower = input.to_lowercase();
    let mut q = QueryIntent { keywords: Vec::new(), ..Default::default() };

    // --- location via alias table (never hard-coded) ---
    for (family, list) in &aliases.families {
        for a in list {
            let a_l = a.to_lowercase();
            if a_l.len() >= 2 && lower.contains(&a_l) {
                if q.scene_family.is_none() {
                    q.scene_family = Some(family.clone());
                    q.keywords.push(a.clone());
                }
                break;
            }
        }
    }

    // --- time ---
    for (pat, t) in [
        ("夜", "night"), ("night", "night"), ("深夜", "night"),
        ("白天", "day"), ("day", "day"), ("daytime", "day"), ("昼", "day"),
        ("黄昏", "sunset"), ("傍晚", "sunset"), ("sunset", "sunset"), ("夕", "sunset"),
        ("早晨", "morning"), ("清晨", "morning"), ("morning", "morning"),
        ("雨天", "rain"), ("rain", "rain"), ("雨", "rain"),
    ] {
        if lower.contains(pat) {
            q.time = Some(t.into());
            break;
        }
    }

    // --- character count: `1女2男` / `两男` / `3人` / `双角色` ---
    let cn_num = |s: &str| -> Option<u32> {
        Some(match s {
            "一" => 1, "二" | "两" => 2, "三" => 3, "四" => 4, "五" => 5, "六" => 6, _ => return None,
        })
    };
    let mut female = 0u32;
    let mut male = 0u32;
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if !(c.is_ascii_digit() || "一二两三四五六".contains(c)) {
            i += 1;
            continue;
        }
        let n: u32 = if c.is_ascii_digit() {
            let mut n = 0u32;
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                n = n * 10 + (chars[j] as u32 - '0' as u32);
                j += 1;
            }
            i = j - 1;
            n
        } else {
            cn_num(&c.to_string()).unwrap_or(0)
        };
        // skip spaces, then classify by the word right after the number
        let mut k = i + 1;
        while k < chars.len() && chars[k] == ' ' {
            k += 1;
        }
        let word: String = chars[k..(k + 6).min(chars.len())].iter().collect();
        if word.starts_with('女')
            || word.starts_with("girl")
            || word.starts_with("woman")
            || word.starts_with("women")
            || word.starts_with("female")
        {
            female += n;
        } else if word.starts_with('男')
            || word.starts_with("man")
            || word.starts_with("men")
            || word.starts_with("boy")
            || word.starts_with("boys")
            || word.starts_with("male")
        {
            male += n;
        }
        i += 1;
    }
    if female + male > 0 {
        q.character_count = Some(female + male);
        if female > 0 {
            q.character_roles.push(if female > 1 { "multiple_females".into() } else { "female_lead".into() });
        }
        if male > 0 {
            q.character_roles.push(if male > 1 { "multiple_males".into() } else { "single_male".into() });
        }
    } else {
        for (pat, n) in [("单人", 1u32), ("1人", 1), ("两人", 2), ("2人", 2), ("双人", 2), ("三人", 3)] {
            if lower.contains(pat) {
                q.character_count = Some(n);
                break;
            }
        }
    }

    // --- roles ---
    for (pat, role) in [
        ("教师", "teacher"), ("teacher", "teacher"), ("老师", "teacher"),
        ("出租车", "taxi driver"), ("taxi", "taxi driver"), ("司机", "driver"),
        ("匿名", "anonymous"), ("faceless", "anonymous"), ("怪男", "anonymous"),
        ("痴汉", "groper"), ("痴漢", "groper"), ("groper", "groper"),
        ("同学", "classmate"), ("classmate", "classmate"),
        ("上司", "boss"), ("boss", "boss"), ("店长", "manager"),
    ] {
        if lower.contains(pat) && !q.character_roles.iter().any(|r| r == role) {
            q.character_roles.push(role.into());
        }
    }

    // --- narrative tags ---
    for (pat, tag) in [
        ("强暴", "rape"), ("rape", "rape"), ("侵犯", "rape"),
        ("睡眠", "sleep"), ("sleep", "sleep"), ("睡奸", "sleep"),
        ("痴汉", "groping"), ("痴漢", "groping"), ("groping", "groping"), ("痴姦", "groping"),
        ("把柄", "blackmail"), ("blackmail", "blackmail"), ("胁迫", "blackmail"),
        ("泥醉", "drunk"), ("泥酔", "drunk"), ("醉酒", "drunk"), ("drunk", "drunk"),
        ("囚禁", "captive"), ("监禁", "captive"), ("captive", "captive"),
        ("群交", "group"), ("gangbang", "group"),
    ] {
        if lower.contains(pat) && !q.narrative_tags.iter().any(|t| t == tag) {
            q.narrative_tags.push(tag.into());
        }
    }

    // --- panel count: `80格` / `80 panels` ---
    let mut chars = lower.char_indices().peekable();
    while let Some((idx, c)) = chars.next() {
        if c.is_ascii_digit() {
            let start = idx;
            let end = {
                let mut e = idx + c.len_utf8();
                while let Some(&(j, cj)) = chars.peek() {
                    if cj.is_ascii_digit() {
                        e = j + cj.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                e
            };
            let num: u32 = lower[start..end].parse().unwrap_or(0);
            let after = &lower[end..];
            if after.starts_with("格") || after.starts_with("张") || after.starts_with(" panels") || after.starts_with(" panels,") {
                q.desired_panel_count = Some(num);
            }
        }
    }

    // --- pace ---
    for (pat, p) in [("快节奏", "fast"), ("fast", "fast"), ("慢节奏", "slow"), ("slow", "slow")] {
        if lower.contains(pat) {
            q.pace_hint = Some(p.into());
            break;
        }
    }

    // --- camera hints ---
    for cam in ["pov", "dutch angle", "cowboy shot", "close-up", "closeup", "from above", "from side", "wide shot"] {
        if lower.contains(cam) {
            q.camera_hints.push(cam.into());
        }
    }

    // --- props (small deterministic set) ---
    for (pat, prop) in [
        ("篝火", "campfire"), ("campfire", "campfire"),
        ("纸板", "cardboard"), ("cardboard", "cardboard"),
        ("长椅", "bench"), ("bench", "bench"),
        ("浴室", "bathroom"), ("bath", "bathroom"),
        ("浴巾", "towel"), ("towel", "towel"),
        ("裤袜", "pantyhose"), ("pantyhose", "pantyhose"),
        ("制服", "uniform"), ("uniform", "uniform"),
        ("泳装", "swimsuit"), ("swimsuit", "swimsuit"),
    ] {
        if lower.contains(pat) {
            q.props.push(prop.into());
        }
    }

    q
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn aliases() -> SceneAliasTable {
        SceneAliasTable::from_pairs(BTreeMap::from([
            ("park".into(), vec!["park".into(), "公园".into(), "night park".into()]),
            ("office".into(), vec!["office".into(), "办公室".into()]),
            ("school".into(), vec!["school".into(), "学校".into(), "教室".into()]),
            ("vehicle".into(), vec!["taxi".into(), "出租车".into()]),
        ]))
    }

    #[test]
    fn parses_chinese_location_time_roles() {
        let q = parse_intent("夜间公园里 1女 被 匿名男 强暴,80格", &aliases());
        assert_eq!(q.scene_family.as_deref(), Some("park"));
        assert_eq!(q.time.as_deref(), Some("night"));
        assert_eq!(q.character_count, Some(1));
        assert!(q.character_roles.iter().any(|r| r == "anonymous"));
        assert!(q.narrative_tags.iter().any(|t| t == "rape"));
        assert_eq!(q.desired_panel_count, Some(80));
    }

    #[test]
    fn parses_english() {
        let q = parse_intent("office scene, 1 girl 1 man, blackmail", &aliases());
        assert_eq!(q.scene_family.as_deref(), Some("office"));
        assert_eq!(q.character_count, Some(2));
        assert!(q.narrative_tags.iter().any(|t| t == "blackmail"));
    }
}
