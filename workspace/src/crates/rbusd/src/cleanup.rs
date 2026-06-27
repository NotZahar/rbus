use rbus_core::constants::MAX_SUBSCRIBERS_PER_STREAM;
use rbus_core::ids::SubscriberId;
use rbus_core::warning::{Warning, WarningCode};
use rbus_ipc::unix_socket::{cleanup_stale_unix_socket, default_socket_path};

use crate::registry::DaemonRegistry;

const COMPONENT_RBUSD: &str = "rbusd";
const REMOVED_RESOURCE_COUNT: u64 = 1;

/// Cleanup result for orphaned daemon resources
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CleanupReport {
    /// Removed stale socket pathnames
    pub socket_paths_removed: u64,
    /// Removed orphaned registry entries
    pub registry_entries_removed: u64,
    /// Pruned empty topics
    pub topics_pruned: u64,
    /// Cleared reader bits for inactive subscriber ids
    pub reader_bits_cleared: u64,
}

impl CleanupReport {
    /// Total removed resources
    pub fn total_removed(self) -> Result<u64, Warning> {
        checked_add(
            checked_add(
                self.socket_paths_removed,
                self.registry_entries_removed,
                "total_a",
            )?,
            checked_add(self.topics_pruned, self.reader_bits_cleared, "total_b")?,
            "total",
        )
    }
}

/// Cleanup orphaned domain resources
pub fn cleanup_domain(
    registry: &mut DaemonRegistry,
    domain: &str,
) -> Result<CleanupReport, Warning> {
    if domain.is_empty() {
        return Err(Warning::new(
            WarningCode::InvalidConfig,
            COMPONENT_RBUSD,
            "cleanup_domain",
        ));
    }
    let mut report = CleanupReport {
        socket_paths_removed: cleanup_stale_unix_socket(&default_socket_path(domain))?,
        ..CleanupReport::default()
    };
    cleanup_orphaned_publishers(registry, &mut report)?;
    cleanup_orphaned_subscribers(registry, &mut report)?;
    cleanup_orphaned_subscriber_mask_bits(registry, &mut report)?;
    cleanup_inactive_reader_bits(registry, &mut report)?;
    cleanup_empty_topics(registry, &mut report)?;
    Ok(report)
}

/// CLI metadata
pub fn format_cleanup_report(
    domain: &str,
    report: CleanupReport,
    json: bool,
) -> Result<String, Warning> {
    if json {
        Ok(format!(
            concat!(
                "{{\"domain\":\"{}\",\"removed\":{},\"socket_paths_removed\":{},",
                "\"registry_entries_removed\":{},\"topics_pruned\":{},",
                "\"reader_bits_cleared\":{}}}"
            ),
            escape_json_string(domain),
            report.total_removed()?,
            report.socket_paths_removed,
            report.registry_entries_removed,
            report.topics_pruned,
            report.reader_bits_cleared
        ))
    } else {
        Ok(format!(
            concat!(
                "domain {}: cleanup removed={} socket_paths_removed={} ",
                "registry_entries_removed={} topics_pruned={} reader_bits_cleared={}"
            ),
            domain,
            report.total_removed()?,
            report.socket_paths_removed,
            report.registry_entries_removed,
            report.topics_pruned,
            report.reader_bits_cleared
        ))
    }
}

fn cleanup_orphaned_publishers(
    registry: &mut DaemonRegistry,
    report: &mut CleanupReport,
) -> Result<(), Warning> {
    let orphaned_publishers: Vec<u64> = registry
        .publishers
        .iter()
        .filter(|(publisher_id, publisher)| {
            !registry
                .topics
                .get(&publisher.topic_hash)
                .is_some_and(|topic| {
                    topic
                        .streams
                        .iter()
                        .any(|stream| stream.metadata.publisher_id == **publisher_id)
                })
        })
        .map(|(publisher_id, _)| *publisher_id)
        .collect();
    for publisher_id in orphaned_publishers {
        registry.publishers.remove(&publisher_id);
        report.registry_entries_removed = checked_add(
            report.registry_entries_removed,
            REMOVED_RESOURCE_COUNT,
            "cleanup_orphaned_publishers",
        )?;
    }
    Ok(())
}

