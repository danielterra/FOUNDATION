//! Provider-backed evaluation of ingestion extraction quality.
//!
//! Run the live-account evaluation explicitly with:
//! `cargo test --manifest-path src-tauri/Cargo.toml ingestion::eval::provider_eval --lib -- --ignored --nocapture`
//! It reads the currently configured Ontologist provider/model and does not write to the ontology.

use serde::Deserialize;
use std::collections::HashSet;

const ONTOLOGIST_IRI: &str = "foundation:SoftwareAgent_1773313705318";
const REAL_EMAIL_EXCERPT: &str = "Seu alerta de vaga para Líder técnico em: São Paulo foi criado\n\nApplied AI Engineer, Digital Natives\nOpenAI\nSão Paulo e Região\n\nStaff AI Engineer\nSerasa Experian\nSão Paulo, SP\n\nEngineering Manager – Brazil – Vaga para mulheres\nDoorDash\nSão Paulo, SP\n\nLead Software Engineer - BR\nNubank\nSão Paulo, SP\n\nGerente de Projetos de Tecnologia e Inteligência Artificial\nAccenture Brasil\nSão Paulo, SP\n\nSoftware Engineer Lead\nKyndryl\nSão Paulo, SP";

#[derive(Debug, Clone, Deserialize)]
struct JobPosting {
    title: String,
    company: String,
    location: String,
}

#[derive(Debug, Deserialize)]
struct JobAlertResult {
    jobs: Vec<JobPosting>,
}

#[derive(Debug, PartialEq)]
struct EvalScore {
    expected: usize,
    predicted: usize,
    matched: usize,
    precision: f32,
    recall: f32,
}

fn expected_jobs() -> Vec<JobPosting> {
    vec![
        job(
            "Applied AI Engineer, Digital Natives",
            "OpenAI",
            "São Paulo e Região",
        ),
        job("Staff AI Engineer", "Serasa Experian", "São Paulo, SP"),
        job(
            "Engineering Manager – Brazil – Vaga para mulheres",
            "DoorDash",
            "São Paulo, SP",
        ),
        job("Lead Software Engineer - BR", "Nubank", "São Paulo, SP"),
        job(
            "Gerente de Projetos de Tecnologia e Inteligência Artificial",
            "Accenture Brasil",
            "São Paulo, SP",
        ),
        job("Software Engineer Lead", "Kyndryl", "São Paulo, SP"),
    ]
}

fn job(title: &str, company: &str, location: &str) -> JobPosting {
    JobPosting {
        title: title.to_string(),
        company: company.to_string(),
        location: location.to_string(),
    }
}

fn score(expected: &[JobPosting], predicted: &[JobPosting]) -> EvalScore {
    let mut matched_expected = HashSet::new();
    let mut matched_predictions = 0usize;

    for prediction in predicted {
        if let Some((index, _)) = expected.iter().enumerate().find(|(index, candidate)| {
            !matched_expected.contains(index) && same_job(candidate, prediction)
        }) {
            matched_expected.insert(index);
            matched_predictions += 1;
        }
    }

    let precision = if predicted.is_empty() {
        0.0
    } else {
        matched_predictions as f32 / predicted.len() as f32
    };
    let recall = if expected.is_empty() {
        1.0
    } else {
        matched_expected.len() as f32 / expected.len() as f32
    };

    EvalScore {
        expected: expected.len(),
        predicted: predicted.len(),
        matched: matched_expected.len(),
        precision,
        recall,
    }
}

fn same_job(expected: &JobPosting, predicted: &JobPosting) -> bool {
    normalize(&expected.title) == normalize(&predicted.title)
        && normalize(&expected.company) == normalize(&predicted.company)
        && normalize(&expected.location) == normalize(&predicted.location)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .collect()
}

