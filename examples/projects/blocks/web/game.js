// Draws the game that web/game.smn sends as JSON and sends it the keys; all rules stay in Somoni.

const WIDTH = 10;
const HEIGHT = 20;
const TICK_MS = 700;
const KEYS = {
    ArrowLeft: "left", a: "left",
    ArrowRight: "right", d: "right",
    ArrowUp: "rotate", w: "rotate",
    ArrowDown: "down", s: "down",
    " ": "drop",
};

const board = document.getElementById("board");
const score = document.getElementById("score");
const over = document.getElementById("over");
const best = document.getElementById("scores");
const cells = [];
let finished = false;
let waiting = 0;
let queue = Promise.resolve();

// Makes the cells of the well once; drawing only changes their classes.
function makeCells() {
    for (let i = 0; i < WIDTH * HEIGHT; i += 1) {
        const cell = document.createElement("div");
        cell.className = "cell";
        board.append(cell);
        cells.push(cell);
    }
}

// Draws a state: "." is empty, a capital letter a landed cell, a small letter the falling piece.
function draw(state) {
    state.rows.forEach((row, r) => {
        [...row].forEach((letter, c) => {
            const cell = cells[r * WIDTH + c];
            if (letter === ".") {
                cell.className = "cell";
            } else if (letter === letter.toLowerCase()) {
                cell.className = `cell shape-${letter.toUpperCase()} falling`;
            } else {
                cell.className = `cell shape-${letter}`;
            }
        });
    });
    score.textContent = state.score;
    over.hidden = !state.over;
    if (state.over && !finished) {
        loadScores();
    }
    finished = state.over;
}

// Shows the best scores from web/scores.smn.
async function loadScores() {
    const scores = await (await fetch("scores")).json();
    best.replaceChildren(...scores.map((entry) => {
        const item = document.createElement("li");
        item.textContent = `${entry.points} · ${entry.date}`;
        return item;
    }));
}

// Asks the server for the state, after running `command` if there is one.
async function request(command) {
    const options = command ? { method: "POST", body: new URLSearchParams({ command }) } : {};
    const response = await fetch("game", options);
    const state = await response.json();
    if (state.error) {
        console.error(state.error);
        return;
    }
    draw(state);
}

// Sends a command after the ones before it, so a key and the tick never change their order.
function send(command) {
    waiting += 1;
    queue = queue
        .then(() => request(command))
        .catch((error) => console.error(error))
        .finally(() => {
            waiting -= 1;
        });
}

document.addEventListener("keydown", (event) => {
    const command = KEYS[event.key];
    if (!command || finished) {
        return;
    }
    event.preventDefault();
    send(command);
});

document.querySelectorAll("[data-command]").forEach((button) => {
    button.addEventListener("click", () => {
        if (!finished) {
            send(button.dataset.command);
        }
    });
});

document.getElementById("new").addEventListener("click", () => send("new"));

// Lets the piece fall one row per tick, but only when no command waits, so a slow answer never builds a queue.
setInterval(() => {
    if (!finished && waiting === 0) {
        send("down");
    }
}, TICK_MS);

makeCells();
send(null);
loadScores();
