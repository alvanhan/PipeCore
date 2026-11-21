mod common;
use pipecore::{
    config::{Config, Table},
    db::Db,
    report::Report,
};
use std::path::Path;
async fn job(
    c: &Config,
    input: Option<&Path>,
    output: Option<&Path>,
    dry: bool,
) -> (Result<(), u8>, Report) {
    let mut r = Report::new(&c.operation);
    let result = pipecore::run(c, &c.operation, input, output, dry, &mut r).await;
    r.finish().unwrap();
    (result, r)
}
async fn count(db: &mut Db, t: &str) -> u64 {
    db.scalar(&format!("SELECT {} FROM {t}", db.text("COUNT(*)")))
        .await
        .unwrap()
}
async fn suite(driver: &str) {
    let dir = tempfile::tempdir().unwrap();
    let mut c = common::config("import");
    c.connection.driver = driver.into();
    c.connection.dsn_env = if driver == "postgres" {
        "PIPECORE_PG_DSN"
    } else {
        "PIPECORE_MSSQL_DSN"
    }
    .into();
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    let mut db = Db::connect(&c.connection).await.unwrap();
    let schema = format!("pc_{}", uuid::Uuid::new_v4().simple());
    db.batch(&format!("CREATE SCHEMA {}", db.q(&schema)))
        .await
        .unwrap();
    let ddl = std::fs::read_to_string(format!("tests/fixtures/sql/{driver}.sql")).unwrap();
    if driver == "postgres" {
        db.batch(&ddl.replace("public.", &format!("{schema}.")))
            .await
            .unwrap();
    } else {
        let ddl = ddl[ddl.find("IF OBJECT_ID").unwrap()..].replace("dbo.", &format!("{schema}."));
        for batch in ddl.split("\nGO") {
            if !batch.trim().is_empty() {
                db.batch(batch).await.unwrap();
            }
        }
    }
    c.target.as_mut().unwrap().schema = schema.clone();
    let table = db.table(c.target.as_ref().unwrap());
    if driver == "mssql" {
        db.batch(&format!("CREATE TRIGGER {}.audit_insert ON {table} AFTER INSERT AS BEGIN DECLARE @audit TABLE (id int); INSERT @audit(id) SELECT 1 FROM inserted; END",db.q(&schema))).await.unwrap();
    }
    let input = dir.path().join("input.xlsx");
    common::fixture(&c, &input, 0, 4000, true);
    let (result, r) = job(&c, Some(&input), None, false).await;
    assert_eq!(result, Err(4), "{r:?}");
    assert_eq!(count(&mut db, &table).await, 0);
    common::fixture(&c, &input, 0, 4000, false);
    let (result, r) = job(&c, Some(&input), None, true).await;
    assert!(result.is_ok(), "{r:?}");
    assert_eq!(count(&mut db, &table).await, 0);
    assert!(!r.committed);
    let (result, r) = job(&c, Some(&input), None, false).await;
    assert!(result.is_ok(), "{r:?}");
    assert!(r.committed);
    assert_eq!(r.summary.inserted_rows, 4000);
    assert_eq!(count(&mut db, &table).await, 4000);
    let sql = format!(
        "SELECT {},{} FROM {table}",
        db.text("debit_amount"),
        db.text("quantity")
    );
    let values = db.query(&sql, &[]).await.unwrap();
    assert_eq!(values[0][0].as_deref(), Some("1234567890123.1234"));
    assert_eq!(values[0][1].as_deref(), Some("9007199254740993"));
    let (result, r) = job(&c, Some(&input), None, false).await;
    assert_eq!(result, Err(6), "{r:?}");
    assert_eq!(count(&mut db, &table).await, 4000);
    let mut update = common::config("bulk-update");
    update.connection = c.connection.clone();
    update.target = c.target.clone();
    update.execution = c.execution.clone();
    db.batch(&format!("UPDATE {table} SET balance_after=0"))
        .await
        .unwrap();
    common::fixture(&update, &input, 2, 4000, false);
    let (result, r) = job(&update, Some(&input), None, false).await;
    assert_eq!(result, Err(5), "{r:?}");
    assert_eq!(r.summary.matched_rows, 3998);
    assert_eq!(r.summary.missing_rows, 2);
    assert_eq!(r.summary.updated_rows, 0);
    assert_eq!(r.errors.len(), 2);
    assert_eq!(r.errors[0].row, Some(4000));
    assert_eq!(r.errors[1].row, Some(4001));
    assert_eq!(
        db.scalar(&format!(
            "SELECT {} FROM {table} WHERE balance_after<>0",
            db.text("COUNT(*)")
        ))
        .await
        .unwrap(),
        0
    );
    common::fixture(&update, &input, 0, 4000, false);
    let mut nonunique = update.clone();
    nonunique.mutation.as_mut().unwrap().matching.keys = vec!["balance_after".into()];
    let (result, r) = job(&nonunique, Some(&input), None, false).await;
    assert_eq!(result, Err(5), "{r:?}");
    let mut blocker = Db::connect(&c.connection).await.unwrap();
    blocker.begin().await.unwrap();
    blocker
        .execute(&format!(
            "DELETE FROM {table} WHERE transaction_id='TX000000000000'"
        ))
        .await
        .unwrap();
    let (outcome, ()) = tokio::join!(job(&update, Some(&input), None, false), async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        blocker.commit().await.unwrap();
    });
    assert_eq!(outcome.0, Err(5), "{:?}", outcome.1);
    assert_eq!(outcome.1.summary.missing_rows, 1);
    assert_eq!(outcome.1.summary.updated_rows, 0);
    let restore = dir.path().join("restore.xlsx");
    common::fixture(&c, &restore, 0, 1, false);
    let (result, r) = job(&c, Some(&restore), None, false).await;
    assert!(result.is_ok(), "{r:?}");
    let (result, r) = job(&update, Some(&input), None, false).await;
    assert!(result.is_ok(), "{r:?}");
    assert_eq!(r.summary.updated_rows, 4000);
    let mut upsert = common::config("upsert");
    upsert.connection = c.connection.clone();
    upsert.target = c.target.clone();
    upsert.execution = c.execution.clone();
    common::fixture(&upsert, &input, 1000, 4000, false);
    let (result, r) = job(&upsert, Some(&input), None, false).await;
    assert!(result.is_ok(), "{r:?}");
    assert_eq!(r.summary.updated_rows, 3000);
    assert_eq!(r.summary.inserted_rows, 1000);
    assert_eq!(count(&mut db, &table).await, 5000);
    let mut export = common::config("export");
    export.connection = c.connection.clone();
    export.execution = c.execution.clone();
    for s in &mut export.sheets {
        s.source.schema = schema.clone();
    }
    export.workbook.as_mut().unwrap().max_data_rows_per_sheet = 3000;
    export.sheets[0].columns.push(pipecore::config::Column {
        db_column: "description".into(),
        excel_header: "Description".into(),
        width: 40.0,
        format: "text".into(),
        decimal_as_text: false,
    });
    let output = dir.path().join("out.xlsx");
    let (result, r) = job(&export, None, Some(&output), false).await;
    assert!(result.is_ok(), "{r:?}");
    assert_eq!(r.summary.exported_rows, 5000);
    use calamine::Reader;
    let mut book: calamine::Xlsx<_> = calamine::open_workbook(&output).unwrap();
    assert_eq!(
        book.sheet_names(),
        ["Transactions", "Transactions_2", "Accounts"]
    );
    let range = book.worksheet_range("Transactions").unwrap();
    assert_eq!(range.height(), 3001);
    assert_eq!(
        range.get_value((1, 7)),
        Some(&calamine::Data::String("=literal + unicode 日本 😀".into()))
    );
    let original = std::fs::read(&output).unwrap();
    let mut invalid_export = export.clone();
    invalid_export.sheets[0].columns[0].db_column = "does_not_exist".into();
    let (result, _) = job(&invalid_export, None, Some(&output), false).await;
    assert!(result.is_err());
    assert_eq!(std::fs::read(&output).unwrap(), original);
    assert_eq!(
        range.get_value((1, 3)),
        Some(&calamine::Data::String("1234567890123.1234".into()))
    );
    // CHECK constraint failure after a valid source must roll back the entire insert.
    db.batch(&format!(
        "ALTER TABLE {table} ADD CONSTRAINT reject_test CHECK (units < 100)"
    ))
    .await
    .unwrap();
    let mut bad = c.clone();
    bad.fields
        .iter_mut()
        .find(|f| f.column == "units")
        .unwrap()
        .null_tokens = vec![];
    common::fixture(&bad, &input, 10000, 2, false);
    db.batch(&format!("ALTER TABLE {table} ADD CONSTRAINT reject_new CHECK (transaction_id NOT LIKE 'TX000000010%')")).await.unwrap();
    let (result, r) = job(&bad, Some(&input), None, false).await;
    assert_eq!(result, Err(6), "{r:?}");
    assert_eq!(count(&mut db, &table).await, 5000);
    if driver == "postgres" {
        db.batch(&format!("UPDATE {table} SET balance_after=0; CREATE FUNCTION {schema}.reject_update() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture rejection'; END $$; CREATE TRIGGER reject_update BEFORE UPDATE ON {table} FOR EACH STATEMENT EXECUTE FUNCTION {schema}.reject_update()" )).await.unwrap();
    } else {
        db.batch(&format!("UPDATE {table} SET balance_after=0"))
            .await
            .unwrap();
        db.batch(&format!("CREATE TRIGGER {schema}.reject_update ON {table} AFTER UPDATE AS BEGIN ;THROW 50001, 'fixture rejection', 1; END")).await.unwrap();
    }
    common::fixture(&update, &input, 0, 4000, false);
    let (result, r) = job(&update, Some(&input), None, false).await;
    assert_eq!(result, Err(6), "{r:?}");
    assert_eq!(
        db.scalar(&format!(
            "SELECT {} FROM {table} WHERE balance_after<>0",
            db.text("COUNT(*)")
        ))
        .await
        .unwrap(),
        0
    );
    if driver == "postgres" {
        db.batch(&format!("DROP SCHEMA {} CASCADE", db.q(&schema)))
            .await
            .unwrap();
    } else {
        db.batch(&format!(
            "DROP TABLE {table}; DROP TABLE {}; DROP SCHEMA {}",
            db.table(&Table {
                schema: schema.clone(),
                table: "finance_accounts".into()
            }),
            db.q(&schema)
        ))
        .await
        .unwrap();
    }
}
#[tokio::test]
#[ignore = "Requires Docker PostgreSQL fixture"]
async fn postgres_end_to_end() {
    suite("postgres").await;
}
#[tokio::test]
#[ignore = "Requires Docker SQL Server fixture"]
async fn mssql_end_to_end() {
    suite("mssql").await;
}
