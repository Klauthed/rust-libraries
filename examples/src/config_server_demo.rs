//! Remote config server: serve a klauthed-native `ConfigDocument` from a
//! throwaway in-process HTTP server, then load it through `ConfigServerProvider`
//! (default `Klauthed` format) and read the nested config tree.
//!
//! (A real deployment would run a service as the server via
//! `klauthed_web::config_server::ConfigServer`; here we hand-roll the response
//! so the example stays dependency-light.)

use klauthed_core::config::ConfigProvider;
use klauthed_core::config::provider::ConfigServerProvider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Run the config-server demo against a one-shot local HTTP server.
pub async fn run() {
    // The native contract: a ConfigDocument whose `config` is the nested tree.
    let body = concat!(
        r#"{"application":"auth-api","profile":"prod","config":{"#,
        r#""database":{"host":"db.internal","port":6543},"app_name":"auth""#,
        r#"}}"#,
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        if let Ok((mut socket, _)) = listener.accept().await {
            // Read (and ignore) the request, then answer with the document.
            let mut scratch = [0u8; 1024];
            let _ = socket.read(&mut scratch).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.flush().await;
        }
    });

    // Default format is Klauthed (native) — pairs with our own config server.
    let provider =
        ConfigServerProvider::new(format!("http://127.0.0.1:{port}"), "auth-api").profile("prod");
    let config = provider.load().await.unwrap();
    let _ = server.await;

    println!("  loaded from {}", provider.name());
    let database = config.get("database").cloned().unwrap_or_default();
    println!("  database = {database}");
    println!("  app_name = {}", config.get("app_name").cloned().unwrap_or_default());

    // The native `config` tree was extracted verbatim.
    assert_eq!(database.get("host").and_then(|v| v.as_str()), Some("db.internal"));
    assert_eq!(database.get("port").and_then(|v| v.as_u64()), Some(6543));
    assert_eq!(config.get("app_name").and_then(|v| v.as_str()), Some("auth"));
}
