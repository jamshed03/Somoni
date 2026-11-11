# Blocks

A falling blocks game written in Somoni: pieces of four cells fall into a well of 10 × 20 cells, a full row disappears
and gives points, and the game ends when a new piece has no room. It runs in the terminal and in the browser with the
same game code, and keeps its high scores in SQLite.

The project shows how a larger Somoni program can be built: a hexagonal architecture (ports and adapters), and inside
it the patterns factory, observer and MVC, with DTOs at the borders.

## The rule

**The domain knows nothing about the outside.** It never reads a file, a request or a database, and it does not
import anything from `ports/` or `adapters/`. Everything it needs from outside comes in through an interface.

```
          driving adapters                                   driven adapters
   (they call the game: terminal, HTTP)              (the game calls them: pieces, storage)

   terminal.smn ──┐                                            ┌── RandomPieces, FixedPieces
                  │      ┌──────────────────────────────┐      │
   web/game.smn ──┼────► │ ports        domain          │ ────►├── SqliteScores
   (controller)   │      │ GameService  Game, Board,    │      │
                  │      │              Piece, events   │      └── SqliteGames
                  └───── │                              │
                         └──────────────────────────────┘
```

Imports only point inwards: `adapters/` may import `ports/` and `domain/`, `ports/` may import `domain/`, `domain/`
imports only `domain/`. So the `Game` does not know where its shapes come from: it says `needs_piece()`, and
`GameService` (in `ports/`) gives it the next shape from a `PieceSource`. The programs at the top (`terminal.smn`, `web/game.smn`) are the composition roots: the only
places that create adapters and hand them to the game.

## Folders

| Folder          | Holds                                                                                     |
| --------------- | ----------------------------------------------------------------------------------------- |
| `lib/domain/`   | the rules: `Shape`, `Piece`, `Board`, `Game`, the events of a game; no input or output      |
| `lib/ports/`    | the interfaces between the domain and the outside, and `GameService`, the one entry point  |
| `lib/adapters/` | the implementations: pieces, storage in SQLite, the text and JSON views                   |
| `web/`          | the browser: `index.html`, `game.js`, `style.css`, and `game.smn` for `somoni serve`        |
| top level       | `terminal.smn` to play in the terminal, and the test programs, each with its `.out`        |

Modules live under `lib/`, because `cargo test` runs every other `.smn` in `examples/` as a program.

## Ports

| Port          | Direction | What it does                                                    | Adapters                         |
| ------------- | --------- | --------------------------------------------------------------- | -------------------------------- |
| `GameService` | driving   | the commands of a player: move, rotate, drop, let time pass      | used by `terminal.smn`, `game.smn` |
| `PieceSource` | driven    | gives the next shape                                            | `RandomPieces`, `FixedPieces`    |
| `GameStore`   | driven    | keeps a game between two requests                               | `SqliteGames`                    |
| `ScoreStore`  | driven    | saves and lists the high scores                                 | `SqliteScores`                   |
| `GameView`    | driven    | turns a game into what a player sees                            | `TextView`, `JsonView`           |

`FixedPieces` gives the shapes in a fixed order, so the tests always see the same game.

## Patterns

-   **Factory:** `PieceFactory` makes a `Piece` from a `Shape`, the only place that knows the cells of each shape.
-   **Observer:** the `Game` tells its listeners (`GameListener`) about events: a piece landed, rows cleared, the
    game is over. The score is a rule of the game, so the `Game` counts it itself; listeners do what the game should
    not know about, like `HighscoreRecorder` saving a score at the end or the terminal printing a message.
-   **MVC** in the browser: the model is the `Game`, the view is `JsonView` with `game.js` drawing it, the controller
    is `web/game.smn`, which turns a request into a command of the `GameService`.
-   **DTO:** data crosses a border only as a plain struct. `GameStateDto` goes from the game to the view and becomes
    JSON; `ScoreDto` comes from a database row through the typed getters (`row.get_int(...)`), the only place where
    database types meet Somoni types.
-   **Dependency injection:** an adapter gets what it needs in its constructor, like `new SqliteScores(db)`; nothing
    creates its own database or reads a global.
-   **Memento:** `game.snapshot()` gives a `GameSnapshot` with everything that makes up the game, and
    `Game::restore(snapshot, factory)` goes on from it; the fields of `Game` stay private.

## Rules of the game

-   Seven shapes of four cells; a new piece starts at the top in the middle.
-   A move or rotation that would leave the well or hit a cell is ignored.
-   When a piece cannot fall any further, it lands; every full row disappears and the rows above move down.
-   Points for rows cleared at once: 1 row 100, 2 rows 300, 3 rows 500, 4 rows 800.
-   The game is over when a new piece has no room.

## What Somoni does not have yet

This project is also there to find the gaps. Each one is worked around here and noted in the checklist:

-   **No key presses without Enter:** in the terminal, every move is a line of input.
-   **No state between requests:** `somoni serve` starts fresh for every request, so the game is saved after each
    command through `GameStore` and loaded before the next.
-   **No JSON:** `JsonView` builds the text by hand.
-   **No enum as text:** `string(shape)` does not work yet, so `shape_letter` and `shape_from_letter` do it.
-   **Paths start at the file with the call:** an adapter in `lib/` cannot use a file path of the program, since
    `read_file("data/game.txt")` would start at `lib/adapters/`. So games are kept in SQLite: the page opens the
    database and hands it to `SqliteGames`.
-   **No headers of its own:** `somoni serve` sends every page as `text/html`, also the JSON of `web/game.smn`;
    `fetch(...).json()` in the browser does not mind.

## Status

The project grows in steps, each one working on its own:

1. [x] this README: the architecture and the rules
2. [x] the domain with tests: shapes, pieces, the board, the game, its events
3. [x] `GameService`, `PieceSource` with `FixedPieces` and `RandomPieces`, `TextView`, and `terminal.smn` to play in the terminal
4. [x] `GameStore` in SQLite, `JsonView`, and `web/game.smn` for `somoni serve`
5. [ ] the browser: `index.html`, `game.js`, `style.css`
6. [ ] high scores in SQLite with `ScoreDto`

## Running

The terminal works now (`demo` as an argument plays a fixed order of shapes). `web/game.smn` answers `GET /game`
and `POST /game` with `command=left` (or `right`, `rotate`, `down`, `drop`, `new`) as JSON; the page to play
it in the browser comes with step 5.


```
cargo run --release -- examples/projects/blocks/terminal.smn
somoni serve examples/projects/blocks/web      # then open http://localhost:8000
```
