use zero_discord::DiscordClient;
use zero_network::ConnectionState;

#[tokio::test]
async fn live_discord_api_gateway_endpoint_responds() {
    let client = DiscordClient::new(None).expect("create DiscordClient");
    let info = client
        .get_gateway()
        .await
        .expect("GET https://discord.com/api/v10/gateway should succeed");

    assert!(
        info.url.starts_with("wss://"),
        "unexpected gateway url: {}",
        info.url
    );

    let diag = client.diagnostics();
    assert_eq!(diag.state, ConnectionState::Connected);
    assert_eq!(diag.successful_requests, 1);
    assert!(diag.last_latency_ms.is_some());
}
