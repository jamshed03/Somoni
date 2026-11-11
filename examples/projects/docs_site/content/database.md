# Database

`db_open` opens an SQLite database: one file, no server. Values reach the SQL only as `?` parameters, and rows come back with typed getters.

```smn
struct Score {
    name: string,
    points: int
}

// Fills the table and turns its rows into Score values: the one place where the types from the database are checked.
fn best(db: database): result<list<Score>, string> {
    db.execute("CREATE TABLE scores (name TEXT, points INTEGER)", [])?;
    db.execute("INSERT INTO scores (name, points) VALUES (?, ?)", ("Ali", 120))?;
    db.execute("INSERT INTO scores (name, points) VALUES (?, ?)", ("Bea", 95))?;
    var found: list<Score> = [];
    for (row in db.fetch("SELECT name, points FROM scores ORDER BY points DESC", [])?) {
        found += [Score { name: row.get_string("name")?, points: row.get_int("points")? }];
    }
    return ok(found);
}

match (db_open(":memory:")) {
    ok(db: database) => {
        print(best(db));
    }
    err(reason: string) => {
        print(reason);
    }
}
```

- `db_open("data/app.db")` opens or makes a file next to the program; `":memory:"` keeps the database in memory, good for tests
- `db.execute(sql, params)` runs a statement and gives the number of changed rows; `db.fetch(sql, params)` gives the rows as `list<row>`
- Parameters are a list (`[name]`, or `[]` for none) or a tuple for mixed types (`("Ali", 120)`); `null` becomes `NULL` and `bool` becomes 0 or 1
- Never put values into the SQL text yourself: with `?`, no input can change the statement (no SQL injection)
- `row.get_int("points")`, `get_float`, `get_string` and `get_bool` return a `result`: `NULL`, another type or an unknown column is an `err`, nothing is converted silently; `row.is_null("name")` asks for `NULL`
- Every function returns a `result`, so a broken statement like a missing table is an `err` to handle with `match` or `?`
- `database` and `row` are types you can write, so a class can get its database from outside: `constructor(db: database)`
- Under `somoni serve` the path stays inside the served folder, like for `read_file`
