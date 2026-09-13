use std::collections::BTreeMap;

use kube::{Api, Client, api::ListParams};

use crate::{
    dto::{JukeboxCategory, PopularityContest},
    error::DiagError,
};
use common::{instanceservice::ServiceInstance, instancesystem::SystemInstance};

/// Get the cluster-wide popularity contest: aggregated package counts
/// per jukebox -> category -> package.
///
/// SECURITY: same rationale as `packages` — only `ServiceInstance` and
/// `SystemInstance` are counted. `TenantInstance` is deliberately excluded.
/// # Errors
///
/// Propagates the [`DiagError`] raised while listing platform instances.
pub async fn get_popularity_contest(client: &Client) -> Result<PopularityContest, DiagError> {
    let mut jukeboxes: BTreeMap<String, BTreeMap<String, BTreeMap<String, u32>>> = BTreeMap::new();

    // Count ServiceInstances
    let service_items = Api::<ServiceInstance>::all(client.clone())
        .list(&ListParams::default())
        .await
        .map_err(DiagError::KubeError)?;

    for instance in &service_items.items {
        let metadata = instance.metadata.clone();
        let spec = instance.spec.clone();

        let jukebox = metadata.name.unwrap_or_default();
        let category = spec.category;
        let package = spec.package;

        let entry = jukeboxes.entry(jukebox).or_default();
        let cats = entry.entry(category).or_default();
        let count = cats.entry(package).or_insert(0);
        *count = count.saturating_add(1);
    }

    // Count SystemInstances
    let system_items = Api::<SystemInstance>::all(client.clone())
        .list(&ListParams::default())
        .await
        .map_err(DiagError::KubeError)?;

    for instance in &system_items.items {
        let metadata = instance.metadata.clone();
        let spec = instance.spec.clone();

        let jukebox = metadata.name.unwrap_or_default();
        let category = spec.category;
        let package = spec.package;

        let entry = jukeboxes.entry(jukebox).or_default();
        let cats = entry.entry(category).or_default();
        let count = cats.entry(package).or_insert(0);
        *count = count.saturating_add(1);
    }

    // Wrap in JukeboxCategory structs
    let jukeboxes_jc: BTreeMap<String, JukeboxCategory> = jukeboxes
        .into_iter()
        .map(|(name, categories)| (name, JukeboxCategory { categories }))
        .collect();

    Ok(PopularityContest {
        jukeboxes: jukeboxes_jc,
    })
}