fn parse_result(content: &str) -> Result<JobAlertResult, String> {
    let start = content
        .find('{')
        .ok_or_else(|| "Model response did not contain a JSON object".to_string())?;
    let end = content
        .rfind('}')
        .ok_or_else(|| "Model response did not contain a complete JSON object".to_string())?;
    serde_json::from_str(&content[start..=end])
        .map_err(|error| format!("Could not decode model response as the expected JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{expected_jobs, job, parse_result, score, REAL_EMAIL_EXCERPT};

    #[test]
    fn real_email_case_passes_with_one_omission_but_no_hallucinations() {
        let expected = expected_jobs();
        let observed = expected[..5].to_vec();
        let result = score(&expected, &observed);

        assert_eq!(result.matched, 5);
        assert_eq!(result.predicted, 5);
        assert!(result.recall >= 5.0 / 6.0);
        assert_eq!(result.precision, 1.0);
    }

    #[test]
    fn invented_or_duplicated_facts_reduce_precision() {
        let expected = expected_jobs();
        let predicted = vec![
            expected[0].clone(),
            expected[0].clone(),
            job("Invented role", "Unknown", "Remote"),
        ];
        let result = score(&expected, &predicted);

        assert_eq!(result.matched, 1);
        assert!(result.precision < 1.0);
    }

    #[test]
    fn fixture_contains_only_the_sanitized_text_representation() {
        assert!(REAL_EMAIL_EXCERPT.contains("Applied AI Engineer, Digital Natives"));
        assert!(!REAL_EMAIL_EXCERPT.contains("http"));
        assert!(!REAL_EMAIL_EXCERPT.contains('@'));
    }

    #[tokio::test]
    #[ignore = "uses the configured ChatGPT subscription/model and makes one provider request"]
    async fn provider_eval_extracts_the_expected_real_email_facts() {
        use crate::ai::{ChatMessage, GenerateRequest};
        use crate::owl::{Connection, DbExecutor};

        let db_path = crate::paths::foundation_dir().join("FOUNDATION.db");
        let connection = Connection::open(&db_path)
            .unwrap_or_else(|error| panic!("Could not open local Foundation DB: {error}"));
        let executor = DbExecutor::new(connection, db_path);
        let config =
            crate::process_automation::agent_runner::resolve_agent_config_without_request_deadline(
                &executor,
                super::ONTOLOGIST_IRI,
            )
            .await
            .expect("Ontologist must have a configured provider and model");
        assert!(
            config.model_identifier.to_lowercase().contains("luna"),
            "Expected the approved Luna test model, got {}",
            config.model_identifier
        );

        let response = config
            .provider
            .generate(GenerateRequest {
                messages: vec![ChatMessage::text(
                    "user",
                    format!(
                        "Extract every job posting from this email text. Use only facts present in the message. Return only JSON with this shape: {{\"jobs\":[{{\"title\":string,\"company\":string,\"location\":string}}]}}. Do not include URLs, email addresses, tracking details, or other email boilerplate.\n\n{REAL_EMAIL_EXCERPT}"
                    ),
                )],
                max_tokens: Some(2000),
                temperature: None,
                system: Some("You are performing a factual extraction evaluation. Preserve titles and locations; do not infer missing facts.".into()),
                blackboard_context: None,
                tools: None,
                supports_web_tools: false,
                thinking: None,
                tool_choice: None,
            })
            .await
            .expect("Configured provider request should succeed");

        let output = parse_result(&response.content)
            .unwrap_or_else(|error| panic!("{error}; response could not be scored"));
        let score = score(&expected_jobs(), &output.jobs);
        eprintln!(
            "ingestion eval: model={} matched={}/{} predicted={} precision={:.2} recall={:.2}",
            response
                .model_used
                .as_deref()
                .unwrap_or(&config.model_identifier),
            score.matched,
            score.expected,
            score.predicted,
            score.precision,
            score.recall,
        );
        assert!(
            score.recall >= 5.0 / 6.0,
            "At least five of six postings must be extracted: {score:?}"
        );
        assert_eq!(
            score.precision, 1.0,
            "The model must not invent or duplicate postings: {score:?}"
        );
    }
}
