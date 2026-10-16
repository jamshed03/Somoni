text = ""
i = 0
while i < 10000:
    text = text + "x"
    i = i + 1
def size(t):
    return len(t)
total = 0
i = 0
while i < 100000:
    total = total + size(text)
    i = i + 1
print(total)
