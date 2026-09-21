use graphite_core::{ConnectionConfig, DatabaseClient};
use graphite_drivers::open_client;

fn docker_on() -> bool {
    std::env::var("GRAPHITE_DOCKER").ok().as_deref() == Some("1")
}

async fn expect_connect(config: ConnectionConfig) {
    let mut client = open_client(config.clone()).await.expect("open");
    match client.connect().await {
        Ok(()) => {}
        Err(err) if err.to_string().contains("Connection refused") => {
            eprintln!("skip: engine not listening ({err})");
            return;
        }
        Err(err) => panic!("connect: {err}"),
    }
    let version = client.version_string().await.expect("version");
    assert!(!version.is_empty());
    let _ = client.list_tables().await;
}

#[tokio::test]
async fn postgres_local_select_1() {
    let mut client = open_client(ConnectionConfig {
        connection_type: "postgresql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        user: Some("postgres".into()),
        password: Some("postgres".into()),
        default_database: Some("allu".into()),
        ..Default::default()
    })
    .await
    .expect("open");
    match client.connect().await {
        Ok(()) => {}
        Err(err) if err.to_string().contains("Connection refused") => return,
        Err(err) => panic!("connect: {err}"),
    }
    let results = client
        .execute_query("select 1 as n")
        .await
        .expect("query");
    let first = results.first().expect("result");
    assert_eq!(first.columns, vec!["n"]);
    assert_eq!(first.rows[0][0], serde_json::json!("1"));
}

#[tokio::test]
#[ignore]
async fn postgres_docker() {
    if !docker_on() {
        return;
    }
    expect_connect(ConnectionConfig {
        connection_type: "postgresql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(55432),
        user: Some("graphite".into()),
        password: Some("graphite".into()),
        default_database: Some("graphite".into()),
        ..Default::default()
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn mysql_docker() {
    if !docker_on() {
        return;
    }
    expect_connect(ConnectionConfig {
        connection_type: "mysql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(53306),
        user: Some("graphite".into()),
        password: Some("graphite".into()),
        default_database: Some("graphite".into()),
        ..Default::default()
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn redis_docker() {
    if !docker_on() {
        return;
    }
    expect_connect(ConnectionConfig {
        connection_type: "redis".into(),
        host: Some("127.0.0.1".into()),
        port: Some(56379),
        ..Default::default()
    })
    .await;
}

#[tokio::test]
#[ignore]
async fn sqlserver_docker() {
    if !docker_on() {
        return;
    }
    expect_connect(ConnectionConfig {
        connection_type: "sqlserver".into(),
        host: Some("127.0.0.1".into()),
        port: Some(51433),
        user: Some("sa".into()),
        password: Some("Graphite_123".into()),
        default_database: Some("master".into()),
        ..Default::default()
    })
    .await;
}

#[tokio::test]
async fn cockroach_redshift_greengage_use_postgres_dialect() {
    for kind in ["cockroachdb", "redshift", "greengage"] {
        let err = open_client(ConnectionConfig {
            connection_type: kind.into(),
            host: Some("127.0.0.1".into()),
            port: Some(1),
            ..Default::default()
        })
        .await;
        assert!(err.is_ok(), "{kind} should open postgres-family client");
    }
}

#[tokio::test]
async fn mysql_family_types_open() {
    for kind in ["mariadb", "tidb", "starrocks", "bedrock"] {
        assert!(open_client(ConnectionConfig {
            connection_type: kind.into(),
            host: Some("127.0.0.1".into()),
            port: Some(1),
            ..Default::default()
        })
        .await
        .is_ok());
    }
}

#[allow(dead_code)]
fn _client_trait(_: &dyn DatabaseClient) {}
