pub(super) fn plain_text_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"", term.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

#[cfg(test)]
mod tests {
    use super::plain_text_fts_query;

    #[test]
    fn plain_text_fts_query_quotes_terms_and_escapes_embedded_quotes() {
        let query = plain_text_fts_query("alpha beta\"gamma");

        assert_eq!(query.as_deref(), Some("\"alpha\" \"beta\"\"gamma\""));
    }

    #[test]
    fn plain_text_fts_query_drops_blank_input() {
        assert_eq!(plain_text_fts_query("   \n\t  "), None);
    }
}