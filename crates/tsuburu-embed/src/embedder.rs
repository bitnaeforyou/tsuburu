//! Turning a phrase into a vector, through a model the user supplies.
//!
//! The index was built with Qwen3-Embedding-4B, so a phrase can only be
//! compared against it by that same model: a different one lands in a
//! different space and the numbers mean nothing. Several gigabytes of
//! weights are not something to ship inside a 3 MB binary, so tsuburu talks
//! to an embeddings endpoint the user runs — llama.cpp's server, Ollama,
//! anything speaking the OpenAI shape — and ships nothing.
//!
//! The stored vectors are 1024 wide while the model emits 2560. That is
//! Matryoshka truncation: keep the leading 1024 and normalise again.

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum EmbedderError {
    #[error("could not reach the embedding server: {0}")]
    Unreachable(String),
    #[error("the embedding server answered {0}")]
    Status(u16),
    #[error("the embedding server's answer could not be read: {0}")]
    Malformed(String),
    #[error("the model returned {got} dimensions, fewer than the {want} the index uses")]
    TooNarrow { got: usize, want: usize },
}

#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    /// An OpenAI-shaped endpoint, e.g. `http://127.0.0.1:8080/v1/embeddings`.
    pub url: String,
    pub model: String,
    /// Width the index expects; the reply is truncated to it.
    pub dims: usize,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            url: "http://127.0.0.1:8080/v1/embeddings".into(),
            model: "Qwen3-Embedding-4B".into(),
            dims: 1024,
        }
    }
}

#[derive(Serialize)]
struct Request<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<Datum>,
}

#[derive(Deserialize)]
struct Datum {
    embedding: Vec<f32>,
}

/// Reads an OpenAI-shaped embeddings reply and prepares it for the index.
pub fn parse_embedding(body: &str, dims: usize) -> Result<Vec<f32>, EmbedderError> {
    let parsed: Response =
        serde_json::from_str(body).map_err(|e| EmbedderError::Malformed(e.to_string()))?;
    let raw = parsed
        .data
        .into_iter()
        .next()
        .ok_or_else(|| EmbedderError::Malformed("no embedding in the reply".into()))?
        .embedding;
    truncate_and_normalise(raw, dims)
}

/// Keeps the leading `dims` values and restores unit length.
pub fn truncate_and_normalise(
    mut vector: Vec<f32>,
    dims: usize,
) -> Result<Vec<f32>, EmbedderError> {
    if vector.len() < dims {
        return Err(EmbedderError::TooNarrow { got: vector.len(), want: dims });
    }
    vector.truncate(dims);
    let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut vector {
            *x /= norm;
        }
    }
    Ok(vector)
}

/// The body to POST for `text`.
pub fn request_body(config: &EmbedderConfig, text: &str) -> String {
    serde_json::to_string(&Request { model: &config.model, input: text })
        .unwrap_or_else(|_| "{}".into())
}

/// Cosine similarity between two unit vectors.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_openai_shaped_reply() {
        let body = r#"{"data":[{"embedding":[3.0,4.0,99.0]}],"model":"x"}"#;
        let v = parse_embedding(body, 2).unwrap();
        assert_eq!(v, vec![0.6, 0.8], "truncated to width, then normalised");
    }

    #[test]
    fn a_narrower_model_is_refused_rather_than_padded() {
        let body = r#"{"data":[{"embedding":[1.0,2.0]}]}"#;
        assert!(matches!(
            parse_embedding(body, 1024),
            Err(EmbedderError::TooNarrow { got: 2, want: 1024 })
        ));
    }

    #[test]
    fn an_empty_or_broken_reply_is_reported() {
        assert!(matches!(parse_embedding(r#"{"data":[]}"#, 4), Err(EmbedderError::Malformed(_))));
        assert!(matches!(parse_embedding("not json", 4), Err(EmbedderError::Malformed(_))));
    }

    #[test]
    fn cosine_of_a_unit_vector_with_itself_is_one() {
        let v = truncate_and_normalise(vec![1.0, 2.0, 3.0], 3).unwrap();
        assert!((cosine(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_request_names_the_model_and_the_text() {
        let body = request_body(&EmbedderConfig::default(), "구급차라도");
        assert!(body.contains("Qwen3-Embedding-4B"));
        assert!(body.contains("구급차라도"));
    }
}
