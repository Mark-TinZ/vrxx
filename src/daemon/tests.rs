use crate::daemon::network::TunManager;
use crate::daemon::dns::DnsManager;

#[tokio::test]
#[ignore = "Requires root privileges"]
async fn test_tun_creation() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();
    assert!(tun_mgr.if_index > 0);
    tun_mgr.teardown().await.unwrap();
}

#[tokio::test]
#[ignore = "Requires root privileges"]
async fn test_routing_rules() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();
    tun_mgr.teardown().await.unwrap();
}

#[tokio::test]
#[ignore = "Requires root privileges and systemd-resolved"]
async fn test_dns_protection() {
    let mut tun_mgr = TunManager::new().await.unwrap();
    tun_mgr.setup().await.unwrap();

    let dns_mgr = DnsManager::new().await.unwrap();
    dns_mgr.set_dns(tun_mgr.if_index as i32, vec!["172.19.0.1".to_string()]).await.unwrap();
    dns_mgr.reset_dns(tun_mgr.if_index as i32).await.unwrap();

    tun_mgr.teardown().await.unwrap();
}