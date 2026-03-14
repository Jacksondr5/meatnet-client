use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ble::transport::{AdvertisementFamily, DiscoveryEvent};
use crate::types::ProductType;

const CACHE_MAX_ENTRIES: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedDiscovery {
    pub peripheral_handle: String,
    pub advertisement_family: AdvertisementFamily,
    pub product_type: ProductType,
    pub serial_number: String,
    pub seen_at_ms: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct DiscoveryCache {
    entries: Vec<CachedDiscovery>,
}

pub fn record_discoveries(discoveries: &[DiscoveryEvent]) -> Result<()> {
    let path = cache_file_path()?;
    let mut cache = load_cache(&path)?;
    let seen_at_ms = now_ms();

    for discovery in discoveries {
        cache.entries.retain(|entry| {
            !(entry.peripheral_handle == discovery.peripheral_handle
                && entry.advertisement_family == discovery.advertisement_family
                && entry.product_type == discovery.product_type
                && entry.serial_number == discovery.serial_number)
        });

        cache.entries.push(CachedDiscovery {
            peripheral_handle: discovery.peripheral_handle.clone(),
            advertisement_family: discovery.advertisement_family,
            product_type: discovery.product_type,
            serial_number: discovery.serial_number.clone(),
            seen_at_ms,
        });
    }

    cache.entries.sort_by_key(|entry| entry.seen_at_ms);
    if cache.entries.len() > CACHE_MAX_ENTRIES {
        let start = cache.entries.len() - CACHE_MAX_ENTRIES;
        cache.entries = cache.entries.split_off(start);
    }

    save_cache(&path, &cache)
}

pub fn load_recent_target(
    product_type: ProductType,
    serial_number: &str,
    max_age: Duration,
) -> Result<Option<CachedDiscovery>> {
    let path = cache_file_path()?;
    let cache = load_cache(&path)?;
    let cutoff = now_ms().saturating_sub(max_age.as_millis() as u64);

    Ok(cache.entries.into_iter().rev().find(|entry| {
        entry.seen_at_ms >= cutoff
            && entry.advertisement_family == AdvertisementFamily::NodeSelf
            && entry.product_type == product_type
            && entry.serial_number == serial_number
    }))
}

fn cache_file_path() -> Result<PathBuf> {
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(".cache")
        .join("sbc-service");
    fs::create_dir_all(&base).with_context(|| format!("failed to create {}", base.display()))?;
    Ok(base.join("discoveries.json"))
}

fn load_cache(path: &PathBuf) -> Result<DiscoveryCache> {
    if !path.exists() {
        return Ok(DiscoveryCache::default());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read discovery cache {}", path.display()))?;
    let cache = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse discovery cache {}; remove the file if it is corrupted",
            path.display()
        )
    })?;
    Ok(cache)
}

fn save_cache(path: &PathBuf, cache: &DiscoveryCache) -> Result<()> {
    let raw = serde_json::to_string_pretty(cache).context("failed to serialize discovery cache")?;
    fs::write(path, raw)
        .with_context(|| format!("failed to write discovery cache {}", path.display()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_target_prefers_latest_matching_node_self_entry() {
        let cache = DiscoveryCache {
            entries: vec![
                CachedDiscovery {
                    peripheral_handle: "old".to_string(),
                    advertisement_family: AdvertisementFamily::NodeSelf,
                    product_type: ProductType::Display,
                    serial_number: "T1000006XS".to_string(),
                    seen_at_ms: 10,
                },
                CachedDiscovery {
                    peripheral_handle: "new".to_string(),
                    advertisement_family: AdvertisementFamily::NodeSelf,
                    product_type: ProductType::Display,
                    serial_number: "T1000006XS".to_string(),
                    seen_at_ms: 20,
                },
            ],
        };

        let found = cache
            .entries
            .into_iter()
            .rev()
            .find(|entry| {
                entry.product_type == ProductType::Display
                    && entry.serial_number == "T1000006XS"
                    && entry.advertisement_family == AdvertisementFamily::NodeSelf
            })
            .expect("matching cache entry");

        assert_eq!(found.peripheral_handle, "new");
    }
}
