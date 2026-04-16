with open("tests/integration_test.rs", "r", encoding="utf-8") as f:
    t = f.read()
    
t = t.replace("use tokio::sync::Mutex as TokioMutex;\n", "")
t = t.replace("let db_arc = Arc::new(TokioMutex::new(db));", "let db_arc = Arc::new(db);")
t = t.replace("let db_lock = db_arc.lock().await;", "")
t = t.replace("db_lock.", "db_arc.")

with open("tests/integration_test.rs", "w", encoding="utf-8") as f:
    f.write(t)

with open("src/db/documents.rs", "r", encoding="utf-8") as f:
    d = f.read()

import re
d = re.sub(r"db\s*\n\s*\.conn\s*\n\s*\.query_row", r"db.get_conn().unwrap().query_row", d)

with open("src/db/documents.rs", "w", encoding="utf-8") as f:
    f.write(d)
