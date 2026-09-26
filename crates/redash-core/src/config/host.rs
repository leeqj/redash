pub use redash_types::host::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_host_config_new_defaults() {
        let host = HostConfig::new("test-host", "192.168.1.1", "root");
        assert_eq!(host.name, "test-host");
        assert_eq!(host.hostname, "192.168.1.1");
        assert_eq!(host.port, 22);
        assert_eq!(host.user, "root");
        assert_eq!(host.group, "Default");
        assert_eq!(host.auth, AuthMethod::Agent);
        assert!(host.tags.is_empty());
        assert_eq!(host.target_os, TargetOs::Linux);
        assert_eq!(host.jump_host, None);
        assert_eq!(host.proxy_jump_id, None);
        assert_eq!(host.bandwidth_limit_gb, None);
        assert_eq!(host.bandwidth_reset_day, None);
    }

    #[test]
    fn test_host_config_serde_with_proxy_jump_id() {
        let mut host = HostConfig::new("jumped-host", "10.0.0.5", "admin");
        let proxy_id = HostId("proxy_bastion_99".to_string());
        host.proxy_jump_id = Some(proxy_id.clone());

        let serialized = serde_json::to_string(&host).expect("Serialization failed");
        assert!(serialized.contains("proxy_jump_id"));
        assert!(serialized.contains("proxy_bastion_99"));

        let deserialized: HostConfig =
            serde_json::from_str(&serialized).expect("Deserialization failed");
        assert_eq!(deserialized.proxy_jump_id, Some(proxy_id));
        assert_eq!(deserialized.proxy_id(), Some(&HostId("proxy_bastion_99".to_string())));
    }

    #[test]
    fn test_host_config_deserialization_without_proxy_jump_id() {
        let json_legacy = r#"{
            "id": "legacy_host_01",
            "name": "Legacy Server",
            "hostname": "192.168.1.50",
            "port": 22,
            "user": "root"
        }"#;

        let host: HostConfig =
            serde_json::from_str(json_legacy).expect("Legacy JSON deserialization failed");
        assert_eq!(host.name, "Legacy Server");
        assert_eq!(host.proxy_jump_id, None);
        assert_eq!(host.bandwidth_limit_gb, None);
        assert_eq!(host.bandwidth_reset_day, None);
    }
}
