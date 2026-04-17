//! Blog article generator — converts a video transcript into a structured
//! article with title, headings, and prose paragraphs. Uses two-pass
//! consolidation for long transcripts (same pattern as summary).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use ai_kit::{CompletionRequest, Provider};
use creator_core::{AiProviderError, TranscriptionSegment};

use crate::batch::{chunk_segments, TranscriptBatch};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSection {
    pub heading: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlogArticle {
    pub language: String,
    pub title: String,
    pub sections: Vec<ArticleSection>,
}

pub fn system_prompt(language: &str) -> String {
    format!(
        "You convert video transcripts into well-structured blog articles. \
         The article should:\n\
         - Have a compelling title\n\
         - Be divided into 3-7 logical sections with clear headings\n\
         - Use proper prose paragraphs (not bullet lists unless appropriate)\n\
         - Preserve the key ideas and flow from the transcript\n\
         - Remove verbal fillers, repetitions, and spoken-language artifacts\n\
         - Read as a standalone article, not as a transcript summary\n\n\
         Respond in {language}. Base everything on the transcript content."
    )
}

pub fn user_prompt_for_batch(batch: &TranscriptBatch, language: &str) -> String {
    format!(
        "Respond in {language}. Convert this transcript into a blog article.\n\n\
         Transcript batch {} (segments {}..{}):\n\n{}",
        batch.batch_index,
        batch.first_segment_index,
        batch.first_segment_index + batch.segments.len(),
        batch.to_prompt_transcript()
    )
}

pub fn user_prompt_for_consolidation(
    partial_articles: &[String],
    language: &str,
) -> String {
    format!(
        "Below are partial article drafts from the same video transcript. \
         Merge them into a single cohesive blog article in {language}. \
         Eliminate redundancy, ensure smooth transitions, and keep the \
         structure clean.\n\n{}",
        partial_articles
            .iter()
            .enumerate()
            .map(|(i, s)| format!("--- partial {i} ---\n{s}"))
            .collect::<Vec<_>>()
            .join("\n\n")
    )
}

pub fn response_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "sections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "heading": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["heading", "content"],
                    "additionalProperties": false
                },
                "minItems": 1
            }
        },
        "required": ["title", "sections"],
        "additionalProperties": false
    })
}

#[derive(Debug, Deserialize)]
struct ArticleResponse {
    title: String,
    sections: Vec<SectionEntry>,
}

#[derive(Debug, Deserialize)]
struct SectionEntry {
    heading: String,
    content: String,
}

#[async_trait]
pub trait BlogArticleRunner {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
        max_batch_seconds: f64,
    ) -> Result<BlogArticle, AiProviderError>;
}

pub struct ProviderBlogArticleRunner<'a> {
    pub provider: &'a dyn Provider,
}

#[async_trait]
impl<'a> BlogArticleRunner for ProviderBlogArticleRunner<'a> {
    async fn run(
        &self,
        segments: &[TranscriptionSegment],
        language: &str,
        model: &str,
        max_batch_seconds: f64,
    ) -> Result<BlogArticle, AiProviderError> {
        let batches = chunk_segments(segments, max_batch_seconds);
        if batches.is_empty() {
            return Ok(BlogArticle {
                language: language.into(),
                title: String::new(),
                sections: vec![],
            });
        }

        let mut partials = Vec::with_capacity(batches.len());
        for batch in &batches {
            let req = CompletionRequest::structured(
                model,
                system_prompt(language),
                user_prompt_for_batch(batch, language),
                "BatchArticle",
                response_schema(),
            );
            let v = self.provider.complete(req).await?;
            let parsed: ArticleResponse =
                serde_json::from_value(v).map_err(|e| AiProviderError::Malformed(e.to_string()))?;
            partials.push(parsed);
        }

        if partials.len() == 1 {
            let p = partials.into_iter().next().unwrap();
            return Ok(to_blog_article(p, language));
        }

        let md_parts: Vec<String> = partials.iter().map(format_as_markdown).collect();
        let req = CompletionRequest::structured(
            model,
            system_prompt(language),
            user_prompt_for_consolidation(&md_parts, language),
            "FinalArticle",
            response_schema(),
        );
        let v = self.provider.complete(req).await?;
        let final_parsed: ArticleResponse =
            serde_json::from_value(v).map_err(|e| AiProviderError::Malformed(e.to_string()))?;

        Ok(to_blog_article(final_parsed, language))
    }
}

fn to_blog_article(resp: ArticleResponse, language: &str) -> BlogArticle {
    BlogArticle {
        language: language.into(),
        title: resp.title,
        sections: resp
            .sections
            .into_iter()
            .map(|s| ArticleSection {
                heading: s.heading,
                content: s.content,
            })
            .collect(),
    }
}

fn format_as_markdown(article: &ArticleResponse) -> String {
    let mut out = format!("# {}\n\n", article.title);
    for s in &article.sections {
        out.push_str(&format!("## {}\n\n{}\n\n", s.heading, s.content));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_strict_at_all_levels() {
        let s = response_schema();
        assert_eq!(s["additionalProperties"], false);
        let item = &s["properties"]["sections"]["items"];
        assert_eq!(item["additionalProperties"], false);
    }

    #[test]
    fn schema_requires_title_and_sections() {
        let s = response_schema();
        let req = s["required"].as_array().unwrap();
        assert!(req.iter().any(|v| v == "title"));
        assert!(req.iter().any(|v| v == "sections"));
    }
}
