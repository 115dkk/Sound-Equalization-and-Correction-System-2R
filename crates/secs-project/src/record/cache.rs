use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct CacheKey {
    path: PathBuf,
    modified_ns: u128,
    size: u64,
}

#[derive(Debug, Clone)]
pub(super) struct CacheValue {
    samples: Vec<f32>,
    sample_rate: u32,
}

#[derive(Debug)]
pub struct IrCache {
    entries: BTreeMap<CacheKey, CacheValue>,
    order: VecDeque<CacheKey>,
    bytes: usize,
    cap: usize,
}

impl Default for IrCache {
    fn default() -> Self {
        Self::new(IR_CACHE_LIMIT_BYTES)
    }
}

impl IrCache {
    pub fn new(cap: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            cap,
        }
    }

    pub fn load(&mut self, path: &Path) -> Result<(Vec<f32>, u32), ProjectError> {
        let key = cache_key(path)?;
        self.remove_old_versions(&key.path, Some(&key));
        if let Some(value) = self.entries.get(&key) {
            let hit = (value.samples.clone(), value.sample_rate);
            if let Some(index) = self.order.iter().position(|old_key| old_key == &key) {
                let touched = self.order.remove(index).expect("index from position");
                self.order.push_back(touched);
            }
            return Ok(hit);
        }
        let (samples, sample_rate) =
            read_mono_float_wav(&crate::irs::read_bounded(path, MAX_IR_BYTES, "WAV")?)
                .map_err(|error| ProjectError::Wav(format!("{}: {error}", path.display())))?;
        self.insert(key, &samples, sample_rate);
        Ok((samples, sample_rate))
    }

    pub fn remember(
        &mut self,
        path: &Path,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<(), ProjectError> {
        let key = cache_key(path)?;
        self.remove_old_versions(&key.path, Some(&key));
        self.insert(key, samples, sample_rate);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sample bytes the cache holds; never more than its cap.
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }

    /// Stores a copy unless it alone exceeds the cap, then evicts from the
    /// least recently used end until the cache fits.
    fn insert(&mut self, key: CacheKey, samples: &[f32], sample_rate: u32) {
        let size = samples.len().saturating_mul(std::mem::size_of::<f32>());
        if let Some(old) = self.entries.remove(&key) {
            self.bytes = self
                .bytes
                .saturating_sub(old.samples.len().saturating_mul(4));
            self.order.retain(|old_key| old_key != &key);
        }
        if size > self.cap {
            return;
        }
        self.bytes = self.bytes.saturating_add(size);
        self.order.push_back(key.clone());
        self.entries.insert(
            key,
            CacheValue {
                samples: samples.to_vec(),
                sample_rate,
            },
        );
        while self.bytes > self.cap
            && let Some(old_key) = self.order.pop_front()
        {
            if let Some(old) = self.entries.remove(&old_key) {
                self.bytes = self
                    .bytes
                    .saturating_sub(old.samples.len().saturating_mul(4));
            }
        }
    }

    fn remove_old_versions(&mut self, path: &Path, keep: Option<&CacheKey>) {
        let old = self
            .entries
            .keys()
            .filter(|key| key.path == path && keep != Some(*key))
            .cloned()
            .collect::<Vec<_>>();
        for key in old {
            if let Some(value) = self.entries.remove(&key) {
                self.bytes = self
                    .bytes
                    .saturating_sub(value.samples.len().saturating_mul(4));
            }
            self.order.retain(|old_key| old_key != &key);
        }
    }
}

pub(super) fn cache_key(path: &Path) -> Result<CacheKey, ProjectError> {
    let metadata = fs::metadata(path)?;
    let modified_ns = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Ok(CacheKey {
        path: path.canonicalize()?,
        modified_ns,
        size: metadata.len(),
    })
}

