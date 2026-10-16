let text = "";
for (let i = 0; i < 10000; i = i + 1) {
    text = text + "x";
}
function size(t) {
    return t.length;
}
let total = 0;
for (let i = 0; i < 100000; i = i + 1) {
    total = total + size(text);
}
console.log(total);
