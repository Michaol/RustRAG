#[derive(Debug, Clone)]
pub struct Chunk<'a> {
    pub position: usize,
    pub content: &'a str,
}

#[derive(Debug, Clone)]
pub struct CodeChunk<'a> {
    pub chunk: Chunk<'a>,
    pub symbol_name: Option<&'a str>,
    pub symbol_type: &'a str,
    pub language: &'a str,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub parent_symbol: Option<&'a str>,
    pub signature: Option<&'a str>,
}

#[derive(Debug)]
pub struct CodeMetadata {
    pub id: i64,
    pub chunk_id: i64,
    pub symbol_name: Option<String>,
    pub symbol_type: String,
    pub language: String,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub parent_symbol: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug)]
pub struct CodeRelation {
    pub id: i64,
    pub source_chunk_id: i64,
    pub target_chunk_id: Option<i64>,
    pub relation_type: String,
    pub target_name: String,
    pub target_file: Option<String>,
    pub confidence: f64,
    // Joined fields from search
    pub source_name: Option<String>,
    pub source_file: Option<String>,
}

#[derive(Debug)]
pub struct WordMapping {
    pub id: i64,
    pub source_word: String,
    pub target_word: String,
    pub source_lang: String,
    pub confidence: f64,
    pub source_document: Option<String>,
}
