//! SQLite databases through the crate `rusqlite`: `db_open` opens one, `db.execute` runs a statement with parameters,
//! `db.fetch` gives rows, and the getters of a row return its values with their Somoni types.
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

/// Runs the query `sql` with the values of `params` for its `?`, as `ok(list of rows)` or `err(reason)`.
pub(super) fn fetch(db: &Database, sql: &str, params: &Value) -> Value {
    let mut statement = match db.connection.prepare(sql) {
        Ok(statement) => statement,
        Err(error) => return sql_error(&error.to_string()),
    };
    let names: Rc<[String]> = statement.column_names().map(String::from).collect();
    let columns = names.len();
    let read = statement.query_map(rusqlite::params_from_iter(sql_params(params)), |row| {
        (0..columns)
            .map(|i| row.get::<_, SqlValue>(i))
            .collect::<Result<Vec<_>, _>>()
    });
    let rows: Result<Vec<Value>, rusqlite::Error> = read.and_then(|found| {
        found
            .map(|values| {
                let names = Rc::clone(&names);
                values.map(|values| Value::Row(Rc::new(Row { names, values })))
            })
            .collect()
    });
    match rows {
        Ok(rows) => Value::Ok(Rc::new(Value::List(Rc::new(rows)))),
        Err(error) => sql_error(&error.to_string()),
    }
}

/// One row of a query: the names of its columns, shared by all rows, and its values.
#[derive(Debug, PartialEq)]
pub struct Row {
    names: Rc<[String]>,
    values: Vec<SqlValue>,
}

impl Row {
    /// Returns the names of the columns, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(String::as_str)
    }

    /// Returns the values as Somoni values for `print`; a blob shows its size.
    pub fn shown_values(&self) -> Vec<Value> {
        self.values.iter().map(shown_value).collect()
    }

    /// Returns the value of the first column with this name, or `err("no column 'x'")`.
    fn cell(&self, column: &str) -> Result<&SqlValue, Value> {
        self.names()
            .position(|name| name == column)
            .map(|i| &self.values[i])
            .ok_or_else(|| sql_error(&format!("no column '{}'", column)))
    }
}

/// The Somoni type a getter like `get_int` wants from a column.
#[derive(Debug, Clone, Copy)]
pub(super) enum Wanted {
    Int,
    Float,
    Str,
    Bool,
}

/// Returns the value of `column` as the type `wanted`, or `err` for an unknown column, `NULL` or another type;
/// no value is changed into another type, like everywhere in Somoni.
pub(super) fn get(row: &Row, column: &str, wanted: Wanted) -> Value {
    let cell = match row.cell(column) {
        Ok(cell) => cell,
        Err(error) => return error,
    };
    let value = match (wanted, cell) {
        (Wanted::Int, SqlValue::Integer(n)) => Value::Int(*n),
        (Wanted::Float, SqlValue::Real(f)) => Value::Float(*f),
        (Wanted::Str, SqlValue::Text(text)) => string_value(text.clone()),
        (Wanted::Bool, SqlValue::Integer(0)) => Value::Bool(false),
        (Wanted::Bool, SqlValue::Integer(1)) => Value::Bool(true),
        (_, SqlValue::Null) => return sql_error(&format!("column '{}' is NULL", column)),
        (_, other) => {
            let message = format!(
                "column '{}' is {}, not {}",
                column,
                kind(other),
                wanted.name()
            );
            return sql_error(&message);
        }
    };
    Value::Ok(Rc::new(value))
}

/// Returns whether `column` is `NULL`, or `err` for an unknown column.
pub(super) fn is_null(row: &Row, column: &str) -> Value {
    match row.cell(column) {
        Ok(cell) => Value::Ok(Rc::new(Value::Bool(*cell == SqlValue::Null))),
        Err(error) => error,
    }
}

impl Wanted {
    /// Returns the Somoni name of the type, like `int`.
    fn name(self) -> &'static str {
        match self {
            Wanted::Int => "int",
            Wanted::Float => "float",
            Wanted::Str => "string",
            Wanted::Bool => "bool",
        }
    }
}

/// Returns what kind of value SQLite holds, in Somoni words: `int`, `float`, `string`, `blob` or `NULL`.
fn kind(value: &SqlValue) -> &'static str {
    match value {
        SqlValue::Integer(_) => "int",
        SqlValue::Real(_) => "float",
        SqlValue::Text(_) => "string",
        SqlValue::Blob(_) => "blob",
        SqlValue::Null => "NULL",
    }
}

/// Returns an SQLite value as the Somoni value `print` shows; a blob as its size, like `<3 bytes>`.
fn shown_value(value: &SqlValue) -> Value {
    match value {
        SqlValue::Integer(n) => Value::Int(*n),
        SqlValue::Real(f) => Value::Float(*f),
        SqlValue::Text(text) => string_value(text.clone()),
        SqlValue::Blob(bytes) => string_value(format!("<{} bytes>", bytes.len())),
        SqlValue::Null => Value::Null,
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
