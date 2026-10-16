<?php
$text = "";
for ($i = 0; $i < 10000; $i = $i + 1) {
    $text = $text . "x";
}
function size(string $t): int {
    return strlen($t);
}
$total = 0;
for ($i = 0; $i < 100000; $i = $i + 1) {
    $total = $total + size($text);
}
echo $total, "\n";
