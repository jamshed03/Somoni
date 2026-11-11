//! SQLite databases through the crate `rusqlite`: `db_open` opens one, `db.execute` runs a statement with parameters.
//! Values only reach the SQL text as `?` parameters, never as text, so no input can change the statement.

use std::fmt;
use std::path::Path;
use std::rc::Rc;

use rusqlite::Connection;
use rusqlite::types::Value as SqlValue;

use crate::value::Value;

use super::string_value;

/// The name of a database that lives only in memory, as SQLite calls it.
const IN_MEMORY: &str = ":memory:";

/// An open database, with the path it was opened with for `print`.
pub struct Database {
    connection: Connection,
    name: String,
}

/// Shows a database as `Database("data/game.db")`, without the connection.
impl fmt::Debug for Database {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Database({:?})", self.name)
    }
}

/// Two databases are equal only if they are the same open connection.
impl PartialEq for Database {
    fn eq(&self, other: &Database) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Database {
    /// Returns the path the database was opened with, like `data/game.db` or `:memory:`.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Opens the database `path`, which starts at the folder `dir` and is made if it is missing, as `ok(database)`,
/// or `err(reason)`; `:memory:` opens one that lives only in memory.
pub(super) fn open(dir: &str, path: &str) -> Value {
    let opened = if path == IN_MEMORY {
        Connection::open_in_memory()
    } else {
        Connection::open(Path::new(dir).join(path))
    };
    match opened {
        Ok(connection) => {
            let name = path.to_string();
            Value::Ok(Rc::new(Value::Database(Rc::new(Database {
                connection,
                name,
            }))))
        }
        Err(error) => sql_error(&format!("cannot open '{}': {}", path, error)),
    }
}

/// Runs the statement `sql` with the values of `params` for its `?`, as `ok(changed rows)` or `err(reason)`.
pub(super) fn execute(db: &Database, sql: &str, params: &Value) -> Value {
    let values = sql_params(params);
    match db
        .connection
        .execute(sql, rusqlite::params_from_iter(values))
    {
        Ok(changed) => {
            let changed = i64::try_from(changed).unwrap_or(i64::MAX);
            Value::Ok(Rc::new(Value::Int(changed)))
        }
        Err(error) => sql_error(&error.to_string()),
    }
}

/// Returns the values of a list or tuple of parameters as SQLite values.
fn sql_params(params: &Value) -> Vec<SqlValue> {
    match params {
        Value::List(items) | Value::Tuple(items) => items.iter().map(sql_value).collect(),
        other => vec![sql_value(other)],
    }
}

/// Returns one parameter as an SQLite value: `bool` as 0 or 1, `null` as `NULL`.
fn sql_value(value: &Value) -> SqlValue {
    match value {
        Value::Int(n) => SqlValue::Integer(*n),
        Value::Float(f) => SqlValue::Real(*f),
        Value::Str(s) => SqlValue::Text(s.as_str().to_string()),
        Value::Bool(b) => SqlValue::Integer(i64::from(*b)),
        _ => SqlValue::Null,
    }
}

/// Returns `err(message)`.
fn sql_error(message: &str) -> Value {
    Value::Err(Rc::new(string_value(message.to_string())))
}