fn cleanup_orphaned_subscribers(
    registry: &mut DaemonRegistry,
    report: &mut CleanupReport,
) -> Result<(), Warning> {
    let orphaned_subscribers: Vec<u64> = registry
        .subscribers
        .iter()
        .filter(|(_, subscriber)| !registry.topics.contains_key(&subscriber.topic_hash))
        .map(|(subscriber_id, _)| *subscriber_id)
        .collect();
    for subscriber_id in orphaned_subscribers {
        registry.subscribers.remove(&subscriber_id);
        clear_active_subscriber_bit(registry, subscriber_id)?;
        report.registry_entries_removed = checked_add(
            report.registry_entries_removed,
            REMOVED_RESOURCE_COUNT,
            "cleanup_orphaned_subscribers",
        )?;
    }
    Ok(())
}

fn cleanup_orphaned_subscriber_mask_bits(
    registry: &mut DaemonRegistry,
    report: &mut CleanupReport,
) -> Result<(), Warning> {
    for subscriber_index in 0..MAX_SUBSCRIBERS_PER_STREAM {
        let subscriber_id = u64::from(subscriber_index);
        let subscriber_bit = subscriber_bit(subscriber_index);
        if registry.active_subscribers_mask & subscriber_bit != 0
            && !registry.subscribers.contains_key(&subscriber_id)
        {
            registry.active_subscribers_mask &= !subscriber_bit;
            report.registry_entries_removed = checked_add(
                report.registry_entries_removed,
                REMOVED_RESOURCE_COUNT,
                "cleanup_orphaned_subscriber_mask_bits",
            )?;
        }
    }
    Ok(())
}

fn cleanup_inactive_reader_bits(
    registry: &DaemonRegistry,
    report: &mut CleanupReport,
) -> Result<(), Warning> {
    for subscriber_index in 0..MAX_SUBSCRIBERS_PER_STREAM {
        if registry.active_subscribers_mask & subscriber_bit(subscriber_index) == 0 {
            let subscriber_id = SubscriberId::new(subscriber_index);
            for topic_entry in registry.topics.values() {
                for stream in &topic_entry.streams {
                    report.reader_bits_cleared = checked_add(
                        report.reader_bits_cleared,
                        stream.clear_reader_bit(subscriber_id)?,
                        "cleanup_inactive_reader_bits",
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn cleanup_empty_topics(
    registry: &mut DaemonRegistry,
    report: &mut CleanupReport,
) -> Result<(), Warning> {
    let before = registry.topics.len() as u64;
    registry
        .topics
        .retain(|_, topic| !topic.streams.is_empty() || topic.subscriber_count != 0);
    report.topics_pruned = before
        .checked_sub(registry.topics.len() as u64)
        .ok_or_else(|| {
            Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, "topics_pruned")
        })?;
    Ok(())
}

fn clear_active_subscriber_bit(
    registry: &mut DaemonRegistry,
    subscriber_id: u64,
) -> Result<(), Warning> {
    let subscriber_index = u8::try_from(subscriber_id).map_err(|_| {
        Warning::new(
            WarningCode::SubscriberLimitReached,
            COMPONENT_RBUSD,
            "clear_active_subscriber_bit",
        )
    })?;
    registry.active_subscribers_mask &= !subscriber_bit(subscriber_index);
    Ok(())
}

fn subscriber_bit(subscriber_index: u8) -> u64 {
    1_u64 << u32::from(subscriber_index)
}

fn checked_add(left: u64, right: u64, field: &'static str) -> Result<u64, Warning> {
    left.checked_add(right)
        .ok_or_else(|| Warning::new(WarningCode::ResourceLimit, COMPONENT_RBUSD, field))
}

fn escape_json_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use crate::registry::DaemonRegistry;

    use super::cleanup_domain;

    const ORPHANED_SUBSCRIBER_MASK: u64 = 1;
    const EXPECTED_REMOVED_REGISTRY_ENTRIES: u64 = 1;

    #[test]
    fn cleanup_removes_orphaned_subscriber_mask_bit() {
        let mut registry = DaemonRegistry::new();
        registry.active_subscribers_mask = ORPHANED_SUBSCRIBER_MASK;
        let domain = format!("cleanup-unit-{}", std::process::id());

        let report = cleanup_domain(&mut registry, &domain).expect("operation should succeed");

        assert_eq!(
            report.registry_entries_removed,
            EXPECTED_REMOVED_REGISTRY_ENTRIES
        );
        assert_eq!(registry.active_subscribers_mask, 0);
    }
}
