import os

def process_file(filepath):
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    original = content
    # Handle the let mut db_guard = self.db.lock().await
    content = content.replace("let mut db_guard = self.db.lock().await;", "let db_guard = self.db.clone();")
    content = content.replace("let db_guard = self.db.lock().await;", "let db_guard = self.db.clone();")
    
    # Handle let mut db = self.ctx.db.lock().await;
    content = content.replace("let mut db = self.ctx.db.lock().await;", "let db = &self.ctx.db;")
    content = content.replace("let db = self.ctx.db.lock().await;", "let db = &self.ctx.db;")

    # Handle inline lock calls:  self.db.lock().await.method() -> self.db.method()
    content = content.replace("self.db.lock().await.", "self.db.")
    content = content.replace("self.ctx.db.lock().await.", "self.ctx.db.")
    content = content.replace("db.lock().await.", "db.")

    if content != original:
        with open(filepath, "w", encoding="utf-8") as f:
            f.write(content)
        print(f"Updated {filepath}")

if __name__ == "__main__":
    for root, _, files in os.walk("src"):
        for file in files:
            if file.endswith(".rs"):
                filepath = os.path.join(root, file)
                process_file(filepath)
