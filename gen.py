import os
os.makedirs("src", exist_ok=True)
with open("src/main.rs", "w", encoding="utf-8") as f:
    f.write(open("src/main.rs.src").read())
