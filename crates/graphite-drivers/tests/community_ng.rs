use graphite_core::{ConnectionConfig, DatabaseClient, NgQueryResult};
use graphite_drivers::open_client;

fn local_allu() -> ConnectionConfig {
    ConnectionConfig {
        connection_type: "postgresql".into(),
        host: Some("127.0.0.1".into()),
        port: Some(5432),
        user: Some("postgres".into()),
        password: Some("postgres".into()),
        default_database: Some("allu".into()),
        ssl: false,
        ..Default::default()
    }
}

#[tokio::test]
async fn local_allu_limit_100_returns_community_ng_objects() {
    let mut client = open_client(local_allu()).await.expect("open");
    client.connect().await.expect("connect local allu");
    let raw = client
        .execute_query("select * from public.migrations limit 100")
        .await
        .expect("query");
    let ng = NgQueryResult::from(raw.into_iter().next().unwrap());
    assert!(!ng.fields.is_empty());
    assert!(ng.row_count > 0);
    assert!(ng.rows[0].is_object());
    assert_eq!(ng.command.as_deref(), Some("SELECT"));
}

#[tokio::test]
async fn local_allu_empty_table_keeps_fields() {
    let mut client = open_client(local_allu()).await.expect("open");
    client.connect().await.expect("connect local allu");
    let raw = client
        .execute_query("select * from public.migrations where false limit 100")
        .await
        .expect("query");
    let ng = NgQueryResult::from(raw.into_iter().next().unwrap());
    assert!(!ng.fields.is_empty());
    assert_eq!(ng.row_count, 0);
    assert!(ng.rows.is_empty());
}
