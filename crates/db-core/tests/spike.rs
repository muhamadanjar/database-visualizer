//! Driver feasibility gate. Run explicitly against disposable Compose databases.
use sqlparser::{
    dialect::{MySqlDialect, PostgreSqlDialect},
    parser::Parser,
};
use sqlx::{Connection, Executor, Row, mysql::MySqlConnectOptions, postgres::PgConnectOptions};
use std::time::Duration;

#[test]
fn parser_handles_literal_semicolons_and_call() {
    for dialect in [
        &PostgreSqlDialect {} as &dyn sqlparser::dialect::Dialect,
        &MySqlDialect {},
    ] {
        assert_eq!(
            Parser::parse_sql(dialect, "SELECT ';' AS value")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            Parser::parse_sql(dialect, "SELECT 1; SELECT 2")
                .unwrap()
                .len(),
            2
        );
        assert!(matches!(
            Parser::parse_sql(dialect, "CALL dangerous()").unwrap()[0],
            sqlparser::ast::Statement::Call(_)
        ));
    }
    assert!(Parser::parse_sql(&PostgreSqlDialect {}, "DO $$ BEGIN PERFORM 1; END $$").is_err());
}

#[tokio::test]
#[ignore = "requires disposable Docker PostgreSQL"]
async fn postgres_session_cancel_recovery() {
    let options = PgConnectOptions::new()
        .host("127.0.0.1")
        .port(
            std::env::var("STUDIO_PG_PORT")
                .unwrap_or_else(|_| "55432".into())
                .parse()
                .unwrap(),
        )
        .username("studio")
        .password(&std::env::var("STUDIO_TEST_PASSWORD").unwrap_or_else(|_| "studio_test".into()))
        .database("studio_test");
    let mut connection = sqlx::PgConnection::connect_with(&options).await.unwrap();
    connection
        .execute("CREATE TEMP TABLE spike_session (n INT)")
        .await
        .unwrap();
    connection.execute("BEGIN").await.unwrap();
    connection
        .execute("INSERT INTO spike_session VALUES (42)")
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT n FROM spike_session")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        42
    );
    // SQLx wrapper depth does not track BEGIN sent as arbitrary SQL.
    assert!(!connection.is_in_transaction());
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    let task = tokio::spawn(async move {
        let result = connection.execute("SELECT pg_sleep(30)").await;
        (connection, result)
    });
    let mut control = sqlx::PgConnection::connect_with(&options).await.unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let sent: bool = sqlx::query_scalar("SELECT pg_cancel_backend($1)")
        .bind(pid)
        .fetch_one(&mut control)
        .await
        .unwrap();
    assert!(sent);
    let (mut connection, result) = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .unwrap(),
        "57014"
    );
    let aborted = connection.execute("SELECT 1").await.unwrap_err();
    assert_eq!(
        aborted.as_database_error().unwrap().code().unwrap(),
        "25P02"
    );
    connection.execute("ROLLBACK").await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM spike_session")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        0
    );
    connection.close().await.unwrap();
    control.close().await.unwrap();
}

#[tokio::test]
#[ignore = "requires disposable Docker MySQL"]
async fn mysql_session_cancel_recovery() {
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(
            std::env::var("STUDIO_MYSQL_PORT")
                .unwrap_or_else(|_| "53306".into())
                .parse()
                .unwrap(),
        )
        .username("studio")
        .password(&std::env::var("STUDIO_TEST_PASSWORD").unwrap_or_else(|_| "studio_test".into()))
        .database("studio_test");
    let mut connection = sqlx::MySqlConnection::connect_with(&options).await.unwrap();
    connection
        .execute("CREATE TEMPORARY TABLE spike_session (n INT)")
        .await
        .unwrap();
    connection.execute("BEGIN").await.unwrap();
    connection
        .execute("INSERT INTO spike_session VALUES (42)")
        .await
        .unwrap();
    assert_eq!(
        sqlx::query("SELECT n FROM spike_session")
            .fetch_one(&mut connection)
            .await
            .unwrap()
            .get::<i32, _>(0),
        42
    );
    assert!(!connection.is_in_transaction());
    let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    let task = tokio::spawn(async move {
        let result = connection
            .execute("SELECT n FROM spike_session WHERE SLEEP(30)")
            .await;
        (connection, result)
    });
    let mut control = sqlx::MySqlConnection::connect_with(&options).await.unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    control
        .execute(format!("KILL QUERY {id}").as_str())
        .await
        .unwrap();
    let (mut connection, result) = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        result
            .unwrap_err()
            .as_database_error()
            .unwrap()
            .code()
            .unwrap(),
        "70100"
    );
    connection.execute("ROLLBACK").await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM spike_session")
            .fetch_one(&mut connection)
            .await
            .unwrap(),
        0
    );
    connection.close().await.unwrap();
    control.close().await.unwrap();
}

#[tokio::test]
#[ignore = "diagnostic disposable MySQL"]
async fn mysql_catalog_binary_names_decode_losslessly() {
    let options = MySqlConnectOptions::new()
        .host("127.0.0.1")
        .port(
            std::env::var("STUDIO_MYSQL_PORT")
                .unwrap_or_else(|_| "53306".into())
                .parse()
                .unwrap(),
        )
        .username("studio")
        .password(&std::env::var("STUDIO_TEST_PASSWORD").unwrap_or_else(|_| "studio_test".into()))
        .database("studio_test");
    let mut c = sqlx::MySqlConnection::connect_with(&options).await.unwrap();
    let rows = sqlx::query(
        "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
    )
    .fetch_all(&mut c)
    .await
    .unwrap();
    for row in rows {
        assert!(row.try_get::<String, _>("name").is_err());
        let name: String = row.try_get_unchecked("name").unwrap();
        assert!(!name.is_empty());
    }
    c.close().await.unwrap();
}
