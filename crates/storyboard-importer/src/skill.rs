use storyboard_domain::SceneAliasTable;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum SkillBundleError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("missing entry: {0}")]
    MissingEntry(String),
    #[error("bad json in {path}: {source}")]
    BadJson { path: String, source: serde_json::Error },
}

/// Access to the frozen `.skill` migration fixture (plan §30). The bundle is
/// read-only for the lifetime of the program: P0-01 extraction only ever
/// copies data out of it.
pub struct SkillBundle {
    inner: BundleInner,
}

enum BundleInner {
    Directory(PathBuf),
    Zip { path: PathBuf, cache: std::sync::Mutex<BTreeMap<String, Vec<u8>>> },
}

impl SkillBundle {
    /// Open an unpacked skill directory (`fixtures/current-skill`).
    pub fn open_dir(path: impl AsRef<Path>) -> Result<Self, SkillBundleError> {
        let p = path.as_ref();
        if !p.join("references/template-index.json").is_file() {
            return Err(SkillBundleError::MissingEntry(format!(
                "{} is not a skill directory (references/template-index.json missing)",
                p.display()
            )));
        }
        Ok(Self { inner: BundleInner::Directory(p.to_path_buf()) })
    }

    /// Open a packed `.skill` file (zip archive).
    pub fn open_zip(path: impl AsRef<Path>) -> Result<Self, SkillBundleError> {
        // Validate eagerly: index must exist inside.
        let f = std::fs::File::open(path.as_ref())?;
        let mut z = zip::ZipArchive::new(f)?;
        if z.by_name("references/template-index.json").is_err() {
            return Err(SkillBundleError::MissingEntry(
                "references/template-index.json not found inside .skill archive".into(),
            ));
        }
        Ok(Self {
            inner: BundleInner::Zip {
                path: path.as_ref().to_path_buf(),
                cache: std::sync::Mutex::new(BTreeMap::new()),
            },
        })
    }

    fn read_entry(&self, rel: &str) -> Result<Vec<u8>, SkillBundleError> {
        match &self.inner {
            BundleInner::Directory(root) => {
                let p = root.join(rel);
                std::fs::read(&p).map_err(|_| SkillBundleError::MissingEntry(rel.to_string()))
            }
            BundleInner::Zip { path, cache } => {
                if let Some(bytes) = cache.lock().unwrap().get(rel) {
                    return Ok(bytes.clone());
                }
                let f = std::fs::File::open(path)?;
                let mut z = zip::ZipArchive::new(f)?;
                let mut entry = z
                    .by_name(rel)
                    .map_err(|_| SkillBundleError::MissingEntry(rel.to_string()))?;
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                cache.lock().unwrap().insert(rel.to_string(), buf.clone());
                Ok(buf)
            }
        }
    }

    pub fn read_template(&self, template_id: &str) -> Result<Vec<u8>, SkillBundleError> {
        let n: u32 = template_id
            .trim_start_matches('T')
            .parse()
            .map_err(|_| SkillBundleError::MissingEntry(format!("bad template id {template_id}")))?;
        let rel = format!("references/template-library/template_{n:03}.json");
        self.read_entry(&rel)
    }

    pub fn legacy_index_json(&self) -> Result<serde_json::Value, SkillBundleError> {
        let bytes = self.read_entry("references/template-index.json")?;
        serde_json::from_slice(&bytes).map_err(|source| SkillBundleError::BadJson {
            path: "template-index.json".into(),
            source,
        })
    }

    /// Parsed legacy index entries (`templates` array).
    pub fn legacy_index(&self) -> Result<Vec<IndexEntry>, SkillBundleError> {
        let v = self.legacy_index_json()?;
        let entries = v
            .get("templates")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            out.push(serde_json::from_value(e).map_err(|source| SkillBundleError::BadJson {
                path: "template-index.json[templates]".into(),
                source,
            })?);
        }
        Ok(out)
    }

    /// Scene alias table loaded from the bundle at runtime — never hard-coded
    /// (template-selection.md §1.2).
    pub fn alias_table(&self) -> Result<SceneAliasTable, SkillBundleError> {
        let v = self.legacy_index_json()?;
        let mut pairs: BTreeMap<String, Vec<String>> = BTreeMap::new();
        if let Some(map) = v.get("scene_aliases").and_then(|s| s.as_object()) {
            for (family, aliases) in map {
                let list = aliases
                    .as_array()
                    .map(|a| {
                        a.iter().filter_map(|x| x.as_str().map(String::from)).collect()
                    })
                    .unwrap_or_default();
                pairs.insert(family.clone(), list);
            }
        }
        Ok(SceneAliasTable::from_pairs(pairs))
    }
}

/// Verbatim (lenient) view over one legacy index entry. Unknown fields are
/// ignored on purpose — the entry is also stored raw for audit.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexEntry {
    pub template_id: String,
    pub source_file: String,
    pub title: String,
    pub scene_family: String,
    #[serde(default)]
    pub exact_scene: Option<String>,
    #[serde(default)]
    pub scene: Vec<String>,
    #[serde(default)]
    pub location: Vec<String>,
    #[serde(default)]
    pub time: Vec<String>,
    #[serde(default)]
    pub environment: Vec<String>,
    #[serde(default)]
    pub character_count: u32,
    #[serde(default)]
    pub female_character_count: u32,
    #[serde(default)]
    pub male_character_count: u32,
    #[serde(default)]
    pub female_anchors: Vec<String>,
    #[serde(default)]
    pub male_identity: Option<String>,
    #[serde(default)]
    pub male_panel_ratio: Option<f32>,
    #[serde(default)]
    pub panel_count: u32,
    #[serde(default)]
    pub narrative_type: Option<String>,
    #[serde(default)]
    pub opening_type: Option<String>,
    #[serde(default)]
    pub ending_type: Option<String>,
    #[serde(default)]
    pub pace: Option<String>,
    #[serde(default)]
    pub first_sex_panel: Option<u32>,
    #[serde(default)]
    pub pov_ratio: Option<f32>,
    #[serde(default)]
    pub torogao_coverage: Option<f32>,
    #[serde(default)]
    pub camera_profile: Vec<String>,
    #[serde(default)]
    pub camera_profile_freq: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub composition_profile: Vec<String>,
    #[serde(default)]
    pub clothing_arc: Option<String>,
    #[serde(default)]
    pub interaction_profile: Vec<String>,
    #[serde(default)]
    pub important_props: Vec<String>,
    #[serde(default)]
    pub aspect_ratio_counts: std::collections::BTreeMap<String, u32>,
    #[serde(default)]
    pub keywords: Vec<String>,
}
