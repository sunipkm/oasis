use sqlx::{
    pool::PoolConnection, sqlite::SqliteRow, Error, Executor, FromRow, Sqlite, Transaction,
};

#[derive(Debug)]
pub struct Query<'a> {
    pub sql: &'a str,
    pub args: Vec<String>,
}

impl<'a> Query<'a> {
    pub fn new(sql: &'a str, args: Vec<String>) -> Self {
        Query { sql, args }
    }
}

#[macro_export]
macro_rules! args {
    ( $( $x:expr ),* ) => {
        {
            let mut temp_vec = Vec::new();
            $(
                let x_str = $x.to_string();
                temp_vec.push(x_str);
            )*
            temp_vec
        }
    };
}

pub async fn fetch_single<'r, T>(
    query: Query<'r>,
    conn: &mut PoolConnection<Sqlite>,
) -> Result<Option<T>, Error>
where
    T: Send + Unpin + for<'a> FromRow<'a, SqliteRow>,
{
    let stmt: sqlx::query::QueryAs<'_, Sqlite, T, sqlx::sqlite::SqliteArguments<'_>> =
        prepare_sql(query.sql, &query.args);

    conn.fetch_optional(stmt)
        .await?
        .map(|u| T::from_row(&u))
        .transpose()
}

pub async fn fetch_multiple<'r, T>(
    query: Query<'r>,
    conn: &mut PoolConnection<Sqlite>,
) -> Result<Vec<T>, Error>
where
    T: Send + Unpin + for<'a> FromRow<'a, SqliteRow>,
{
    let stmt: sqlx::query::QueryAs<'_, Sqlite, T, sqlx::sqlite::SqliteArguments<'_>> =
        prepare_sql(query.sql, &query.args);

    let res = conn
        .fetch_all(stmt)
        .await?
        .iter()
        .map(|row| T::from_row(row))
        .collect::<Result<Vec<T>, Error>>()?;

    Ok(res)
}

pub async fn execute<'r>(query: Query<'r>, tx: &mut Transaction<'_, Sqlite>) -> Result<i64, Error> {
    let mut row_id = 0;
    let stmt = prepare_exec_sql(query.sql, &query.args);

    if query.sql.to_lowercase().starts_with("insert") {
        row_id = tx.execute(stmt).await?.last_insert_rowid();
    } else {
        tx.execute(stmt).await?;
    }

    Ok(row_id)
}

fn prepare_sql<'a, T>(
    sql: &'a str,
    args: &'a [String],
) -> sqlx::query::QueryAs<'a, sqlx::Sqlite, T, sqlx::sqlite::SqliteArguments<'a>>
where
    T: Send + Unpin + for<'b> FromRow<'b, SqliteRow>,
{
    let mut stmt = sqlx::query_as(sql);
    for arg in args.iter() {
        stmt = stmt.bind(arg);
    }

    stmt
}

fn prepare_exec_sql<'a>(
    sql: &'a str,
    args: &'a [String],
) -> sqlx::query::Query<'a, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'a>> {
    let mut stmt = sqlx::query(sql);
    for arg in args.iter() {
        stmt = stmt.bind(arg);
    }

    stmt
}
