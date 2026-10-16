<?php
$s = "";
for ($i = 0; $i < 100000; $i = $i + 1) {
    $s = $s . "x";
}
echo strlen($s), "\n";
