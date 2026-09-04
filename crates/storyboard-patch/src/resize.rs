/// Panel resize (template-mutation §6): stage-proportional sampling when
/// compressing, adjacent duplication when expanding. Never invents new
/// prompts — every output panel's prompt is copied verbatim from a source
/// panel.

pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

pub fn uuid_from_rng(rng: &mut SplitMix64) -> uuid::Uuid {
    let mut b = [0u8; 16];
    b[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
    b[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(b)
}

/// Resize the `panels` array in `draft` to `target` panels.
///
/// Compress: pick evenly spaced source panels (always keeps first and last).
/// Expand: duplicate adjacent panels with fresh ids/seeds. Output indexes are
/// renumbered 1..=target in both cases.
pub fn resize_panels(draft: &mut serde_json::Value, target: u32, seed: u64) {
    let Some(panels) = draft.get_mut("panels").and_then(|p| p.as_array_mut()) else {
        return;
    };
    let len = panels.len() as u32;
    if target == len {
        return;
    }
    let source: Vec<serde_json::Value> = if target < len {
        // even sampling: keep first + last
        let mut picked = Vec::new();
        for i in 0..target {
            let src = ((i as u64 * (len as u64 - 1)) / (target as u64 - 1).max(1)) as u32;
            picked.push(panels[src.min(len - 1) as usize].clone());
        }
        picked
    } else {
        // duplicate adjacent
        let mut picked = Vec::new();
        for i in 0..target {
            let src = ((i as u64 * len as u64) / target as u64) as u32;
            picked.push(panels[src.min(len - 1) as usize].clone());
        }
        picked
    };
    let mut rng = SplitMix64::new(seed);
    let mut out = Vec::with_capacity(target as usize);
    for (i, mut p) in source.into_iter().enumerate() {
        p["index"] = serde_json::Value::Number((i + 1).into());
        p["id"] = serde_json::Value::String(uuid_from_rng(&mut rng).to_string());
        let s = rng.next_u64() % 4_000_000_000;
        p["paramsOverride"]["params"]["seed"] = serde_json::Value::Number(s.into());
        out.push(p);
    }
    *panels = out;
}
