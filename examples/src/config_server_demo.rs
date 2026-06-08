//! Remote config server: serve a Spring Cloud-style document from a throwaway
//! in-process HTTP server, then load it through `ConfigServerProvider` and watch
//! the ordered property sources merge and the dotted keys nest.

use klauthed_core::config::ConfigProvider;
use klauthed_core::config::provider::ConfigServerProvider;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Run the config-server demo against a one-shot local HTTP server.
pub async fn run() {
    // `overrides` has higher precedence than `base`, so its database.port wins;
    // the flat dotted keys nest into a `database` object.
    let body = concat!(
        r#"{"propertySources":["#,
        r#"{"name":"overrides","source":{"database.port":6543}},"#,
        r#"{"name":"base","source":{"database.host":"db.internal","database.port":5432,"app_name":"auth"}}"#,
        r#"]}"#,
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

    let provider =
        ConfigServerProvider::new(format!("http://127.0.0.1:{port}"), "auth-api").profile("prod");
    let config = provider.load().await.unwrap();
    let _ = server.await;

    println!("  loaded from {}", provider.name());
    let database = config.get("database").cloned().unwrap_or_default();
    println!("  database = {database}");
    println!("  app_name = {}", config.get("app_name").cloned().unwrap_or_default());

    // The higher-precedence source overrode the port; both sources' keys nested.
    assert_eq!(database.get("host").and_then(|v| v.as_str()), Some("db.internal"));
    assert_eq!(database.get("port").and_then(|v| v.as_u64()), Some(6543));
    assert_eq!(config.get("app_name").and_then(|v| v.as_str()), Some("auth"));
}
