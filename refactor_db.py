import os
import re

def refactor_file(filepath):
    with open(filepath, "r", encoding="utf-8") as f:
        content = f.read()

    # Change test references
    content = content.replace("db.conn.query_row", "db.get_conn().unwrap().query_row")
    content = content.replace("db.conn.execute", "db.get_conn().unwrap().execute")
    
    # Process line by line
    new_lines = []
    lines = content.split('\n')
    inside_impl_db = False
    brace_depth = 0
    in_fn = False
    
    for line in lines:
        if "impl Db {" in line:
            inside_impl_db = True
            
        if inside_impl_db:
            if "{" in line:
                brace_depth += line.count("{")
            if "}" in line:
                brace_depth -= line.count("}")
                if brace_depth == 0:
                    inside_impl_db = False
            
            if "pub fn" in line or "fn query_basic_relations" in line:
                # Signature replacement
                if "(&mut self" in line:
                    line = line.replace("(&mut self", "(&self")
                elif "&mut self" in line:
                    line = line.replace("&mut self", "&self")
                
                # Check if it's a one-line signature ending with {
                if "{" in line:
                    if "self.conn" in content: # rough approximation
                        new_lines.append(line)
                        new_lines.append("        let mut conn = self.get_conn()?;")
                        continue
                else:
                    in_fn = True
            
            if in_fn and "{" in line:
                new_lines.append(line)
                new_lines.append("        let mut conn = self.get_conn()?;")
                in_fn = False
                continue
                
            # Replace self.conn inside method bodies
            if "self.conn" in line:
                line = line.replace("self.conn", "conn")
        
        new_lines.append(line)

    with open(filepath, "w", encoding="utf-8") as f:
        f.write("\n".join(new_lines))


if __name__ == "__main__":
    db_dir = "src/db"
    for filename in ["documents.rs", "relations.rs", "search.rs"]:
        refactor_file(os.path.join(db_dir, filename))
